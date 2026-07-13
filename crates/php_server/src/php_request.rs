use super::{
    diagnostics::{RequestDiagnostic, emit_request_diagnostic, emit_server_debug_lazy},
    metrics::RequestPhase,
    perf_trace::PerfTraceEvent,
    request_pipeline::{PhpResponseBytes, RequestCleanup, RequestOutcome, RequestStage},
    sessions::seed_session_state,
    state::{AppState, RequestExecutorCacheKey},
};
use crate::{
    multipart::{MultipartError, MultipartStats, multipart_boundary, parse_multipart_into_context},
    response::{self, RequestBody, ResponseBody},
    routing::RequestRewriteRule,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::{
    Method, Response, StatusCode, Version,
    header::{self, HeaderName, HeaderValue},
    http::{HeaderMap, request::Parts},
};
use php_executor::{
    CompiledPhpScript, CompiledScriptCacheLookup, PhpExecutionError, PhpExecutionOutput,
    PhpExecutionStatus, PhpExecutor, PhpRequestExecutionInput,
};
use php_runtime::api::{
    RuntimeContext, RuntimeHttpRequestContext, RuntimeHttpResponseState, SessionIdGenerateCallback,
    SessionLoadCallback, SessionState, Value, parse_cookie_header, parse_form_urlencoded_body,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{OwnedSemaphorePermit, TryAcquireError},
    time::timeout,
};
use tracing::{debug, warn};

pub(crate) struct PartsAndBody {
    pub(crate) parts: Parts,
    pub(crate) body: RequestBody,
}

thread_local! {
    static REQUEST_EXECUTOR_CACHE: RefCell<Option<CachedRequestExecutor>> = const { RefCell::new(None) };
}

struct CachedRequestExecutor {
    key: RequestExecutorCacheKey,
    executor: PhpExecutor,
}

struct RequestTarget {
    script_path: PathBuf,
    path_info: Option<String>,
}

fn route_target_selection_stage(script_path: PathBuf, path_info: Option<String>) -> RequestTarget {
    RequestTarget {
        script_path,
        path_info,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RequestLocalAddr(pub(crate) SocketAddr);

pub(crate) async fn execute_php_request(
    request: PartsAndBody,
    state: Arc<AppState>,
    script_path: PathBuf,
    path_info: Option<String>,
    peer: SocketAddr,
    request_id: String,
    route_resolution: Duration,
) -> (Response<ResponseBody>, Option<bool>) {
    let RequestTarget {
        script_path,
        path_info,
    } = route_target_selection_stage(script_path, path_info);
    let PartsAndBody { parts, body } = request;
    // Trace events only reach the perf-trace/request-profile writers; skip
    // the per-request string clones and phase vector entirely when neither
    // consumer is configured.
    let collect_request_trace =
        state.observability.perf_trace.is_some() || state.observability.request_profile.is_some();
    let mut trace = collect_request_trace.then(|| PerfTraceEvent {
        request_id: request_id.clone(),
        method: parts.method.to_string(),
        path: parts
            .uri
            .path_and_query()
            .map_or_else(|| parts.uri.path().to_string(), |value| value.to_string()),
        script_path: script_path.display().to_string(),
        phases: vec![("route_resolution", route_resolution.as_nanos())],
        ..PerfTraceEvent::default()
    });
    emit_server_debug_lazy(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_BODY_READ_START",
        "body_read",
        "request body read started",
        || {
            BTreeMap::from([(
                "max_body_bytes".to_string(),
                state.request.max_body_bytes.to_string(),
            )])
        },
    );
    let body_started = Instant::now();
    let body = match body_and_multipart_stage(&state, body).await {
        Err(_) => {
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_BODY_READ_TIMEOUT",
                "body_read",
                "request body read timed out",
                || {
                    BTreeMap::from([(
                        "timeout_ms".to_string(),
                        state.request.request_timeout.as_millis().to_string(),
                    )])
                },
            );
            record_phase(
                &state,
                &mut trace,
                RequestPhase::BodyRead,
                "body_read",
                body_started.elapsed(),
            );
            let response = response::text(StatusCode::REQUEST_TIMEOUT, "request timeout\n");
            return finish_php_request(
                &state,
                trace,
                response,
                None,
                Some(RequestStage::BodyAndMultipart),
            );
        }
        Ok(Ok(body)) => body,
        Ok(Err(BodyReadError::TooLarge)) => {
            state
                .services
                .metrics
                .body_too_large
                .fetch_add(1, Ordering::Relaxed);
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_BODY_TOO_LARGE",
                "body_read",
                "request body exceeded configured limit",
                || {
                    BTreeMap::from([(
                        "max_body_bytes".to_string(),
                        state.request.max_body_bytes.to_string(),
                    )])
                },
            );
            debug!(%peer, max_body_bytes=state.request.max_body_bytes, "request body too large");
            record_phase(
                &state,
                &mut trace,
                RequestPhase::BodyRead,
                "body_read",
                body_started.elapsed(),
            );
            let response = response::text(StatusCode::PAYLOAD_TOO_LARGE, "payload too large\n");
            return finish_php_request(
                &state,
                trace,
                response,
                None,
                Some(RequestStage::BodyAndMultipart),
            );
        }
        Ok(Err(BodyReadError::Invalid)) => {
            let script_filename = script_path.display().to_string();
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_BODY_INVALID",
                "body_read",
                "request body read failed",
                BTreeMap::new,
            );
            emit_request_diagnostic(
                &state,
                &parts,
                Some(&request_id),
                RequestDiagnostic::new(
                    "E_PHP_REQUEST_BODY_PARSE_FAILED",
                    "body_read",
                    "server could not read the request body",
                    "read_limited_body",
                    parts.uri.path(),
                    &script_filename,
                ),
            );
            warn!(%peer, "failed to read request body");
            record_phase(
                &state,
                &mut trace,
                RequestPhase::BodyRead,
                "body_read",
                body_started.elapsed(),
            );
            let response = response::text(StatusCode::BAD_REQUEST, "bad request\n");
            return finish_php_request(
                &state,
                trace,
                response,
                None,
                Some(RequestStage::BodyAndMultipart),
            );
        }
    };
    record_phase(
        &state,
        &mut trace,
        RequestPhase::BodyRead,
        "body_read",
        body_started.elapsed(),
    );
    if let Some(trace) = trace.as_mut() {
        trace.body_bytes = body.len() as u64;
    }
    emit_server_debug_lazy(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_BODY_READ_END",
        "body_read",
        "request body read completed",
        || BTreeMap::from([("body_bytes".to_string(), body.len().to_string())]),
    );
    if let Some(response) = execute_builtin_router_if_configured(
        &parts,
        Arc::clone(&state),
        Arc::clone(&body),
        peer,
        &request_id,
        Some(&script_path),
    )
    .await
    {
        return finish_php_request(
            &state,
            trace,
            response,
            None,
            Some(RequestStage::RouteTargetSelection),
        );
    }
    emit_server_debug_lazy(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_SCRIPT_RESOLVED",
        "routing",
        "PHP script resolved",
        || {
            BTreeMap::from([
                ("script_path".to_string(), script_path.display().to_string()),
                (
                    "path_info".to_string(),
                    path_info.clone().unwrap_or_default(),
                ),
            ])
        },
    );
    emit_server_debug_lazy(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_SCRIPT_CACHE_START",
        "cache",
        "script cache lookup started",
        || BTreeMap::from([("script_path".to_string(), script_path.display().to_string())]),
    );
    // Cache-stat deltas only surface through the perf-trace/request-profile
    // writers; snapshotting locks every script-cache shard, so skip it when
    // no trace consumer is configured.
    let script_cache_before =
        collect_request_trace.then(|| state.services.engine.script_cache.cache_stats());
    let script_cache_started = Instant::now();
    let lookup = match executor_acquisition_stage(&state, &script_path) {
        Ok(lookup) => {
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_SCRIPT_CACHE_END",
                "cache",
                "script cache lookup completed",
                || {
                    BTreeMap::from([
                        ("script_path".to_string(), script_path.display().to_string()),
                        ("cache_hit".to_string(), lookup.hit.to_string()),
                    ])
                },
            );
            debug!(script=%script_path.display(), hit=lookup.hit, "compiled script cache lookup");
            lookup
        }
        Err(PhpExecutionError::Compile(output)) => {
            log_php_execution_failure(&script_path, &output);
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_SCRIPT_CACHE_ERROR",
                "cache",
                "script compile failed",
                || {
                    BTreeMap::from([
                        ("script_path".to_string(), script_path.display().to_string()),
                        (
                            "diagnostic_text_bytes".to_string(),
                            output.diagnostics_text.len().to_string(),
                        ),
                    ])
                },
            );
            let response =
                php_output_response(*output, parts.method == Method::HEAD, parts.version);
            return finish_php_request(
                &state,
                trace,
                response,
                None,
                Some(RequestStage::ExecutorAcquisition),
            );
        }
        Err(PhpExecutionError::Engine(_)) => {
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_SCRIPT_CACHE_ERROR",
                "cache",
                "script compile engine error",
                || BTreeMap::from([("script_path".to_string(), script_path.display().to_string())]),
            );
            warn!(script=%script_path.display(), "php execution engine error");
            let response =
                response::text(StatusCode::INTERNAL_SERVER_ERROR, "php execution failed\n");
            return finish_php_request(
                &state,
                trace,
                response,
                None,
                Some(RequestStage::ExecutorAcquisition),
            );
        }
    };
    record_phase(
        &state,
        &mut trace,
        RequestPhase::ScriptCache,
        "script_cache_lookup",
        script_cache_started.elapsed(),
    );
    if let Some((script_cache_before, trace)) = script_cache_before.as_ref().zip(trace.as_mut()) {
        let script_cache_after = state.services.engine.script_cache.cache_stats();
        trace.counters.extend([
            (
                "entry_script_cache_hits",
                script_cache_after
                    .hits
                    .saturating_sub(script_cache_before.hits),
            ),
            (
                "entry_script_cache_misses",
                script_cache_after
                    .misses
                    .saturating_sub(script_cache_before.misses),
            ),
            (
                "entry_script_source_reads",
                script_cache_after
                    .source_reads
                    .saturating_sub(script_cache_before.source_reads),
            ),
        ]);
    }
    let script_cache_hit = Some(lookup.hit);
    let Some(cpu_permit) = acquire_cpu_execution_permit(&state).await else {
        let response = response::text(
            StatusCode::SERVICE_UNAVAILABLE,
            "PHP execution queue full\n",
        );
        return finish_php_request(
            &state,
            trace,
            response,
            script_cache_hit,
            Some(RequestStage::ExecutorAcquisition),
        );
    };
    let workers = Arc::clone(&state.concurrency.php_workers);
    workers
        .execute(move || {
            run_php_request_on_worker(
                state,
                parts,
                body,
                script_path,
                path_info,
                peer,
                request_id,
                trace,
                lookup,
                script_cache_hit,
                cpu_permit,
            )
        })
        .await
}

