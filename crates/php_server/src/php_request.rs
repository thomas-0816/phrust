use super::{
    diagnostics::{RequestDiagnostic, emit_request_diagnostic, emit_server_debug},
    sessions::{finalize_session_state, seed_session_state},
    state::AppState,
};
use crate::{
    multipart::{
        MultipartError, cleanup_uploaded_files, multipart_boundary, parse_multipart_into_context,
    },
    response::{self, ResponseBody},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::{
    Method, Response, StatusCode,
    body::Incoming,
    header::{self, HeaderName, HeaderValue},
    http::{HeaderMap, request::Parts},
};
use php_executor::{
    CompiledScriptCacheLookup, PhpExecutionError, PhpExecutionOutput, PhpExecutionStatus,
    PhpExecutor, PhpRequestExecutionInput,
};
use php_runtime::api::{
    RuntimeContext, RuntimeHttpRequestContext, RuntimeHttpResponseState, SessionState,
    parse_cookie_header, parse_form_urlencoded_body,
};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{task, time::timeout};
use tracing::{debug, warn};

pub(crate) struct PartsAndBody {
    pub(crate) parts: Parts,
    pub(crate) body: Incoming,
}

pub(crate) async fn execute_php_request(
    request: PartsAndBody,
    state: Arc<AppState>,
    script_path: PathBuf,
    path_info: Option<String>,
    peer: SocketAddr,
    request_id: String,
) -> (Response<ResponseBody>, Option<bool>) {
    let PartsAndBody { parts, body } = request;
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_BODY_READ_START",
        "body_read",
        "request body read started",
        BTreeMap::from([(
            "max_body_bytes".to_string(),
            state.max_body_bytes.to_string(),
        )]),
    );
    let body = match timeout(
        state.request_timeout,
        read_limited_body(body, state.max_body_bytes),
    )
    .await
    {
        Err(_) => {
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_BODY_READ_TIMEOUT",
                "body_read",
                "request body read timed out",
                BTreeMap::from([(
                    "timeout_ms".to_string(),
                    state.request_timeout.as_millis().to_string(),
                )]),
            );
            return (
                response::text(StatusCode::REQUEST_TIMEOUT, "request timeout\n"),
                None,
            );
        }
        Ok(Ok(body)) => body,
        Ok(Err(BodyReadError::TooLarge)) => {
            state.metrics.body_too_large.fetch_add(1, Ordering::Relaxed);
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_BODY_TOO_LARGE",
                "body_read",
                "request body exceeded configured limit",
                BTreeMap::from([(
                    "max_body_bytes".to_string(),
                    state.max_body_bytes.to_string(),
                )]),
            );
            debug!(%peer, max_body_bytes=state.max_body_bytes, "request body too large");
            return (
                response::text(StatusCode::PAYLOAD_TOO_LARGE, "payload too large\n"),
                None,
            );
        }
        Ok(Err(BodyReadError::Invalid)) => {
            let script_filename = script_path.display().to_string();
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_BODY_INVALID",
                "body_read",
                "request body read failed",
                BTreeMap::new(),
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
            return (
                response::text(StatusCode::BAD_REQUEST, "bad request\n"),
                None,
            );
        }
    };
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_BODY_READ_END",
        "body_read",
        "request body read completed",
        BTreeMap::from([("body_bytes".to_string(), body.len().to_string())]),
    );
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_SCRIPT_RESOLVED",
        "routing",
        "PHP script resolved",
        BTreeMap::from([
            ("script_path".to_string(), script_path.display().to_string()),
            (
                "path_info".to_string(),
                path_info.clone().unwrap_or_default(),
            ),
        ]),
    );
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_SCRIPT_CACHE_START",
        "cache",
        "script cache lookup started",
        BTreeMap::from([("script_path".to_string(), script_path.display().to_string())]),
    );
    let lookup = match state.compile_script(&script_path) {
        Ok(lookup) => {
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_SCRIPT_CACHE_END",
                "cache",
                "script cache lookup completed",
                BTreeMap::from([
                    ("script_path".to_string(), script_path.display().to_string()),
                    ("cache_hit".to_string(), lookup.hit.to_string()),
                ]),
            );
            debug!(script=%script_path.display(), hit=lookup.hit, "compiled script cache lookup");
            lookup
        }
        Err(PhpExecutionError::Compile(output)) => {
            log_php_execution_failure(&script_path, &output);
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_SCRIPT_CACHE_ERROR",
                "cache",
                "script compile failed",
                BTreeMap::from([
                    ("script_path".to_string(), script_path.display().to_string()),
                    (
                        "diagnostic_text_bytes".to_string(),
                        output.diagnostics_text.len().to_string(),
                    ),
                ]),
            );
            return (
                php_output_response(*output, parts.method == Method::HEAD),
                None,
            );
        }
        Err(PhpExecutionError::Engine(_)) => {
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_SCRIPT_CACHE_ERROR",
                "cache",
                "script compile engine error",
                BTreeMap::from([("script_path".to_string(), script_path.display().to_string())]),
            );
            warn!(script=%script_path.display(), "php execution engine error");
            return (
                response::text(StatusCode::INTERNAL_SERVER_ERROR, "php execution failed\n"),
                None,
            );
        }
    };
    let script_cache_hit = Some(lookup.hit);
    let script_name = script_name_for(&state.route_config.docroot, &script_path);
    let mut request_context = http_runtime_context(
        &parts,
        &state,
        &script_path,
        &script_name,
        path_info,
        &body,
        peer,
    );
    if let Some(boundary) = match multipart_boundary(request_context.content_type.as_deref()) {
        Ok(boundary) => boundary,
        Err(error) => {
            return (
                multipart_error_response(error, &state, &parts, &request_id, &script_path, peer),
                script_cache_hit,
            );
        }
    } {
        match parse_multipart_into_context(
            &mut request_context,
            &body,
            &boundary,
            &state.multipart_config,
        ) {
            Ok(stats) => {
                emit_server_debug(
                    &state,
                    Some(&request_id),
                    "D_PHRUST_SERVER_MULTIPART_PARSED",
                    "multipart",
                    "multipart body parsed",
                    BTreeMap::from([
                        ("upload_count".to_string(), stats.uploads_total.to_string()),
                        (
                            "upload_bytes".to_string(),
                            stats.upload_bytes_accepted.to_string(),
                        ),
                    ]),
                );
                state
                    .metrics
                    .uploads_total
                    .fetch_add(stats.uploads_total, Ordering::Relaxed);
                state
                    .metrics
                    .upload_bytes_accepted
                    .fetch_add(stats.upload_bytes_accepted, Ordering::Relaxed);
            }
            Err(error) => {
                return (
                    multipart_error_response(
                        error,
                        &state,
                        &parts,
                        &request_id,
                        &script_path,
                        peer,
                    ),
                    script_cache_hit,
                );
            }
        }
    }
    let upload_cleanup = request_context.uploaded_files.clone();
    let _session_guard = if state.session_config.enabled {
        Some(
            state
                .session_lock
                .lock()
                .expect("session mutex poisoned while handling request session"),
        )
    } else {
        None
    };
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_SESSION_SEED_START",
        "session",
        "session seed started",
        BTreeMap::from([(
            "sessions_enabled".to_string(),
            state.session_config.enabled.to_string(),
        )]),
    );
    let session_state = match seed_session_state(&request_context, &state) {
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
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_SESSION_ERROR",
                "session",
                "session seed failed",
                BTreeMap::from([("error".to_string(), error.clone())]),
            );
            warn!(%peer, error=%error, "session state preparation failed");
            return (
                response::text(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session storage failed\n",
                ),
                script_cache_hit,
            );
        }
    };
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_SESSION_SEED_END",
        "session",
        "session seed completed",
        BTreeMap::from([(
            "session_active".to_string(),
            (!session_state.id().is_empty()).to_string(),
        )]),
    );
    let runtime_context = php_runtime_context_for_http(
        &state,
        request_context,
        session_state,
        body.clone(),
        std::env::vars().collect(),
    );
    if state.execution_time_limit.is_none() {
        state
            .metrics
            .execution_deadline_disabled
            .fetch_add(1, Ordering::Relaxed);
    }
    let is_head = parts.method == Method::HEAD;
    let script_log_path = script_path.clone();
    let execution_started = Instant::now();
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_EXECUTE_START",
        "execute",
        "PHP execution started",
        BTreeMap::from([(
            "script_path".to_string(),
            script_log_path.display().to_string(),
        )]),
    );
    let result = execute_compiled_php_in_blocking_region(
        Arc::clone(&state),
        lookup,
        script_path,
        runtime_context,
    );
    match result {
        Ok(mut output) => {
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
            }
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_EXECUTE_END",
                "execute",
                "PHP execution completed",
                execute_end_context,
            );
            output.upload_registry.cleanup_unmoved();
            if let Err(error) = finalize_session_state(&mut output, &state) {
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
                emit_server_debug(
                    &state,
                    Some(&request_id),
                    "D_PHRUST_SERVER_SESSION_ERROR",
                    "session",
                    "session finalization failed",
                    BTreeMap::from([("error".to_string(), error.clone())]),
                );
                warn!(%peer, error=%error, "session state finalization failed");
                return (
                    response::text(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "session storage failed\n",
                    ),
                    script_cache_hit,
                );
            }
            if php_execution_timed_out(&output) {
                state
                    .metrics
                    .execution_timeouts
                    .fetch_add(1, Ordering::Relaxed);
                return (
                    php_timeout_response(is_head, &output.http_response),
                    script_cache_hit,
                );
            }
            log_php_execution_failure(&script_log_path, &output);
            (php_output_response(output, is_head), script_cache_hit)
        }
        Err(PhpExecutionError::Compile(output)) => {
            cleanup_uploaded_files(&upload_cleanup);
            log_php_execution_failure(&script_log_path, &output);
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_EXECUTE_END",
                "execute",
                "PHP execution produced compile diagnostics",
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
                ]),
            );
            (php_output_response(*output, is_head), script_cache_hit)
        }
        Err(PhpExecutionError::Engine(error)) => {
            cleanup_uploaded_files(&upload_cleanup);
            emit_server_debug(
                &state,
                Some(&request_id),
                "D_PHRUST_SERVER_EXECUTE_END",
                "execute",
                "PHP execution engine error",
                BTreeMap::from([
                    ("status".to_string(), "EngineError".to_string()),
                    (
                        "duration_ms".to_string(),
                        execution_started.elapsed().as_millis().to_string(),
                    ),
                    ("error".to_string(), error.to_string()),
                ]),
            );
            warn!(script=%script_log_path.display(), %error, "php execution engine error");
            (
                response::text(StatusCode::INTERNAL_SERVER_ERROR, "php execution failed\n"),
                script_cache_hit,
            )
        }
    }
}

