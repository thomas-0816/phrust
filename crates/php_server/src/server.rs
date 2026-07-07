use crate::{
    access_log::AccessLogger,
    config::{ConfigError, ServerConfig},
    metrics::ServerMetrics,
    multipart::MultipartConfig,
    routing::RouteConfig,
    serve::serve_until_shutdown,
    session_store::SessionStore,
    state::{AppState, ServerEngineState, SessionConfig, preload_script_cache},
    tls::build_tls_acceptor,
};
use php_diagnostics::{
    DiagnosticCause, DiagnosticEnvelope, DiagnosticLayer, DiagnosticPhase, DiagnosticSeverity,
    DiagnosticSuggestion,
};
use php_executor::{CompiledScriptCache, IncludeCache};
use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};
use tokio::{net::TcpListener, sync::Semaphore};
use tracing::debug;

#[derive(Debug)]
pub enum ServerError {
    Config(Box<ConfigError>),
    Io(std::io::Error),
    Preload(String),
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
            Self::Preload(message) => {
                let mut diagnostic = DiagnosticEnvelope::new(
                    "E_PHRUST_SERVER_PRELOAD",
                    DiagnosticLayer::server(),
                    DiagnosticPhase::new("preload"),
                    DiagnosticSeverity::Error,
                    message.clone(),
                );
                diagnostic.suggestion = Some(DiagnosticSuggestion::new(
                    "fix the preload file entry or run without --strict-preload",
                ));
                diagnostic
            }
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
    let listener = TcpListener::bind(config.listen).await?;
    let local_addr = listener.local_addr()?;
    let script_cache_preload = config.script_cache_preload.clone();
    let strict_preload = config.strict_preload;
    let startup_front_controller = config.front_controller.clone();
    let startup_upload_temp_dir = config.upload_temp_dir.clone();
    let startup_session_save_path = config.session_save_path.clone();
    let startup_script_cache_enabled = config.script_cache_enabled;
    let startup_script_cache_shards = config.script_cache_shards;
    let startup_script_cache_max_entries = config.script_cache_max_entries;
    let startup_metrics_endpoint_enabled = config.metrics_endpoint_enabled;
    let startup_metrics_token_enabled = config.metrics_token.is_some();
    let startup_access_log = config.access_log.clone();
    let startup_perf_trace = config.perf_trace.clone();
    let startup_request_profile = config.request_profile.clone();
    let startup_tls_enabled = config.tls_cert.is_some();
    let engine_profile = config.engine_preset;
    let tls_acceptor = build_tls_acceptor(config.tls_cert.as_deref(), config.tls_key.as_deref())?;
    let access_log = config
        .access_log
        .as_deref()
        .map(AccessLogger::open)
        .transpose()?
        .map(Arc::new);
    let perf_trace = config
        .perf_trace
        .map(crate::perf_trace::PerfTraceWriter::open)
        .transpose()?
        .map(Arc::new);
    let request_profile = config
        .request_profile
        .map(crate::request_profile::RequestProfileWriter::open)
        .transpose()?
        .map(Arc::new);
    let session_store = Arc::new(SessionStore::new(config.session_save_path));
    if config.sessions_enabled {
        session_store
            .ensure_ready()
            .map_err(std::io::Error::other)?;
    }
    let startup_scheme = if startup_tls_enabled { "https" } else { "http" };
    println!("listening {startup_scheme}://{local_addr}");
    eprintln!(
        "startup docroot={} front_controller={} engine_preset={} script_cache={} script_cache_shards={} script_cache_max_entries={} upload_temp_dir={} session_save_path={} metrics_endpoint={} metrics_token={} access_log={} perf_trace={} request_profile={} tls={} tls_alpn={}",
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
        if startup_tls_enabled { "http/1.1" } else { "-" },
    );
    debug!(%local_addr, docroot=%docroot.display(), "starting phrust server");
    let script_cache = Arc::new(if config.script_cache_enabled {
        CompiledScriptCache::new_with_limits(
            config.script_cache_shards,
            config.script_cache_max_entries,
            Duration::from_millis(config.script_cache_check_interval_ms),
        )
    } else {
        CompiledScriptCache::disabled()
    });
    let include_cache = Arc::new(IncludeCache::new(config.script_cache_shards));
    let engine = Arc::new(ServerEngineState::new(
        engine_profile,
        config.max_vm_steps,
        script_cache,
        include_cache,
        config.dense_includes,
        config.perf_ablation,
    ));
    let state = Arc::new(AppState {
        route_config: RouteConfig {
            docroot,
            index: config.index,
            front_controller: config.front_controller,
            builtin_router: config.builtin_router,
            request_rewrites: config.request_rewrites,
            metrics_endpoint_enabled: config.metrics_endpoint_enabled,
            cache_clear_endpoint_enabled: config.cache_clear_endpoint_enabled,
        },
        max_body_bytes: config.max_body_bytes,
        multipart_config: MultipartConfig {
            upload_temp_dir: config.upload_temp_dir,
            max_upload_files: config.max_upload_files,
            max_upload_file_bytes: config.max_upload_file_bytes,
        },
        request_timeout: Duration::from_millis(config.request_timeout_ms),
        execution_time_limit: config
            .execution_deadline_enabled
            .then(|| Duration::from_millis(config.max_execution_ms)),
        in_flight: Arc::new(Semaphore::new(config.max_in_flight)),
        max_in_flight: config.max_in_flight,
        metrics: Arc::new(ServerMetrics::default()),
        engine,
        metrics_token: config.metrics_token,
        access_log,
        perf_trace,
        perf_trace_vm_counters: config.perf_trace_vm_counters,
        request_profile,
        request_profile_vm_counters: config.request_profile_vm_counters,
        request_profile_source_attribution: config.request_profile_source_attribution,
        request_profile_trigger_header: config.request_profile_trigger_header,
        network_requests_enabled: config.network_requests_enabled,
        env_snapshot: Arc::new(std::env::vars().collect()),
        debug: config.debug,
        error_format: config.error_format,
        debug_log: config.debug_log,
        request_counter: Arc::new(AtomicU64::new(0)),
        session_config: SessionConfig {
            enabled: config.sessions_enabled,
            cookie_name: config.session_cookie_name,
            cookie_path: config.session_cookie_path,
        },
        session_store,
        local_addr,
        request_scheme: if startup_tls_enabled { "https" } else { "http" },
    });
    preload_script_cache(&state, script_cache_preload.as_deref(), strict_preload)?;
    serve_until_shutdown(listener, state, tls_acceptor).await;
    Ok(())
}