/// Synchronous request core: builds the PHP runtime context, parses
/// multipart bodies, seeds the session, executes the compiled script, and
/// renders the HTTP response. Everything execution-side (`RuntimeContext`,
/// `PhpExecutionOutput`) is created and consumed here, so this function can
/// run unchanged on a dedicated worker thread — its inputs and its return
/// value are the `Send` payload set pinned by `worker_payload_tests`.
#[allow(clippy::too_many_arguments)]
fn run_php_request_on_worker(
    state: Arc<AppState>,
    parts: Parts,
    body: Arc<[u8]>,
    script_path: PathBuf,
    path_info: Option<String>,
    peer: SocketAddr,
    request_id: String,
    mut trace: Option<PerfTraceEvent>,
    lookup: CompiledScriptCacheLookup,
    script_cache_hit: Option<bool>,
    cpu_permit: OwnedSemaphorePermit,
) -> (Response<ResponseBody>, Option<bool>) {
    let collect_request_trace =
        state.observability.perf_trace.is_some() || state.observability.request_profile.is_some();
    let script_name = script_name_for(&state.route_config.docroot, &script_path);
    let request_context_started = Instant::now();
    let mut request_context = request_globals_stage(
        &parts,
        &state,
        &script_path,
        &script_name,
        path_info,
        Arc::clone(&body),
        peer,
    );
    match multipart_handling_stage(&mut request_context, &body, &state) {
        Ok(Some(stats)) => {
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_MULTIPART_PARSED",
                "multipart",
                "multipart body parsed",
                || {
                    BTreeMap::from([
                        ("upload_count".to_string(), stats.uploads_total.to_string()),
                        (
                            "upload_bytes".to_string(),
                            stats.upload_bytes_accepted.to_string(),
                        ),
                    ])
                },
            );
            state
                .services
                .metrics
                .uploads_total
                .fetch_add(stats.uploads_total, Ordering::Relaxed);
            state
                .services
                .metrics
                .upload_bytes_accepted
                .fetch_add(stats.upload_bytes_accepted, Ordering::Relaxed);
        }
        Ok(None) => {}
        Err(error) => {
            let response =
                multipart_error_response(error, &state, &parts, &request_id, &script_path, peer);
            return finish_php_request(
                &state,
                trace,
                response,
                script_cache_hit,
                Some(RequestStage::BodyAndMultipart),
            );
        }
    }
    record_phase(
        &state,
        &mut trace,
        RequestPhase::RequestContext,
        "request_context",
        request_context_started.elapsed(),
    );
    let cleanup = RequestCleanup::new(request_context.uploaded_files.clone());
    emit_server_debug_lazy(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_SESSION_SEED_START",
        "session",
        "session seed started",
        || {
            BTreeMap::from([(
                "sessions_enabled".to_string(),
                state.sessions.config.enabled.to_string(),
            )])
        },
    );
    let session_seed_started = Instant::now();
    let session_state = match session_load_stage(&request_context, &state) {
        Ok(session) => session,
        Err(error) => {
            emit_request_diagnostic(
                &state,
                &parts,
                Some(&request_id),
                RequestDiagnostic::new(
                    "E_PHP_SESSION_STORE_UNAVAILABLE",
                    "session",
                    "server session store failed while preparing request state",
                    "seed_session_state",
                    parts.uri.path(),
                    &script_path.display().to_string(),
                ),
            );
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_SESSION_ERROR",
                "session",
                "session seed failed",
                || BTreeMap::from([("error".to_string(), error.clone())]),
            );
            warn!(%peer, error=%error, "session state preparation failed");
            let response = response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session storage failed\n",
            );
            return finish_php_request(
                &state,
                trace,
                response,
                script_cache_hit,
                Some(RequestStage::SessionLoad),
            );
        }
    };
    record_phase(
        &state,
        &mut trace,
        RequestPhase::SessionSeed,
        "session_seed",
        session_seed_started.elapsed(),
    );
    emit_server_debug_lazy(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_SESSION_SEED_END",
        "session",
        "session seed completed",
        || {
            BTreeMap::from([(
                "session_active".to_string(),
                (!session_state.id().is_empty()).to_string(),
            )])
        },
    );
    let runtime_context = php_runtime_context_for_http(
        &state,
        request_context,
        session_state,
        Arc::clone(&body),
        server_env_for_request(&state),
    );
    if state.request.execution_time_limit.is_none() {
        state
            .services
            .metrics
            .execution_deadline_disabled
            .fetch_add(1, Ordering::Relaxed);
    }
    let is_head = parts.method == Method::HEAD;
    let script_log_path = script_path.clone();
    let execution_started = Instant::now();
    emit_server_debug_lazy(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_EXECUTE_START",
        "execute",
        "PHP execution started",
        || {
            BTreeMap::from([(
                "script_path".to_string(),
                script_log_path.display().to_string(),
            )])
        },
    );
    let include_cache_before =
        collect_request_trace.then(|| state.services.engine.include_cache.cache_stats());
    let profile_requested = request_profile_requested(&state, &parts.headers);
    let result = execution_stage(
        Arc::clone(&state),
        lookup,
        script_path,
        runtime_context,
        profile_requested,
    );
    drop(cpu_permit);
    record_phase(
        &state,
        &mut trace,
        RequestPhase::VmExecution,
        "php_vm_execution",
        execution_started.elapsed(),
    );
    if let Some((include_cache_before, trace)) = include_cache_before.as_ref().zip(trace.as_mut()) {
        let include_cache_after = state.services.engine.include_cache.cache_stats();
        trace.counters.extend([
            (
                "include_resolution_hits",
                include_cache_after
                    .resolution_hits
                    .saturating_sub(include_cache_before.resolution_hits),
            ),
            (
                "include_resolution_misses",
                include_cache_after
                    .resolution_misses
                    .saturating_sub(include_cache_before.resolution_misses),
            ),
            (
                "include_compile_hits",
                include_cache_after
                    .compile_hits
                    .saturating_sub(include_cache_before.compile_hits),
            ),
            (
                "include_compile_misses",
                include_cache_after
                    .compile_misses
                    .saturating_sub(include_cache_before.compile_misses),
            ),
            (
                "include_source_reads",
                include_cache_after
                    .source_reads
                    .saturating_sub(include_cache_before.source_reads),
            ),
            (
                "include_source_bytes_hashed",
                include_cache_after
                    .source_bytes_hashed
                    .saturating_sub(include_cache_before.source_bytes_hashed),
            ),
            (
                "include_content_validations",
                include_cache_after
                    .content_validations
                    .saturating_sub(include_cache_before.content_validations),
            ),
            (
                "include_identity_only_hits",
                include_cache_after
                    .identity_only_hits
                    .saturating_sub(include_cache_before.identity_only_hits),
            ),
            (
                "include_content_mismatches",
                include_cache_after
                    .content_mismatches
                    .saturating_sub(include_cache_before.content_mismatches),
            ),
            (
                "include_conservative_misses",
                include_cache_after
                    .conservative_misses
                    .saturating_sub(include_cache_before.conservative_misses),
            ),
        ]);
    }
    match result {
        Ok(mut output) => {
            if let Some(trace) = trace.as_mut() {
                append_vm_counters_to_trace(trace, output.counters.as_ref());
                if state.observability.request_profile.is_some() {
                    trace.profile_counters = output.counters.clone();
                }
                trace.profile_requested = profile_requested;
            }
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_EXECUTE_END",
                "execute",
                "PHP execution completed",
                || {
                    let mut execute_end_context = BTreeMap::from([
                        ("status".to_string(), format!("{:?}", output.status)),
                        (
                            "duration_ms".to_string(),
                            execution_started.elapsed().as_millis().to_string(),
                        ),
                        (
                            "runtime_diagnostic_count".to_string(),
                            output.runtime_diagnostics.len().to_string(),
                        ),
                    ]);
                    if !output.runtime_diagnostics.is_empty() {
                        execute_end_context.insert(
                            "runtime_diagnostic_codes".to_string(),
                            output
                                .runtime_diagnostics
                                .iter()
                                .map(|diagnostic| diagnostic.id())
                                .collect::<Vec<_>>()
                                .join(","),
                        );
                        execute_end_context.insert(
                            "runtime_diagnostic_samples".to_string(),
                            runtime_diagnostic_samples(&output),
                        );
                    }
                    execute_end_context
                },
            );
            let session_finalize_started = Instant::now();
            if let Err(error) = cleanup.finalize_output(&mut output, &state) {
                emit_request_diagnostic(
                    &state,
                    &parts,
                    Some(&request_id),
                    RequestDiagnostic::new(
                        "E_PHP_SESSION_STORE_UNAVAILABLE",
                        "session",
                        "server session store failed while finalizing request state",
                        "finalize_session_state",
                        parts.uri.path(),
                        &script_log_path.display().to_string(),
                    ),
                );
                emit_server_debug_lazy(
                    &state,
                    Some(&request_id),
                    "D_PHRUST_SERVER_SESSION_ERROR",
                    "session",
                    "session finalization failed",
                    || BTreeMap::from([("error".to_string(), error.clone())]),
                );
                warn!(%peer, error=%error, "session state finalization failed");
                let response = response::text(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session storage failed\n",
                );
                return finish_php_request(
                    &state,
                    trace,
                    response,
                    script_cache_hit,
                    Some(RequestStage::SessionAndUploadCleanup),
                );
            }
            record_phase(
                &state,
                &mut trace,
                RequestPhase::SessionFinalize,
                "session_finalize",
                session_finalize_started.elapsed(),
            );
            let runtime_diagnostics = output.runtime_diagnostics.len() as u64;
            if let Some(trace) = trace.as_mut() {
                trace.runtime_diagnostics = runtime_diagnostics;
            }
            state
                .services
                .metrics
                .runtime_diagnostics
                .fetch_add(runtime_diagnostics, Ordering::Relaxed);
            if php_execution_timed_out(&output) {
                state
                    .services
                    .metrics
                    .execution_timeouts
                    .fetch_add(1, Ordering::Relaxed);
                let response = php_timeout_response(is_head, &output.http_response);
                return finish_php_request(
                    &state,
                    trace,
                    response,
                    script_cache_hit,
                    Some(RequestStage::Execution),
                );
            }
            log_php_execution_failure(&script_log_path, &output);
            let response_started = Instant::now();
            let response = response_construction_stage(output, is_head, parts.version);
            record_phase(
                &state,
                &mut trace,
                RequestPhase::ResponseBuild,
                "response_build",
                response_started.elapsed(),
            );
            finish_php_request(&state, trace, response, script_cache_hit, None)
        }
        Err(PhpExecutionError::Compile(output)) => {
            log_php_execution_failure(&script_log_path, &output);
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_EXECUTE_END",
                "execute",
                "PHP execution produced compile diagnostics",
                || {
                    BTreeMap::from([
                        ("status".to_string(), "CompileError".to_string()),
                        (
                            "duration_ms".to_string(),
                            execution_started.elapsed().as_millis().to_string(),
                        ),
                        (
                            "diagnostic_text_bytes".to_string(),
                            output.diagnostics_text.len().to_string(),
                        ),
                    ])
                },
            );
            let response = response_construction_stage(*output, is_head, parts.version);
            finish_php_request(
                &state,
                trace,
                response,
                script_cache_hit,
                Some(RequestStage::Execution),
            )
        }
        Err(PhpExecutionError::Engine(error)) => {
            emit_server_debug_lazy(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_EXECUTE_END",
                "execute",
                "PHP execution engine error",
                || {
                    BTreeMap::from([
                        ("status".to_string(), "EngineError".to_string()),
                        (
                            "duration_ms".to_string(),
                            execution_started.elapsed().as_millis().to_string(),
                        ),
                        ("error".to_string(), error.to_string()),
                    ])
                },
            );
            warn!(script=%script_log_path.display(), %error, "php execution engine error");
            let response =
                response::text(StatusCode::INTERNAL_SERVER_ERROR, "php execution failed\n");
            finish_php_request(
                &state,
                trace,
                response,
                script_cache_hit,
                Some(RequestStage::Execution),
            )
        }
    }
}