pub(crate) fn execute_compiled_php_in_blocking_region(
    state: Arc<AppState>,
    lookup: CompiledScriptCacheLookup,
    script_path: PathBuf,
    runtime_context: RuntimeContext,
) -> Result<PhpExecutionOutput, PhpExecutionError> {
    task::block_in_place(move || {
        execute_compiled_php_with_state(&state, lookup, script_path, runtime_context, false)
    })
}

pub(crate) fn execute_compiled_php_with_state(
    state: &AppState,
    lookup: CompiledScriptCacheLookup,
    script_path: PathBuf,
    runtime_context: RuntimeContext,
    collect_counters: bool,
) -> Result<PhpExecutionOutput, PhpExecutionError> {
    state
        .metrics
        .persistent_engine_request_local_resets
        .fetch_add(1, Ordering::Relaxed);
    state
        .metrics
        .persistent_engine_request_local_rejections
        .fetch_add(1, Ordering::Relaxed);
    state
        .metrics
        .persistent_engine_policy_reuses
        .fetch_add(1, Ordering::Relaxed);
    let executor = PhpExecutor::with_options(state.engine.executor_options_with_include_cache());
    let output = executor.execute_compiled(
        &lookup.compiled,
        PhpRequestExecutionInput {
            real_path: Some(script_path),
            cwd: state.route_config.docroot.clone(),
            include_roots: include_roots_for_docroot(&state.route_config.docroot),
            runtime_context,
            collect_counters,
        },
    );
    Ok(output)
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
                .metrics
                .upload_parse_errors
                .fetch_add(1, Ordering::Relaxed);
            debug!(%peer, "multipart request rejected as malformed");
            response::text(StatusCode::BAD_REQUEST, "bad multipart request\n")
        }
        MultipartError::TooManyFiles | MultipartError::FileTooLarge => {
            state
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

pub(crate) fn php_output_response(
    output: PhpExecutionOutput,
    is_head: bool,
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
    let body = if output.stdout.is_empty() && status != StatusCode::OK {
        Bytes::from_static(b"php execution failed\n")
    } else if is_head {
        Bytes::new()
    } else {
        Bytes::from(output.stdout)
    };
    let content_length = if is_head { stdout_len } else { body.len() };
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
        content_length,
        http_response,
    )
}