pub fn run_blocking(config: ServerConfig) -> Result<(), ServerError> {
    #[cfg(not(target_os = "wasi"))]
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(ServerError::Io)?;
    #[cfg(target_os = "wasi")]
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(ServerError::Io)?;
    runtime.block_on(run(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::ServerPerfAblation,
        perf_trace::{PerfTraceEvent, PerfTraceWriter},
        php_request::{
            RequestCounterMode, append_vm_counters_to_trace, execute_compiled_php_with_state,
            http_runtime_context, php_runtime_context_for_http, request_counter_mode,
            server_env_for_request,
        },
        request_profile::RequestProfileWriter,
        routing::RequestRewriteRule,
        serve::clear_cache_response,
        static_files::{
            ByteRange, RangeParseError, accepts_encoding, parse_single_byte_range, weak_etag,
        },
        tls::{load_tls_certs, load_tls_private_key},
    };
    use hyper::{
        Request, StatusCode, header,
        http::{HeaderMap, HeaderValue},
    };
    use php_diagnostics::DiagnosticOutputFormat;
    use php_executor::{EngineProfileName, IncludeLoader, OptimizationLevel, PhpExecutionStatus};
    use php_runtime::api::{RuntimeContext, RuntimeHttpRequestContext, SessionState};
    use php_vm::api::{
        DenseIncludeMode, DenseJumpThreadingMode, InlineCacheMode, JitMode, QuickeningMode,
    };
    use std::{
        path::PathBuf,
        sync::atomic::Ordering,
        time::{SystemTime, UNIX_EPOCH},
    };

    static SERVER_CACHE_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn parses_single_byte_ranges() {
        assert_eq!(
            parse_single_byte_range("bytes=0-4", 10),
            Ok(ByteRange { start: 0, end: 4 })
        );
        assert_eq!(
            parse_single_byte_range("bytes=4-", 10),
            Ok(ByteRange { start: 4, end: 9 })
        );
        assert_eq!(
            parse_single_byte_range("bytes=-3", 10),
            Ok(ByteRange { start: 7, end: 9 })
        );
        assert_eq!(
            parse_single_byte_range("bytes=8-99", 10),
            Ok(ByteRange { start: 8, end: 9 })
        );
        assert_eq!(
            parse_single_byte_range("bytes=10-12", 10),
            Err(RangeParseError::Unsatisfiable)
        );
        assert_eq!(
            parse_single_byte_range("bytes=3-1", 10),
            Err(RangeParseError::Invalid)
        );
        assert_eq!(
            parse_single_byte_range("bytes=0-1,3-4", 10),
            Err(RangeParseError::Unsatisfiable)
        );
    }

    #[test]
    fn weak_etag_is_deterministic_for_file_metadata() {
        let fixture = ServerCacheFixture::new();
        fixture.write("static bytes\n");
        let metadata = std::fs::metadata(&fixture.path).expect("fixture metadata");

        let first = weak_etag(&metadata);
        let second = weak_etag(&metadata);

        assert_eq!(first, second);
        assert!(first.starts_with("W/\""));
        assert!(first.ends_with('"'));
        assert!(first.contains('d'));
    }

    #[test]
    fn accepts_encoding_parses_comma_separated_tokens() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_static("gzip;q=1, br, zstd;q=0.8"),
        );

        assert!(accepts_encoding(&headers, "br"));
        assert!(accepts_encoding(&headers, "gzip"));
        assert!(accepts_encoding(&headers, "zstd"));
        assert!(!accepts_encoding(&headers, "deflate"));

        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_static("gzip;q=0"),
        );
        assert!(!accepts_encoding(&headers, "gzip"));
    }

    #[test]
    fn app_state_cache_records_hit_after_repeated_compile() {
        let fixture = ServerCacheFixture::new();
        fixture.write("<?php echo \"cached\";");
        let cache = Arc::new(CompiledScriptCache::new(1));
        let state = test_state(&fixture, Arc::clone(&cache), false);

        let first = state
            .compile_script(&fixture.path)
            .expect("first server compile");
        let second = state
            .compile_script(&fixture.path)
            .expect("second server compile");

        assert!(!first.hit);
        assert!(second.hit);
        assert_eq!(cache.cache_stats().hits, 1);
        assert_eq!(cache.cache_stats().misses, 1);
    }

    #[test]
    fn app_state_default_engine_runs_managed_fast_paths_with_counters() {
        let fixture = ServerCacheFixture::new();
        fixture.write(managed_fast_counter_source());
        let cache = Arc::new(CompiledScriptCache::new(1));
        let state = test_state(&fixture, cache, false);
        let lookup = state
            .compile_script(&fixture.path)
            .expect("server compile should use managed defaults");
        let request = RuntimeHttpRequestContext::new(
            "GET",
            "localhost",
            "/index.php",
            "/index.php",
            fixture.path.to_string_lossy().into_owned(),
            fixture.root.to_string_lossy().into_owned(),
        );
        let runtime_context = RuntimeContext::controlled_http(request)
            .with_cwd(state.route_config.docroot.clone())
            .with_include_path(vec![state.route_config.docroot.clone()]);

        let output = execute_compiled_php_with_state(
            &state,
            lookup,
            fixture.path.clone(),
            runtime_context,
            RequestCounterMode::VmCounters,
            false,
        )
        .expect("server execution should succeed");

        assert_eq!(output.status, PhpExecutionStatus::Success);
        assert_eq!(output.stdout, b"123512351235");
        let counters = output.counters.expect("counters should be collected");
        assert_eq!(counters.jit_mode, "cranelift");
        assert_eq!(counters.native_executions, counters.jit_executed);
        assert!(counters.bytecode_lower_attempts > 0, "{counters:?}");
        assert!(counters.quickening_attempts > 0, "{counters:?}");
        assert!(counters.inline_cache_observations > 0, "{counters:?}");

        let mut trace = PerfTraceEvent::default();
        append_vm_counters_to_trace(&mut trace, Some(&counters));
        let traced_counters = trace.counters.iter().copied().collect::<BTreeMap<_, _>>();
        // Dense-plan threading can execute the whole request as dense
        // bytecode, so instruction evidence is tier-agnostic.
        let executed_instructions = traced_counters
            .get("vm_instructions_executed")
            .copied()
            .unwrap_or_default()
            + traced_counters
                .get("vm_bytecode_instructions_executed")
                .copied()
                .unwrap_or_default();
        assert!(executed_instructions > 0, "{traced_counters:?}");
        assert!(
            traced_counters
                .get("vm_function_calls")
                .is_some_and(|value| *value > 0),
            "{traced_counters:?}"
        );
        assert!(
            traced_counters
                .get("vm_inline_cache_hits")
                .is_some_and(|value| *value > 0),
            "{traced_counters:?}"
        );
    }

    #[test]
    fn engine_perf_ablation_disables_selected_fast_paths() {
        let ablation = ServerPerfAblation {
            disable_dense_includes: true,
            disable_quickening: true,
            disable_inline_caches: true,
            disable_builtin_ic: true,
            disable_jit: true,
            disable_include_o2: true,
            disable_dense_jump_threading: true,
        };
        let engine = ServerEngineState::new(
            EngineProfileName::Default,
            100_000,
            Arc::new(CompiledScriptCache::new(1)),
            Arc::new(IncludeCache::new(1)),
            Some(DenseIncludeMode::Auto),
            ablation,
        );

        let options = engine.executor_options();

        assert_eq!(engine.compile_optimization_level, OptimizationLevel::O0);
        assert_eq!(
            options.vm_options.dense_include_execution,
            DenseIncludeMode::Off
        );
        assert_eq!(options.vm_options.quickening, QuickeningMode::Off);
        assert_eq!(options.vm_options.inline_caches, InlineCacheMode::Off);
        assert!(!options.vm_options.internal_function_dispatch_cache);
        assert_eq!(options.vm_options.jit, JitMode::Off);
        assert!(!options.vm_options.tiering.enabled);
        assert_eq!(
            options.vm_options.include_optimization_level,
            OptimizationLevel::O0
        );
        assert_eq!(
            options.vm_options.dense_jump_threading,
            DenseJumpThreadingMode::Off
        );
    }

    #[test]
    fn request_counter_modes_select_by_configuration() {
        let fixture = ServerCacheFixture::new();
        let plain_state = test_state(&fixture, Arc::new(CompiledScriptCache::new(1)), false);
        assert_eq!(request_counter_mode(&plain_state), RequestCounterMode::Off);

        let mut traced_state = test_state(&fixture, Arc::new(CompiledScriptCache::new(1)), false);
        traced_state.perf_trace = Some(Arc::new(
            PerfTraceWriter::open(fixture.root.join("perf.jsonl")).expect("perf trace"),
        ));
        assert_eq!(request_counter_mode(&traced_state), RequestCounterMode::Off);

        traced_state.perf_trace_vm_counters = true;
        assert_eq!(
            request_counter_mode(&traced_state),
            RequestCounterMode::VmCounters
        );

        // A request profile alone stays in Summary: no VM hot counters and
        // no per-clone source attribution for ordinary profiled requests.
        let mut profiled_state = test_state(&fixture, Arc::new(CompiledScriptCache::new(1)), false);
        profiled_state.request_profile = Some(Arc::new(
            RequestProfileWriter::open(fixture.root.join("profiles")).expect("request profile"),
        ));
        assert_eq!(
            request_counter_mode(&profiled_state),
            RequestCounterMode::Summary
        );
        assert!(!request_counter_mode(&profiled_state).collects_vm_counters());

        profiled_state.request_profile_vm_counters = true;
        assert_eq!(
            request_counter_mode(&profiled_state),
            RequestCounterMode::VmCounters
        );
        assert!(!request_counter_mode(&profiled_state).collects_source_attribution());

        profiled_state.request_profile_source_attribution = true;
        assert_eq!(
            request_counter_mode(&profiled_state),
            RequestCounterMode::SourceAttributedLayout
        );
        assert!(request_counter_mode(&profiled_state).collects_vm_counters());
        assert!(request_counter_mode(&profiled_state).collects_source_attribution());
    }

    #[test]
    fn persistent_engine_shares_immutable_artifacts_without_request_state() {
        let fixture = ServerCacheFixture::new();
        fixture.write(request_isolation_source());
        let cache = Arc::new(CompiledScriptCache::new_with_limits(2, 8, Duration::ZERO));
        let state = Arc::new(test_state(&fixture, Arc::clone(&cache), false));
        let initial = state
            .compile_script(&fixture.path)
            .expect("initial compile");
        assert!(!initial.hit);

        let barrier = Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for id in ["one", "two"] {
            let state = Arc::clone(&state);
            let fixture_path = fixture.path.clone();
            let fixture_root = fixture.root.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                let lookup = state.compile_script(&fixture_path).expect("cached compile");
                assert!(lookup.hit);
                barrier.wait();
                let request = RuntimeHttpRequestContext::new(
                    "GET",
                    "localhost",
                    format!("/index.php?id={id}"),
                    "/index.php",
                    fixture_path.to_string_lossy().into_owned(),
                    fixture_root.to_string_lossy().into_owned(),
                );
                let runtime_context = RuntimeContext::controlled_http(request)
                    .with_cwd(fixture_root.clone())
                    .with_include_path(vec![fixture_root]);
                let output = execute_compiled_php_with_state(
                    &state,
                    lookup,
                    fixture_path,
                    runtime_context,
                    RequestCounterMode::Off,
                    false,
                )
                .expect("request execution");
                assert_eq!(output.status, PhpExecutionStatus::Success);
                let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
                let isolated_header = output.http_response.headers.iter().any(|header| {
                    header.name.eq_ignore_ascii_case("x-isolated") && header.value == id
                });
                (id.to_string(), stdout, isolated_header)
            }));
        }

        for handle in handles {
            let (id, stdout, isolated_header) = handle.join().expect("request thread");
            assert!(
                stdout.contains(&format!("id={id}|global=1|buf=buffer|cow=1:2|ref=2")),
                "{stdout}"
            );
            assert!(stdout.contains("|destruct"), "{stdout}");
            assert!(isolated_header);
        }

        let stats = cache.cache_stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.entries, 1);
        assert_eq!(
            state
                .metrics
                .persistent_engine_request_local_resets
                .load(Ordering::Relaxed),
            2
        );
        // Each request rebuilds request-local engine state; the rejected
        // persistence stays visible instead of being renamed away.
        assert_eq!(
            state
                .metrics
                .persistent_engine_request_local_rejections
                .load(Ordering::Relaxed),
            2
        );
        assert_eq!(
            state
                .metrics
                .persistent_engine_immutable_metadata_reuses
                .load(Ordering::Relaxed),
            2
        );
    }

    #[test]
    fn preload_script_cache_compiles_entries_and_reports_metrics() {
        let fixture = ServerCacheFixture::new();
        let first = fixture.write_named("first.php", "<?php echo \"first\";");
        let second = fixture.write_named("second.php", "<?php echo \"second\";");
        let preload = fixture.root.join("preload.txt");
        std::fs::write(&preload, "first.php\nsecond.php\n").expect("write preload list");
        let cache = Arc::new(CompiledScriptCache::new_with_limits(2, 8, Duration::ZERO));
        let state = test_state(&fixture, Arc::clone(&cache), false);

        preload_script_cache(&state, Some(&preload), true).expect("preload scripts");

        let stats = cache.cache_stats();
        assert_eq!(stats.entries, 2);
        let include_stats = state.engine.include_cache.cache_stats();
        assert_eq!(include_stats.compile_misses, 2);
        assert_eq!(include_stats.source_reads, 2);
        assert_eq!(
            state
                .metrics
                .script_cache_preload_successes
                .load(Ordering::Relaxed),
            2
        );
        assert_eq!(
            state
                .metrics
                .script_cache_preload_failures
                .load(Ordering::Relaxed),
            0
        );
        assert!(state.compile_script(&first).expect("first cached").hit);
        assert!(state.compile_script(&second).expect("second cached").hit);

        let loader = IncludeLoader::for_root(&fixture.root).expect("include loader");
        let resolved = state
            .engine
            .include_cache
            .resolve_with_include_path(&loader, None, "first.php", &[], Some(&fixture.root))
            .expect("resolve preloaded include");
        state
            .engine
            .include_cache
            .get_or_compile_include(&loader, &resolved, state.engine.compile_optimization_level)
            .expect("preloaded include cache hit");
        let include_stats_after_hit = state.engine.include_cache.cache_stats();
        assert_eq!(include_stats_after_hit.compile_hits, 1);
        assert_eq!(
            include_stats_after_hit.source_reads,
            include_stats.source_reads
        );
    }

    #[test]
    fn clear_cache_response_removes_script_cache_entries() {
        let fixture = ServerCacheFixture::new();
        fixture.write("<?php echo \"clear\";");
        let cache = Arc::new(CompiledScriptCache::new(1));
        let state = test_state(&fixture, Arc::clone(&cache), true);
        state
            .compile_script(&fixture.path)
            .expect("compile before clear");
        assert_eq!(cache.cache_stats().entries, 1);

        let response =
            clear_cache_response(&state, "127.0.0.1:45000".parse().expect("loopback peer"));

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(cache.cache_stats().entries, 0);
    }

    #[test]
    fn clear_cache_response_rejects_non_loopback_peers() {
        let fixture = ServerCacheFixture::new();
        let cache = Arc::new(CompiledScriptCache::new(1));
        let state = test_state(&fixture, cache, true);

        let response = clear_cache_response(
            &state,
            "192.0.2.10:45000".parse().expect("non-loopback peer"),
        );

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn http_php_runtime_context_includes_server_process_env() {
        let fixture = ServerCacheFixture::new();
        let state = test_state(&fixture, Arc::new(CompiledScriptCache::new(1)), false);
        let request_context = RuntimeHttpRequestContext::new(
            "GET",
            "example.test",
            "/index.php",
            "/index.php",
            fixture.path.to_string_lossy().into_owned(),
            fixture.root.to_string_lossy().into_owned(),
        );

        let context = php_runtime_context_for_http(
            &state,
            request_context,
            SessionState::default(),
            Arc::from(&b"request-body"[..]),
            vec![
                (
                    "PHRUST_MYSQL_TEST_DSN".to_string(),
                    "mysql://app:secret@mariadb:3306/app".to_string(),
                ),
                ("APP_DB_HOST".to_string(), "mariadb:3306".to_string()),
            ],
        );

        assert_eq!(
            context
                .env
                .iter()
                .find(|(key, _)| key == "PHRUST_MYSQL_TEST_DSN")
                .map(|(_, value)| value.as_str()),
            Some("mysql://app:secret@mariadb:3306/app")
        );
        assert_eq!(context.stdin.as_ref(), b"request-body");
    }

    #[test]
    fn server_env_for_request_injects_network_capability_when_enabled() {
        let fixture = ServerCacheFixture::new();
        let mut state = test_state(&fixture, Arc::new(CompiledScriptCache::new(1)), false);
        state.network_requests_enabled = true;

        let env = server_env_for_request(&state);

        assert_eq!(
            env.iter()
                .find(|(key, _)| key == "PHRUST_NET_TESTS")
                .map(|(_, value)| value.as_str()),
            Some("1")
        );
    }

    #[test]
    fn server_env_for_request_hides_rewrite_configuration() {
        let fixture = ServerCacheFixture::new();
        let mut state = test_state(&fixture, Arc::new(CompiledScriptCache::new(1)), false);
        state.env_snapshot = Arc::new(vec![(
            crate::config::BUILTIN_SERVER_REWRITE_PREFIX_QUERY_ENV.to_string(),
            "/api=route".to_string(),
        )]);

        let env = server_env_for_request(&state);

        assert!(
            env.iter()
                .all(|(key, _)| key != crate::config::BUILTIN_SERVER_REWRITE_PREFIX_QUERY_ENV),
            "{env:?}"
        );
    }

    #[test]
    fn server_env_for_request_preserves_existing_network_capability_value() {
        let fixture = ServerCacheFixture::new();
        let mut state = test_state(&fixture, Arc::new(CompiledScriptCache::new(1)), false);
        state.network_requests_enabled = true;
        state.env_snapshot = Arc::new(vec![("PHRUST_NET_TESTS".to_string(), "0".to_string())]);

        let env = server_env_for_request(&state);

        assert_eq!(
            env.iter()
                .filter(|(key, _)| key == "PHRUST_NET_TESTS")
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>(),
            vec!["0"]
        );
    }

    #[test]
    fn http_runtime_context_maps_server_name_https_and_remote_addr() {
        let fixture = ServerCacheFixture::new();
        let mut state = test_state(&fixture, Arc::new(CompiledScriptCache::new(1)), false);
        state.request_scheme = "https";
        state.local_addr = "127.0.0.1:8443".parse().expect("local addr");
        let script_path = fixture.write_named("index.php", "<?php echo 'ok';");
        let (parts, _) = Request::builder()
            .method("GET")
            .uri("/index.php?name=phrust")
            .header(header::HOST, "example.test:8443")
            .body(())
            .expect("request")
            .into_parts();

        let context = http_runtime_context(
            &parts,
            &state,
            &script_path,
            "/index.php",
            None,
            Arc::from(&b""[..]),
            "192.0.2.44:50123".parse().expect("peer addr"),
        );

        assert_eq!(context.scheme, "https");
        assert_eq!(context.host, "example.test:8443");
        assert_eq!(context.server_name, "example.test");
        assert_eq!(context.server_port, 8443);
        assert!(context.https);
        assert_eq!(context.remote_addr, "192.0.2.44");
        assert_eq!(context.request_uri, "/index.php?name=phrust");
        assert!(context.request_time > 0);
        assert!(context.request_time_float_micros >= context.request_time * 1_000_000);
    }

    #[test]
    fn http_runtime_context_applies_configured_prefix_query_rewrite() {
        let fixture = ServerCacheFixture::new();
        let mut state = test_state(&fixture, Arc::new(CompiledScriptCache::new(1)), false);
        state.local_addr = "127.0.0.1:18080".parse().expect("local addr");
        state
            .route_config
            .request_rewrites
            .push(RequestRewriteRule {
                path_prefix: "/api".to_string(),
                query_parameter: "route".to_string(),
            });
        let script_path = fixture.write_named("index.php", "<?php echo 'ok';");
        let (parts, _) = Request::builder()
            .method("GET")
            .uri("/api/v1/types?context=edit")
            .header(header::HOST, "127.0.0.1:18080")
            .body(())
            .expect("request")
            .into_parts();

        let context = http_runtime_context(
            &parts,
            &state,
            &script_path,
            "/index.php",
            Some("/api/v1/types".to_string()),
            Arc::from(&b""[..]),
            "127.0.0.1:50123".parse().expect("peer addr"),
        );

        assert_eq!(context.request_uri, "/?route=%2Fv1%2Ftypes&context=edit");
        assert_eq!(context.query_string, "route=%2Fv1%2Ftypes&context=edit");
        assert_eq!(
            context.parsed_get,
            vec![
                ("route".to_string(), "/v1/types".to_string()),
                ("context".to_string(), "edit".to_string()),
            ]
        );
        assert_eq!(context.path_info.as_deref(), Some("/api/v1/types"));
    }

    #[test]
    fn tls_fixture_cert_and_key_load() {
        let cert = tls_fixture("localhost.crt");
        let key = tls_fixture("localhost.key");

        assert_eq!(load_tls_certs(&cert).expect("load cert").len(), 1);
        assert!(load_tls_private_key(&key).is_ok());
        assert!(
            build_tls_acceptor(Some(&cert), Some(&key))
                .expect("build acceptor")
                .is_some()
        );
    }

    fn test_state(
        fixture: &ServerCacheFixture,
        cache: Arc<CompiledScriptCache>,
        cache_clear_endpoint_enabled: bool,
    ) -> AppState {
        AppState {
            route_config: RouteConfig {
                docroot: fixture.root.clone(),
                index: "index.php".to_string(),
                front_controller: None,
                builtin_router: None,
                request_rewrites: Vec::new(),
                metrics_endpoint_enabled: true,
                cache_clear_endpoint_enabled,
            },
            max_body_bytes: 1024,
            multipart_config: MultipartConfig {
                upload_temp_dir: fixture.root.join("uploads"),
                max_upload_files: 32,
                max_upload_file_bytes: 1024,
            },
            request_timeout: Duration::from_secs(30),
            execution_time_limit: Some(Duration::from_secs(30)),
            in_flight: Arc::new(Semaphore::new(1)),
            max_in_flight: 1,
            metrics: Arc::new(ServerMetrics::default()),
            engine: Arc::new(ServerEngineState::new(
                EngineProfileName::Default,
                100_000,
                cache,
                Arc::new(IncludeCache::new(1)),
                None,
                Default::default(),
            )),
            metrics_token: None,
            access_log: None,
            perf_trace: None,
            perf_trace_vm_counters: false,
            request_profile: None,
            request_profile_vm_counters: false,
            request_profile_source_attribution: false,
            request_profile_trigger_header: false,
            network_requests_enabled: false,
            env_snapshot: Arc::new(Vec::new()),
            debug: false,
            error_format: DiagnosticOutputFormat::Text,
            debug_log: None,
            request_counter: Arc::new(AtomicU64::new(0)),
            session_config: SessionConfig {
                enabled: false,
                cookie_name: "PHPSESSID".to_string(),
                cookie_path: "/".to_string(),
            },
            session_store: Arc::new(SessionStore::new(fixture.root.join("sessions"))),
            local_addr: "127.0.0.1:8080".parse().expect("local addr"),
            request_scheme: "http",
        }
    }

    fn managed_fast_counter_source() -> &'static str {
        "<?php\n\
         function ic_f() { return 1; }\n\
         class ICSlotSmoke {\n\
             public $x = 3;\n\
             public function m() { return 2; }\n\
         }\n\
         $object = new ICSlotSmoke();\n\
         $items = [4, 5];\n\
         for ($i = 0; $i < 3; $i = $i + 1) {\n\
             echo ic_f(), $object->m(), $object->x, $items[1];\n\
         }\n"
    }

    fn request_isolation_source() -> &'static str {
        "<?php\n\
         $GLOBALS['counter'] = ($GLOBALS['counter'] ?? 0) + 1;\n\
         header('X-Isolated: ' . $_GET['id']);\n\
         ob_start();\n\
         echo 'buffer';\n\
         $buffer = ob_get_clean();\n\
         $items = [1];\n\
         $copy = $items;\n\
         $copy[] = 2;\n\
         $ref = 1;\n\
         $alias =& $ref;\n\
         $alias = $alias + 1;\n\
         class ServerIsolationDestructor {\n\
             public function __destruct() { echo '|destruct'; }\n\
         }\n\
         $object = new ServerIsolationDestructor();\n\
         echo 'id=', $_GET['id'], '|global=', $GLOBALS['counter'], '|buf=', $buffer, '|cow=', count($items), ':', count($copy), '|ref=', $ref;\n"
    }

    struct ServerCacheFixture {
        root: PathBuf,
        path: PathBuf,
    }

    impl ServerCacheFixture {
        fn new() -> Self {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let unique = SERVER_CACHE_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "phrust-server-cache-{}-{timestamp}-{unique}",
                std::process::id(),
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir(&root).expect("create server cache fixture");
            let path = root.join("index.php");
            Self { root, path }
        }

        fn write(&self, source: &str) {
            std::fs::write(&self.path, source).expect("write server cache fixture");
        }

        fn write_named(&self, relative: &str, source: &str) -> PathBuf {
            let path = self.root.join(relative);
            std::fs::write(&path, source).expect("write server cache fixture");
            path
        }
    }

    impl Drop for ServerCacheFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn tls_fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures/server/tls")
            .join(name)
            .canonicalize()
            .expect("tls fixture")
    }
}