pub(crate) async fn execute_builtin_router_if_configured(
    parts: &Parts,
    state: Arc<AppState>,
    body: Arc<[u8]>,
    peer: SocketAddr,
    request_id: &str,
    target_script_path: Option<&Path>,
) -> Option<Response<ResponseBody>> {
    let router = state.route_config.builtin_router.as_ref()?;
    let router_path = state.route_config.docroot.join(router);
    let Ok(router_path) = router_path.canonicalize() else {
        return Some(response::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "router script not found\n",
        ));
    };
    if !router_path.starts_with(&state.route_config.docroot) {
        return Some(response::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "router script outside document root\n",
        ));
    }
    if target_script_path.is_some_and(|target| router_path == target) {
        return None;
    }
    let Some(cpu_permit) = acquire_cpu_execution_permit(&state).await else {
        return Some(response::text(
            StatusCode::SERVICE_UNAVAILABLE,
            "PHP execution queue full\n",
        ));
    };
    let script_name = script_name_for(&state.route_config.docroot, &router_path);
    let request_context = http_runtime_context(
        parts,
        &state,
        &router_path,
        &script_name,
        None,
        Arc::clone(&body),
        peer,
    );
    let session_state = match session_load_stage(&request_context, &state) {
        Ok(session) => session,
        Err(error) => {
            warn!(%peer, error=%error, "router session state preparation failed");
            return Some(response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session storage failed\n",
            ));
        }
    };
    let runtime_context = php_runtime_context_for_http(
        &state,
        request_context,
        session_state,
        body,
        server_env_for_request(&state),
    );
    let lookup = match executor_acquisition_stage(&state, &router_path) {
        Ok(lookup) => lookup,
        Err(PhpExecutionError::Compile(output)) => {
            return Some(php_output_response(*output, false, parts.version));
        }
        Err(PhpExecutionError::Engine(error)) => {
            warn!(script=%router_path.display(), %error, "router compile engine error");
            return Some(response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "router execution failed\n",
            ));
        }
    };
    let output = match execution_stage(
        Arc::clone(&state),
        lookup,
        router_path.clone(),
        runtime_context,
        true,
    ) {
        Ok(output) => output,
        Err(error) => {
            warn!(script=%router_path.display(), error=?error, "router execution engine error");
            return Some(response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "router execution failed\n",
            ));
        }
    };
    drop(cpu_permit);
    emit_server_debug_lazy(
        &state,
        Some(request_id),
        "D_PHRUST_SERVER_BUILTIN_ROUTER_END",
        "routing",
        "built-in router executed",
        || {
            BTreeMap::from([(
                "fallthrough".to_string(),
                matches!(output.return_value, Some(Value::Bool(false))).to_string(),
            )])
        },
    );
    if matches!(output.return_value, Some(Value::Bool(false))) {
        None
    } else {
        Some(php_output_response(
            output,
            parts.method == Method::HEAD,
            parts.version,
        ))
    }
}