pub(crate) fn php_transport_response(
    status: StatusCode,
    body: Bytes,
    content_length: usize,
    http_response: &RuntimeHttpResponseState,
) -> Response<ResponseBody> {
    let mut response = Response::builder()
        .status(status)
        .body(response::full_body(body))
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
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string()).expect("content length header is valid"),
    );
    response
}

pub(crate) const PHP_CONTENT_TYPE: &str = "text/html; charset=UTF-8";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BodyReadError {
    TooLarge,
    Invalid,
}

pub(crate) async fn read_limited_body(
    mut body: Incoming,
    max_body_bytes: usize,
) -> Result<Vec<u8>, BodyReadError> {
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
    Ok(bytes)
}

pub(crate) fn http_runtime_context(
    parts: &Parts,
    state: &AppState,
    script_path: &Path,
    script_name: &str,
    path_info: Option<String>,
    body: &[u8],
    peer: SocketAddr,
) -> RuntimeHttpRequestContext {
    let request_uri = parts.uri.path_and_query().map_or_else(
        || parts.uri.path().to_string(),
        |value| value.as_str().to_string(),
    );
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
    context.scheme = state.request_scheme.to_string();
    context.host = host;
    context.server_name = server_name_from_host(&context.host);
    context.server_port = state.local_addr.port();
    context.server_protocol = format!("{:?}", parts.version);
    context.https = state.request_scheme == "https";
    context.php_self = php_self_for(script_name, path_info.as_deref());
    context.path_info = path_info;
    context.remote_addr = peer.ip().to_string();
    context.request_time = request_time;
    context.request_time_float_micros = request_time_float_micros;
    context.headers = runtime_headers(&parts.headers);
    context.content_type = header_value(&parts.headers, header::CONTENT_TYPE);
    context.content_length = header_value(&parts.headers, header::CONTENT_LENGTH)
        .and_then(|value| value.parse::<u64>().ok());
    context.raw_body = body.to_vec();
    if context
        .content_type
        .as_deref()
        .is_some_and(is_form_urlencoded_content_type)
    {
        context.parsed_post = parse_form_urlencoded_body(body);
    }
    if let Some(cookie) = header_value(&parts.headers, header::COOKIE) {
        context.parsed_cookie = parse_cookie_header(&cookie);
    }
    context
}

pub(crate) fn php_runtime_context_for_http(
    state: &AppState,
    request_context: RuntimeHttpRequestContext,
    session_state: SessionState,
    body: Vec<u8>,
    env: Vec<(String, String)>,
) -> RuntimeContext {
    RuntimeContext::controlled_http(request_context)
        .with_cwd(state.route_config.docroot.clone())
        .with_include_path(vec![state.route_config.docroot.clone()])
        .with_session_state(session_state)
        .with_execution_time_limit(state.execution_time_limit)
        .with_env(env)
        .with_stdin(body)
}

pub(crate) fn header_value(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

pub(crate) fn runtime_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            Some((name.as_str().to_string(), value.to_str().ok()?.to_string()))
        })
        .collect()
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