struct CpuQueueCancellationGuard<'a> {
    metrics: &'a super::metrics::ServerMetrics,
    completed: bool,
}

impl Drop for CpuQueueCancellationGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.metrics
                .cpu_execution_cancelled
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

async fn acquire_cpu_execution_permit(state: &AppState) -> Option<OwnedSemaphorePermit> {
    let started = Instant::now();
    match Arc::clone(&state.concurrency.cpu_execution).try_acquire_owned() {
        Ok(permit) => {
            state
                .services
                .metrics
                .cpu_execution_admitted
                .fetch_add(1, Ordering::Relaxed);
            state
                .services
                .metrics
                .record_phase(RequestPhase::CpuQueue, started.elapsed().as_nanos());
            return Some(permit);
        }
        Err(TryAcquireError::Closed) => {
            state
                .services
                .metrics
                .cpu_execution_rejected
                .fetch_add(1, Ordering::Relaxed);
            return None;
        }
        Err(TryAcquireError::NoPermits) => {
            state
                .services
                .metrics
                .cpu_execution_queued
                .fetch_add(1, Ordering::Relaxed);
            state
                .services
                .metrics
                .cpu_execution_saturated
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    let mut cancellation = CpuQueueCancellationGuard {
        metrics: &state.services.metrics,
        completed: false,
    };
    let permit = timeout(
        state.request.request_timeout,
        Arc::clone(&state.concurrency.cpu_execution).acquire_owned(),
    )
    .await;
    cancellation.completed = true;
    state
        .services
        .metrics
        .record_phase(RequestPhase::CpuQueue, started.elapsed().as_nanos());
    match permit {
        Ok(Ok(permit)) => {
            state
                .services
                .metrics
                .cpu_execution_admitted
                .fetch_add(1, Ordering::Relaxed);
            Some(permit)
        }
        Ok(Err(_)) => {
            state
                .services
                .metrics
                .cpu_execution_rejected
                .fetch_add(1, Ordering::Relaxed);
            None
        }
        Err(_) => {
            state
                .services
                .metrics
                .cpu_execution_timeouts
                .fetch_add(1, Ordering::Relaxed);
            state
                .services
                .metrics
                .cpu_execution_rejected
                .fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

/// Executes a compiled script on the current thread. Callers are pinned
/// PHP worker threads (or the pool's degraded inline path, which already
/// wraps the whole request core in `block_in_place`), so no additional
/// scheduling boundary is needed here.
fn executor_acquisition_stage(
    state: &AppState,
    script_path: &Path,
) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
    state.compile_script(script_path)
}

pub(crate) fn execution_stage(
    state: Arc<AppState>,
    lookup: CompiledScriptCacheLookup,
    script_path: PathBuf,
    runtime_context: RuntimeContext,
    profile_requested: bool,
) -> Result<PhpExecutionOutput, PhpExecutionError> {
    let mode = if profile_requested {
        request_counter_mode(&state)
    } else {
        perf_trace_counter_mode(&state)
    };
    let collect_profile_spans = profile_requested && collect_vm_profile_spans_for_request(&state);
    execute_compiled_php_with_state(
        &state,
        lookup,
        script_path,
        runtime_context,
        mode,
        collect_profile_spans,
    )
}

/// Counter mode for requests that did not ask for a profile (only relevant
/// with `--request-profile-trigger-header`); perf-trace VM counters still
/// apply because they are a process-wide policy.
pub(crate) fn perf_trace_counter_mode(state: &AppState) -> RequestCounterMode {
    if state.observability.perf_trace.is_some() && state.observability.perf_trace_vm_counters {
        return RequestCounterMode::VmCounters;
    }
    RequestCounterMode::Off
}

/// True when this request opts into profiling: only header-triggered by
/// default; config/env can explicitly disable the header trigger for
/// profiling every request in controlled benchmark runs.
pub(crate) fn request_profile_requested(state: &AppState, headers: &HeaderMap) -> bool {
    if !state.observability.request_profile_trigger_header {
        return true;
    }
    headers
        .get("x-phrust-request-profile")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "on"))
}

/// How much VM accounting a request pays. `--request-profile` alone stays in
/// `Summary` (phase/boundary JSON only); VM hot counters and per-clone source
/// attribution are explicit opt-ins because they distort the measured request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequestCounterMode {
    Off,
    Summary,
    VmCounters,
    SourceAttributedLayout,
}

impl RequestCounterMode {
    pub(crate) fn collects_vm_counters(self) -> bool {
        matches!(self, Self::VmCounters | Self::SourceAttributedLayout)
    }

    pub(crate) fn collects_source_attribution(self) -> bool {
        matches!(self, Self::SourceAttributedLayout)
    }
}

pub(crate) fn request_counter_mode(state: &AppState) -> RequestCounterMode {
    if state.observability.request_profile.is_some()
        && state.observability.request_profile_source_attribution
    {
        return RequestCounterMode::SourceAttributedLayout;
    }
    if state.observability.request_profile.is_some()
        && state.observability.request_profile_vm_counters
    {
        return RequestCounterMode::VmCounters;
    }
    if state.observability.perf_trace.is_some() && state.observability.perf_trace_vm_counters {
        return RequestCounterMode::VmCounters;
    }
    if state.observability.request_profile.is_some() {
        return RequestCounterMode::Summary;
    }
    RequestCounterMode::Off
}

pub(crate) fn collect_vm_profile_spans_for_request(state: &AppState) -> bool {
    state.observability.request_profile.is_some()
}

pub(crate) fn execute_compiled_php_with_state(
    state: &AppState,
    lookup: CompiledScriptCacheLookup,
    script_path: PathBuf,
    runtime_context: RuntimeContext,
    mode: RequestCounterMode,
    collect_profile_spans: bool,
) -> Result<PhpExecutionOutput, PhpExecutionError> {
    state
        .services
        .metrics
        .persistent_engine_request_local_resets
        .fetch_add(1, Ordering::Relaxed);
    // PHP-visible state is always rebuilt. The worker retains only explicitly
    // engine-owned plans, constants, builtin/JIT handles, tiering hotness, and
    // guarded adaptive tables; rejecting frames/globals/resources remains visible.
    state
        .services
        .metrics
        .persistent_engine_request_local_rejections
        .fetch_add(1, Ordering::Relaxed);
    state
        .services
        .metrics
        .persistent_engine_policy_reuses
        .fetch_add(1, Ordering::Relaxed);
    let output = execute_compiled_with_request_executor(
        state,
        &lookup.compiled,
        PhpRequestExecutionInput {
            real_path: Some(script_path),
            cwd: state.route_config.docroot.clone(),
            include_roots: include_roots_for_docroot(&state.route_config.docroot),
            runtime_context,
            collect_counters: mode.collects_vm_counters(),
            collect_profile_spans,
            collect_layout_source_attribution: mode.collects_source_attribution(),
        },
    );
    Ok(output)
}

fn execute_compiled_with_request_executor(
    state: &AppState,
    compiled: &CompiledPhpScript,
    input: PhpRequestExecutionInput,
) -> PhpExecutionOutput {
    let options = state
        .services
        .engine
        .executor_options_for_request(compiled.path(), &state.services.metrics);
    let key = state.services.engine.request_executor_cache_key();
    REQUEST_EXECUTOR_CACHE.with(|cache| {
        let mut cached = cache.borrow_mut();
        let refresh = match cached.as_ref() {
            Some(cached) => cached.key != key,
            None => true,
        };
        if refresh {
            *cached = Some(CachedRequestExecutor {
                key,
                executor: PhpExecutor::with_options(options.clone()),
            });
        }
        match cached.as_mut() {
            Some(cached) => {
                cached.executor.reconfigure(options);
                cached.executor.execute_compiled(compiled, input)
            }
            None => PhpExecutor::with_options(options).execute_compiled(compiled, input),
        }
    })
}

pub(crate) fn log_php_execution_failure(script_path: &Path, output: &PhpExecutionOutput) {
    if output.status == PhpExecutionStatus::Success {
        return;
    }

    let diagnostics = output
        .runtime_diagnostics
        .iter()
        .take(5)
        .map(|diagnostic| diagnostic.to_json())
        .collect::<Vec<_>>()
        .join(" | ");
    let diagnostic_summary = if diagnostics.is_empty() {
        output.diagnostics_text.trim()
    } else {
        diagnostics.as_str()
    };

    warn!(
        script=%script_path.display(),
        status=?output.status,
        runtime_diagnostics=output.runtime_diagnostics.len(),
        stdout_bytes=output.stdout.len(),
        diagnostics=%diagnostic_summary,
        "php execution failed"
    );
}

pub(crate) fn append_vm_counters_to_trace(
    trace: &mut PerfTraceEvent,
    counters: Option<&php_vm::api::VmCounters>,
) {
    let Some(counters) = counters else {
        return;
    };
    trace.counters.extend([
        ("vm_native_candidates", counters.native_candidates),
        (
            "vm_native_compiled_regions",
            counters.native_compiled_regions,
        ),
        ("vm_native_executions", counters.native_executions),
        ("vm_function_calls", counters.function_calls),
        ("vm_method_calls", counters.method_calls),
        ("vm_frame_allocations", counters.frame_allocations),
        ("vm_frame_reuses", counters.frame_reuses),
        ("vm_value_clones", counters.value_clones),
        ("vm_string_allocations", counters.string_allocations),
        ("vm_array_handle_clones", counters.array_handle_clones),
        ("vm_object_allocations", counters.object_allocations),
        ("vm_cow_separations", counters.cow_separations),
        (
            "vm_reference_cell_creations",
            counters.reference_cell_creations,
        ),
        ("vm_array_dim_fetches", counters.array_dim_fetches),
        ("vm_output_bytes", counters.output_bytes),
        (
            "vm_internal_function_dispatches",
            counters.internal_function_dispatches,
        ),
        (
            "vm_internal_function_dispatch_cache_hits",
            counters.internal_function_dispatch_cache_hits,
        ),
        (
            "vm_internal_function_dispatch_cache_misses",
            counters.internal_function_dispatch_cache_misses,
        ),
        ("vm_builtin_call_ic_hits", counters.builtin_call_ic_hits),
        ("vm_builtin_call_ic_misses", counters.builtin_call_ic_misses),
        ("vm_includes", counters.includes),
        ("vm_autoloads", counters.autoloads),
        ("vm_inline_cache_hits", counters.inline_cache_hits),
        ("vm_inline_cache_misses", counters.inline_cache_misses),
        (
            "vm_inline_cache_guard_failures",
            counters.inline_cache_guard_failures,
        ),
        ("vm_function_call_ic_hits", counters.function_call_ic_hits),
        (
            "vm_function_call_ic_misses",
            counters.function_call_ic_misses,
        ),
        ("vm_method_ic_hits", counters.method_ic_hits),
        ("vm_method_ic_misses", counters.method_ic_misses),
        ("vm_property_ic_hits", counters.property_ic_hits),
        ("vm_property_ic_misses", counters.property_ic_misses),
        (
            "vm_property_assign_ic_hits",
            counters.property_assign_ic_hits,
        ),
        (
            "vm_property_assign_ic_misses",
            counters.property_assign_ic_misses,
        ),
        ("vm_include_path_ic_hits", counters.include_path_ic_hits),
        ("vm_include_path_ic_misses", counters.include_path_ic_misses),
        (
            "vm_autoload_class_lookup_ic_hits",
            counters.autoload_class_lookup_ic_hits,
        ),
        (
            "vm_autoload_class_lookup_ic_misses",
            counters.autoload_class_lookup_ic_misses,
        ),
        (
            "vm_persistent_engine_allocations",
            counters.persistent_engine_allocations,
        ),
        (
            "vm_persistent_engine_bytes",
            counters.persistent_engine_bytes,
        ),
        ("vm_jit_compile_attempts", counters.jit_compile_attempts),
        ("vm_jit_compiled", counters.jit_compiled),
        ("vm_jit_executed", counters.jit_executed),
        ("vm_jit_side_exits", counters.jit_side_exits),
        ("vm_jit_process_cache_hits", counters.jit_process_cache_hits),
        (
            "vm_jit_process_cache_misses",
            counters.jit_process_cache_misses,
        ),
        ("vm_jit_compile_waits", counters.jit_compile_waits),
        (
            "vm_jit_duplicate_compiles_avoided",
            counters.jit_duplicate_compiles_avoided,
        ),
        ("vm_jit_code_bytes_live", counters.jit_code_bytes_live),
        ("vm_jit_code_bytes_retired", counters.jit_code_bytes_retired),
        ("vm_jit_code_generations", counters.jit_code_generations),
        ("vm_jit_evictions", counters.jit_evictions),
    ]);
}

fn runtime_diagnostic_samples(output: &PhpExecutionOutput) -> String {
    output
        .runtime_diagnostics
        .iter()
        .take(5)
        .map(|diagnostic| {
            let mut sample = String::new();
            sample.push_str(diagnostic.id());
            sample.push_str(": ");
            sample.push_str(&truncate_debug_value(diagnostic.message(), 240));
            let span = diagnostic.source_span();
            if let Some(file) = &span.file {
                sample.push_str(" @ ");
                sample.push_str(&truncate_debug_value(file, 160));
                sample.push(':');
                sample.push_str(&span.start.to_string());
            }
            sample
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn truncate_debug_value(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            out.push_str("...");
            break;
        }
        if ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}
pub(crate) fn multipart_error_response(
    error: MultipartError,
    state: &AppState,
    parts: &Parts,
    request_id: &str,
    script_path: &Path,
    peer: SocketAddr,
) -> Response<ResponseBody> {
    emit_request_diagnostic(
        state,
        parts,
        Some(request_id),
        RequestDiagnostic::new(
            "E_PHP_REQUEST_BODY_PARSE_FAILED",
            "multipart",
            "server could not parse multipart request body",
            "parse_multipart_into_context",
            parts.uri.path(),
            &script_path.display().to_string(),
        ),
    );
    match error {
        MultipartError::Malformed => {
            state
                .services
                .metrics
                .upload_parse_errors
                .fetch_add(1, Ordering::Relaxed);
            debug!(%peer, "multipart request rejected as malformed");
            response::text(StatusCode::BAD_REQUEST, "bad multipart request\n")
        }
        MultipartError::TooManyFiles | MultipartError::FileTooLarge => {
            state
                .services
                .metrics
                .upload_files_rejected
                .fetch_add(1, Ordering::Relaxed);
            debug!(%peer, ?error, "multipart upload rejected by configured limits");
            response::text(StatusCode::PAYLOAD_TOO_LARGE, "upload rejected\n")
        }
        MultipartError::Storage => {
            warn!(%peer, "multipart upload temp storage failed");
            response::text(StatusCode::INTERNAL_SERVER_ERROR, "upload storage failed\n")
        }
    }
}

fn response_construction_stage(
    output: PhpExecutionOutput,
    is_head: bool,
    version: Version,
) -> Response<ResponseBody> {
    php_output_response(output, is_head, version)
}

pub(crate) fn php_output_response(
    output: PhpExecutionOutput,
    is_head: bool,
    request_version: Version,
) -> Response<ResponseBody> {
    let status = match output.status {
        PhpExecutionStatus::Success => {
            StatusCode::from_u16(output.http_response.status_code).unwrap_or(StatusCode::OK)
        }
        PhpExecutionStatus::CompileError
        | PhpExecutionStatus::RuntimeError
        | PhpExecutionStatus::Unsupported
        | PhpExecutionStatus::Fatal => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let stdout_len = output.stdout.len();
    let execution_failed = output.status != PhpExecutionStatus::Success;
    let body = if output.stdout.is_empty() && execution_failed {
        Bytes::from_static(b"php execution failed\n")
    } else if is_head {
        Bytes::new()
    } else {
        Bytes::from(output.stdout)
    };
    let content_length = if is_head {
        Some(stdout_len)
    } else if execution_failed || request_version != Version::HTTP_2 {
        Some(body.len())
    } else {
        None
    };
    php_transport_response(status, body, content_length, &output.http_response)
}

pub(crate) fn php_execution_timed_out(output: &PhpExecutionOutput) -> bool {
    output
        .runtime_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.id() == "E_PHP_VM_EXECUTION_TIMEOUT")
}

pub(crate) fn php_timeout_response(
    is_head: bool,
    http_response: &RuntimeHttpResponseState,
) -> Response<ResponseBody> {
    let body = if is_head {
        Bytes::new()
    } else {
        Bytes::from_static(b"php execution timeout\n")
    };
    let content_length = if is_head {
        "php execution timeout\n".len()
    } else {
        body.len()
    };
    php_transport_response(
        StatusCode::GATEWAY_TIMEOUT,
        body,
        Some(content_length),
        http_response,
    )
}

pub(crate) fn php_transport_response(
    status: StatusCode,
    body: Bytes,
    content_length: Option<usize>,
    http_response: &RuntimeHttpResponseState,
) -> Response<ResponseBody> {
    let response_bytes = body.len() as u64;
    let response_body = if content_length.is_some() {
        response::full_body(body)
    } else {
        response::stream_body_from_bytes(body)
    };
    let mut response = Response::builder()
        .status(status)
        .body(response_body)
        .expect("php response builder is valid");
    let headers = response.headers_mut();
    for header in &http_response.headers {
        if header.name.eq_ignore_ascii_case("Content-Length") {
            continue;
        }
        let Ok(name) = HeaderName::from_bytes(header.name.as_bytes()) else {
            continue;
        };
        let Ok(value) = HeaderValue::from_str(&header.value) else {
            continue;
        };
        headers.append(name, value);
    }
    if !headers.contains_key(header::CONTENT_TYPE) {
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static(PHP_CONTENT_TYPE),
        );
    }
    if let Some(content_length) = content_length {
        headers.insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&content_length.to_string())
                .expect("content length header is valid"),
        );
    }
    response
        .extensions_mut()
        .insert(PhpResponseBytes(response_bytes));
    response
}

pub(crate) const PHP_CONTENT_TYPE: &str = "text/html; charset=UTF-8";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BodyReadError {
    TooLarge,
    Invalid,
}

async fn body_and_multipart_stage(
    state: &AppState,
    body: RequestBody,
) -> Result<Result<Arc<[u8]>, BodyReadError>, tokio::time::error::Elapsed> {
    timeout(
        state.request.request_timeout,
        read_limited_body(body, state.request.max_body_bytes),
    )
    .await
}

pub(crate) async fn read_limited_body(
    mut body: RequestBody,
    max_body_bytes: usize,
) -> Result<Arc<[u8]>, BodyReadError> {
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| BodyReadError::Invalid)?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        if bytes.len().saturating_add(data.len()) > max_body_bytes {
            return Err(BodyReadError::TooLarge);
        }
        bytes.extend_from_slice(&data);
    }
    Ok(Arc::from(bytes))
}

#[allow(clippy::too_many_arguments)]
fn request_globals_stage(
    parts: &Parts,
    state: &AppState,
    script_path: &Path,
    script_name: &str,
    path_info: Option<String>,
    body: Arc<[u8]>,
    peer: SocketAddr,
) -> RuntimeHttpRequestContext {
    http_runtime_context(
        parts,
        state,
        script_path,
        script_name,
        path_info,
        body,
        peer,
    )
}

fn multipart_handling_stage(
    context: &mut RuntimeHttpRequestContext,
    body: &[u8],
    state: &AppState,
) -> Result<Option<MultipartStats>, MultipartError> {
    let Some(boundary) = multipart_boundary(context.content_type.as_deref())? else {
        return Ok(None);
    };
    parse_multipart_into_context(context, body, &boundary, &state.request.multipart_config)
        .map(Some)
}

fn session_load_stage(
    request: &RuntimeHttpRequestContext,
    state: &AppState,
) -> Result<SessionState, String> {
    seed_session_state(request, state)
}

pub(crate) fn http_runtime_context(
    parts: &Parts,
    state: &AppState,
    script_path: &Path,
    script_name: &str,
    path_info: Option<String>,
    body: Arc<[u8]>,
    peer: SocketAddr,
) -> RuntimeHttpRequestContext {
    let request_uri = parts.uri.path_and_query().map_or_else(
        || parts.uri.path().to_string(),
        |value| value.as_str().to_string(),
    );
    let request_uri = rewrite_request_uri(&request_uri, &state.route_config.request_rewrites);
    let host =
        header_value(&parts.headers, header::HOST).unwrap_or_else(|| "localhost".to_string());
    let (request_time, request_time_float_micros) = request_time_pair();
    let mut context = RuntimeHttpRequestContext::new(
        parts.method.as_str(),
        host.clone(),
        request_uri,
        script_name.to_string(),
        script_path.to_string_lossy().into_owned(),
        state.route_config.docroot.to_string_lossy().into_owned(),
    );
    context.scheme = state.transport.request_scheme.to_string();
    context.host = host;
    context.server_name = server_name_from_host(&context.host);
    let local_addr = parts
        .extensions
        .get::<RequestLocalAddr>()
        .map_or(state.transport.local_addr, |addr| addr.0);
    context.server_addr = local_addr.ip().to_string();
    context.server_port = local_addr.port();
    context.server_protocol = format!("{:?}", parts.version);
    context.https = state.transport.request_scheme == "https";
    context.php_self = php_self_for(script_name, path_info.as_deref());
    context.path_info = path_info;
    context.remote_addr = peer.ip().to_string();
    context.remote_port = Some(peer.port());
    if let Some((user, password)) = basic_authorization(&parts.headers) {
        context.auth_type = Some("Basic".to_string());
        context.remote_user = Some(user.clone());
        context.php_auth_user = Some(user);
        context.php_auth_pw = Some(password);
    }
    context.request_time = request_time;
    context.request_time_float_micros = request_time_float_micros;
    let header_snapshot = runtime_headers(&parts.headers);
    state
        .services
        .metrics
        .request_headers_seen
        .fetch_add(header_snapshot.seen, Ordering::Relaxed);
    state
        .services
        .metrics
        .request_headers_materialized
        .fetch_add(header_snapshot.entries.len() as u64, Ordering::Relaxed);
    state
        .services
        .metrics
        .request_headers_skipped_direct
        .fetch_add(header_snapshot.skipped_direct, Ordering::Relaxed);
    context.headers = header_snapshot.entries;
    context.content_type = header_value(&parts.headers, header::CONTENT_TYPE);
    context.content_length = header_value(&parts.headers, header::CONTENT_LENGTH)
        .and_then(|value| value.parse::<u64>().ok());
    context.raw_body = Arc::clone(&body);
    if context
        .content_type
        .as_deref()
        .is_some_and(is_form_urlencoded_content_type)
    {
        context.parsed_post = parse_form_urlencoded_body(&body);
    }
    if let Some(cookie) = header_value(&parts.headers, header::COOKIE) {
        context.parsed_cookie = parse_cookie_header(&cookie);
    }
    context
}

fn rewrite_request_uri(request_uri: &str, rules: &[RequestRewriteRule]) -> String {
    let (path, query) = request_uri
        .split_once('?')
        .map_or((request_uri, ""), |(path, query)| (path, query));
    for rule in rules {
        let Some(route) = rewritten_route_for_prefix(path, &rule.path_prefix) else {
            continue;
        };
        let rewrite_query = format!(
            "{}={}",
            rule.query_parameter,
            percent_encode_query_value(&route)
        );
        return if query.is_empty() {
            format!("/?{rewrite_query}")
        } else {
            format!("/?{rewrite_query}&{query}")
        };
    }
    request_uri.to_string()
}

fn rewritten_route_for_prefix(path: &str, prefix: &str) -> Option<String> {
    if prefix == "/" {
        return Some(if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        });
    }
    if path == prefix {
        return Some("/".to_string());
    }
    let remainder = path.strip_prefix(prefix)?;
    remainder
        .starts_with('/')
        .then(|| if remainder.is_empty() { "/" } else { remainder }.to_string())
}

fn percent_encode_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push(hex_digit(byte >> 4));
                encoded.push(hex_digit(byte & 0x0f));
            }
        }
    }
    encoded
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'A' + (nibble - 10)) as char,
        _ => unreachable!("hex nibble is four bits"),
    }
}

pub(crate) fn server_env_for_request(state: &AppState) -> Arc<Vec<(String, String)>> {
    if !state.capabilities.network_requests_enabled
        || state
            .capabilities
            .env_snapshot
            .iter()
            .any(|(name, _)| name == "PHRUST_NET_TESTS")
    {
        return Arc::clone(&state.capabilities.env_snapshot);
    }

    let mut env = state
        .capabilities
        .env_snapshot
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    env.push(("PHRUST_NET_TESTS".to_string(), "1".to_string()));
    env.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    Arc::new(env)
}

pub(crate) fn php_runtime_context_for_http(
    state: &AppState,
    request_context: RuntimeHttpRequestContext,
    session_state: SessionState,
    body: Arc<[u8]>,
    env: Arc<Vec<(String, String)>>,
) -> RuntimeContext {
    RuntimeContext::controlled_http(request_context)
        .with_cwd(state.route_config.docroot.clone())
        .with_include_path(vec![state.route_config.docroot.clone()])
        .with_session_state(session_state)
        .with_session_loader(session_load_callback(state))
        .with_session_id_generator(session_id_generate_callback(state))
        .with_execution_time_limit(state.request.execution_time_limit)
        .with_sorted_env_arc(env)
        .with_stdin(body)
}

fn session_load_callback(state: &AppState) -> SessionLoadCallback {
    let metrics = Arc::clone(&state.services.metrics);
    let store = Arc::clone(&state.sessions.session_store);
    SessionLoadCallback::new(move |id| {
        metrics.session_lazy_loads.fetch_add(1, Ordering::Relaxed);
        metrics.session_store_loads.fetch_add(1, Ordering::Relaxed);
        store.load(id).map_err(|error| {
            format!("E_PHP_SESSION_STORE_UNAVAILABLE: failed to load session: {error}")
        })
    })
}

fn session_id_generate_callback(state: &AppState) -> SessionIdGenerateCallback {
    let metrics = Arc::clone(&state.services.metrics);
    SessionIdGenerateCallback::new(move || {
        metrics
            .session_id_generations
            .fetch_add(1, Ordering::Relaxed);
        crate::session_store::generate_session_id().map_err(|error| {
            format!("E_PHP_SESSION_STORE_UNAVAILABLE: failed to generate session id: {error}")
        })
    })
}

fn record_phase(
    state: &AppState,
    trace: &mut Option<PerfTraceEvent>,
    phase: RequestPhase,
    name: &'static str,
    duration: Duration,
) {
    let nanos = duration.as_nanos();
    state.services.metrics.record_phase(phase, nanos);
    if let Some(trace) = trace.as_mut() {
        trace.phases.push((name, nanos));
    }
}

fn finish_php_request(
    state: &AppState,
    trace: Option<PerfTraceEvent>,
    response: Response<ResponseBody>,
    cache_hit: Option<bool>,
    failure_stage: Option<RequestStage>,
) -> (Response<ResponseBody>, Option<bool>) {
    match failure_stage {
        Some(stage) => RequestOutcome::failure(response, cache_hit, stage),
        None => RequestOutcome::success(response, cache_hit),
    }
    .finalize(state, trace)
}

pub(crate) fn header_value(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn basic_authorization(headers: &HeaderMap) -> Option<(String, String)> {
    let authorization = header_value(headers, header::AUTHORIZATION)?;
    let mut parts = authorization.splitn(2, char::is_whitespace);
    let scheme = parts.next()?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let token = parts.next()?.trim();
    if token.is_empty() {
        return None;
    }
    let decoded = BASE64_STANDARD.decode(token.as_bytes()).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (user, password) = decoded.split_once(':')?;
    Some((user.to_string(), password.to_string()))
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimeHeaderSnapshot {
    pub(crate) entries: Vec<(String, String)>,
    pub(crate) seen: u64,
    pub(crate) skipped_direct: u64,
}

pub(crate) fn runtime_headers(headers: &HeaderMap) -> RuntimeHeaderSnapshot {
    let mut snapshot = RuntimeHeaderSnapshot {
        seen: headers.len() as u64,
        ..RuntimeHeaderSnapshot::default()
    };
    for (name, value) in headers {
        if matches!(name.as_str(), "host" | "content-type" | "content-length") {
            snapshot.skipped_direct = snapshot.skipped_direct.saturating_add(1);
            continue;
        }
        let Some(value) = value.to_str().ok() else {
            continue;
        };
        snapshot
            .entries
            .push((name.as_str().to_string(), value.to_string()));
    }
    snapshot
}

pub(crate) fn is_form_urlencoded_content_type(value: &str) -> bool {
    value.split(';').next().is_some_and(|media_type| {
        media_type
            .trim()
            .eq_ignore_ascii_case("application/x-www-form-urlencoded")
    })
}

pub(crate) fn script_name_for(docroot: &Path, script_path: &Path) -> String {
    let relative = script_path.strip_prefix(docroot).unwrap_or(script_path);
    let mut value = String::from("/");
    value.push_str(
        &relative
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"),
    );
    value
}

pub(crate) fn include_roots_for_docroot(docroot: &Path) -> Vec<PathBuf> {
    let mut roots = vec![docroot.to_path_buf()];
    if let Some(parent) = docroot.parent()
        && parent != docroot
    {
        roots.push(parent.to_path_buf());
    }
    roots
}

pub(crate) fn php_self_for(script_name: &str, path_info: Option<&str>) -> String {
    path_info.map_or_else(
        || script_name.to_string(),
        |path_info| format!("{script_name}{path_info}"),
    )
}

pub(crate) fn server_name_from_host(host: &str) -> String {
    if let Some(rest) = host.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        return rest[..end].to_string();
    }
    host.rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|byte| byte.is_ascii_digit()))
        .map_or_else(|| host.to_string(), |(name, _)| name.to_string())
}

pub(crate) fn request_time() -> i64 {
    request_time_pair().0
}

pub(crate) fn request_time_pair() -> (i64, i64) {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (
        duration.as_secs() as i64,
        duration
            .as_secs()
            .saturating_mul(1_000_000)
            .saturating_add(u64::from(duration.subsec_micros())) as i64,
    )
}

#[cfg(test)]
mod worker_payload_tests {
    use super::*;

    fn assert_send<T: Send>() {}

    /// The dedicated PHP worker pool moves a request job onto a pinned
    /// worker thread and a finished HTTP response back. Everything the job
    /// captures and everything it returns must stay `Send`; execution-side
    /// types (`RuntimeContext`, `PhpExecutionOutput`) are deliberately NOT
    /// in this list — they are `Rc`-based and must be created and consumed
    /// entirely inside the worker.
    #[test]
    fn worker_job_payload_types_are_send() {
        assert_send::<Arc<AppState>>();
        assert_send::<CompiledScriptCacheLookup>();
        assert_send::<PathBuf>();
        assert_send::<Parts>();
        assert_send::<Arc<[u8]>>();
        assert_send::<Option<crate::perf_trace::PerfTraceEvent>>();
        assert_send::<OwnedSemaphorePermit>();
        assert_send::<Response<ResponseBody>>();
    }
}
