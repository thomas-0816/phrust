use super::{
    access_log::AccessLogEntry,
    diagnostics::{
        RequestDiagnostic, emit_request_diagnostic, emit_server_debug, header_debug_value,
        route_debug_name,
    },
    metrics::metrics_response,
    php_request::{
        BodyReadError, PartsAndBody, execute_builtin_router_if_configured, execute_php_request,
        read_limited_body, request_time,
    },
    state::AppState,
    static_files::static_file_response,
};
use crate::{
    response::{self, ResponseBody},
    routing::{ResolvedRoute, resolve_route},
};
use hyper::{
    Method, Request, Response, StatusCode,
    body::Incoming,
    header::{self, HeaderValue},
    http::request::Parts,
    service::service_fn,
};
use hyper_util::{rt::TokioExecutor, rt::TokioIo, server::conn::auto::Builder};
use std::{
    collections::BTreeMap,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    task::JoinSet,
    time::timeout,
};
use tokio_rustls::TlsAcceptor;
use tracing::{debug, warn};

const REQUEST_ADMISSION_TIMEOUT: Duration = Duration::from_millis(500);

pub(crate) async fn serve_until_shutdown(
    listener: TcpListener,
    state: Arc<AppState>,
    tls_acceptor: Option<TlsAcceptor>,
) {
    let mut tasks = JoinSet::new();
    #[cfg(not(target_os = "wasi"))]
    let shutdown = tokio::signal::ctrl_c();
    #[cfg(target_os = "wasi")]
    let shutdown = std::future::pending::<std::io::Result<()>>();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            accept = listener.accept() => {
                let Ok((stream, peer)) = accept else {
                    continue;
                };
                let state = Arc::clone(&state);
                let tls_acceptor = tls_acceptor.clone();
                tasks.spawn(async move {
                    if let Some(tls_acceptor) = tls_acceptor {
                        match tls_acceptor.accept(stream).await {
                            Ok(stream) => serve_connection(stream, state, peer).await,
                            Err(error) => warn!(%peer, %error, "TLS handshake failed"),
                        }
                    } else {
                        serve_connection(stream, state, peer).await;
                    }
                });
            }
            Some(_) = tasks.join_next() => {}
            signal = &mut shutdown => {
                if signal.is_err() {
                    break;
                }
                break;
            }
        }
    }
    let _ = timeout(Duration::from_secs(5), async {
        while tasks.join_next().await.is_some() {}
    })
    .await;
    tasks.abort_all();
}

pub(crate) async fn serve_connection<S>(stream: S, state: Arc<AppState>, peer: SocketAddr)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service = service_fn(move |request| {
        let state = Arc::clone(&state);
        async move { Ok::<_, Infallible>(handle(request, state, peer).await) }
    });
    let io = TokioIo::new(stream);
    let builder = Builder::new(TokioExecutor::new());
    let _ = builder.serve_connection(io, service).await;
}

pub(crate) async fn handle(
    request: Request<Incoming>,
    state: Arc<AppState>,
    peer: SocketAddr,
) -> Response<ResponseBody> {
    let started = Instant::now();
    let request_id = state.next_request_id();
    state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
    let (parts, body) = request.into_parts();
    let method = parts.method.clone();
    let request_target = parts
        .uri
        .path_and_query()
        .map_or_else(|| parts.uri.path().to_string(), |value| value.to_string());
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_REQUEST_ACCEPTED",
        "request",
        "server request accepted",
        BTreeMap::from([
            ("peer".to_string(), peer.to_string()),
            ("method".to_string(), method.to_string()),
            ("path".to_string(), parts.uri.path().to_string()),
            (
                "query_present".to_string(),
                parts.uri.query().is_some().to_string(),
            ),
            (
                "authorization".to_string(),
                header_debug_value(&parts.headers, header::AUTHORIZATION),
            ),
            (
                "cookie".to_string(),
                header_debug_value(&parts.headers, header::COOKIE),
            ),
        ]),
    );
    let admission_started = Instant::now();
    let _permit = match timeout(
        REQUEST_ADMISSION_TIMEOUT,
        Arc::clone(&state.in_flight).acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => {
            // Queue-wait signal: time spent waiting for an in-flight permit
            // (the blocking-region admission gate). Near-zero under low load;
            // grows as workers saturate.
            state.metrics.record_phase(
                super::metrics::RequestPhase::AdmissionWait,
                admission_started.elapsed().as_nanos(),
            );
            permit
        }
        Ok(Err(_)) | Err(_) => {
            state.metrics.overload.fetch_add(1, Ordering::Relaxed);
            let response = overloaded();
            state.metrics.record_response(response.status());
            write_access_log(
                &state,
                AccessLogEntry {
                    timestamp: request_time() as u64,
                    method: method.as_str(),
                    path: &request_target,
                    status: response.status(),
                    bytes: response_content_length(&response),
                    duration: started.elapsed(),
                    route: "overload",
                    cache_hit: None,
                },
            );
            debug!(%peer, "request rejected because max in-flight admission wait expired");
            return response;
        }
    };
    let route_started = Instant::now();
    let route = resolve_route(method.as_str(), parts.uri.path(), &state.route_config);
    let route_resolution = route_started.elapsed();
    state.metrics.record_phase(
        super::metrics::RequestPhase::RouteResolution,
        route_resolution.as_nanos(),
    );
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_ROUTE_RESOLVED",
        "routing",
        "server route resolved",
        BTreeMap::from([("route".to_string(), route_debug_name(&route).to_string())]),
    );
    debug!(
        %peer,
        method=%method,
        path=%parts.uri.path(),
        route=?route,
        "classified request"
    );
    let (response, route_kind, cache_hit) = match route {
        ResolvedRoute::Health => match method {
            Method::GET => (response::text(StatusCode::OK, "ok\n"), "health", None),
            Method::HEAD => (response::empty(StatusCode::OK), "health", None),
            _ => (method_not_allowed(), "health", None),
        },
        ResolvedRoute::Metrics => (metrics_response(&state, &parts), "metrics", None),
        ResolvedRoute::CacheClear => (clear_cache_response(&state, peer), "cache-clear", None),
        ResolvedRoute::StaticFile { path, metadata } => {
            if let Some(response) = execute_builtin_router_before_normal_route(
                &parts,
                body,
                Arc::clone(&state),
                peer,
                &request_id,
            )
            .await
            {
                (response, "builtin-router", None)
            } else {
                state
                    .metrics
                    .static_responses
                    .fetch_add(1, Ordering::Relaxed);
                (
                    static_file_response(&parts, &state, path, metadata).await,
                    "static",
                    None,
                )
            }
        }
        ResolvedRoute::PhpScript {
            script_path,
            path_info,
        } => {
            state.metrics.php_responses.fetch_add(1, Ordering::Relaxed);
            let route_kind = if path_info.is_some() {
                "front-controller"
            } else {
                "php"
            };
            let (response, cache_hit) = execute_php_request(
                PartsAndBody { parts, body },
                Arc::clone(&state),
                script_path,
                path_info,
                peer,
                request_id.clone(),
                route_resolution,
            )
            .await;
            (response, route_kind, cache_hit)
        }
        ResolvedRoute::NotFound => {
            if let Some(response) = execute_builtin_router_before_normal_route(
                &parts,
                body,
                Arc::clone(&state),
                peer,
                &request_id,
            )
            .await
            {
                (response, "builtin-router", None)
            } else {
                emit_request_diagnostic(
                    &state,
                    &parts,
                    Some(&request_id),
                    RequestDiagnostic::new(
                        "E_PHP_SERVER_SCRIPT_RESOLUTION_FAILED",
                        "routing",
                        "server could not resolve a PHP script for the request",
                        "resolve_route",
                        parts.uri.path(),
                        "",
                    ),
                );
                (
                    response::text(StatusCode::NOT_FOUND, "not found\n"),
                    "not-found",
                    None,
                )
            }
        }
        ResolvedRoute::Forbidden => {
            emit_request_diagnostic(
                &state,
                &parts,
                Some(&request_id),
                RequestDiagnostic::new(
                    "E_PHP_SERVER_OUTSIDE_DOCUMENT_ROOT",
                    "routing",
                    "server rejected a path outside the document root",
                    "resolve_route",
                    parts.uri.path(),
                    "",
                ),
            );
            (
                response::text(StatusCode::FORBIDDEN, "forbidden\n"),
                "forbidden",
                None,
            )
        }
        ResolvedRoute::BadRequest => {
            emit_request_diagnostic(
                &state,
                &parts,
                Some(&request_id),
                RequestDiagnostic::new(
                    "E_PHP_SERVER_SCRIPT_RESOLUTION_FAILED",
                    "routing",
                    "server could not parse the request path for script resolution",
                    "resolve_route",
                    parts.uri.path(),
                    "",
                ),
            );
            (
                response::text(StatusCode::BAD_REQUEST, "bad request\n"),
                "bad-request",
                None,
            )
        }
        ResolvedRoute::MethodNotAllowed => (method_not_allowed(), "method-not-allowed", None),
    };
    state.metrics.record_response(response.status());
    emit_server_debug(
        &state,
        Some(&request_id),
        "D_PHRUST_SERVER_RESPONSE",
        "response",
        "server response generated",
        BTreeMap::from([
            ("status".to_string(), response.status().as_u16().to_string()),
            (
                "content_length".to_string(),
                response_content_length(&response).to_string(),
            ),
            ("route".to_string(), route_kind.to_string()),
            (
                "duration_ms".to_string(),
                started.elapsed().as_millis().to_string(),
            ),
        ]),
    );
    write_access_log(
        &state,
        AccessLogEntry {
            timestamp: request_time() as u64,
            method: method.as_str(),
            path: &request_target,
            status: response.status(),
            bytes: response_content_length(&response),
            duration: started.elapsed(),
            route: route_kind,
            cache_hit,
        },
    );
    response
}
pub(crate) fn write_access_log(state: &AppState, entry: AccessLogEntry<'_>) {
    if let Some(access_log) = &state.access_log
        && let Err(error) = access_log.write(&entry)
    {
        warn!(%error, "access log write failed");
    }
}
pub(crate) fn response_content_length(response: &Response<ResponseBody>) -> u64 {
    response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

async fn execute_builtin_router_before_normal_route(
    parts: &Parts,
    body: Incoming,
    state: Arc<AppState>,
    peer: SocketAddr,
    request_id: &str,
) -> Option<Response<ResponseBody>> {
    state.route_config.builtin_router.as_ref()?;
    emit_server_debug(
        &state,
        Some(request_id),
        "D_PHRUST_SERVER_BODY_READ_START",
        "body_read",
        "request body read started",
        BTreeMap::from([(
            "max_body_bytes".to_string(),
            state.max_body_bytes.to_string(),
        )]),
    );
    let body_started = Instant::now();
    let body = match timeout(
        state.request_timeout,
        read_limited_body(body, state.max_body_bytes),
    )
    .await
    {
        Err(_) => {
            emit_server_debug(
                &state,
                Some(request_id),
                "D_PHRUST_SERVER_BODY_READ_TIMEOUT",
                "body_read",
                "request body read timed out",
                BTreeMap::from([(
                    "timeout_ms".to_string(),
                    state.request_timeout.as_millis().to_string(),
                )]),
            );
            state.metrics.record_phase(
                super::metrics::RequestPhase::BodyRead,
                body_started.elapsed().as_nanos(),
            );
            return Some(response::text(
                StatusCode::REQUEST_TIMEOUT,
                "request timeout\n",
            ));
        }
        Ok(Ok(body)) => body,
        Ok(Err(BodyReadError::TooLarge)) => {
            state.metrics.body_too_large.fetch_add(1, Ordering::Relaxed);
            emit_server_debug(
                &state,
                Some(request_id),
                "D_PHRUST_SERVER_BODY_TOO_LARGE",
                "body_read",
                "request body exceeded configured limit",
                BTreeMap::from([(
                    "max_body_bytes".to_string(),
                    state.max_body_bytes.to_string(),
                )]),
            );
            debug!(%peer, max_body_bytes=state.max_body_bytes, "request body too large");
            state.metrics.record_phase(
                super::metrics::RequestPhase::BodyRead,
                body_started.elapsed().as_nanos(),
            );
            return Some(response::text(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload too large\n",
            ));
        }
        Ok(Err(BodyReadError::Invalid)) => {
            emit_server_debug(
                &state,
                Some(request_id),
                "D_PHRUST_SERVER_BODY_INVALID",
                "body_read",
                "request body read failed",
                BTreeMap::new(),
            );
            warn!(%peer, "failed to read request body");
            state.metrics.record_phase(
                super::metrics::RequestPhase::BodyRead,
                body_started.elapsed().as_nanos(),
            );
            return Some(response::text(StatusCode::BAD_REQUEST, "bad request\n"));
        }
    };
    state.metrics.record_phase(
        super::metrics::RequestPhase::BodyRead,
        body_started.elapsed().as_nanos(),
    );
    emit_server_debug(
        &state,
        Some(request_id),
        "D_PHRUST_SERVER_BODY_READ_END",
        "body_read",
        "request body read completed",
        BTreeMap::from([("body_bytes".to_string(), body.len().to_string())]),
    );
    execute_builtin_router_if_configured(parts, state, body, peer, request_id, None)
}

pub(crate) fn clear_cache_response(state: &AppState, peer: SocketAddr) -> Response<ResponseBody> {
    if !peer.ip().is_loopback() {
        return response::text(StatusCode::FORBIDDEN, "forbidden\n");
    }
    state.engine.script_cache.clear();
    if let Err(error) = state.engine.include_cache.clear() {
        warn!(%error, "failed to clear include cache");
        return response::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "include cache clear failed\n",
        );
    }
    response::text(StatusCode::OK, "cache cleared\n")
}
pub(crate) fn method_not_allowed() -> Response<ResponseBody> {
    let mut response = response::text(StatusCode::METHOD_NOT_ALLOWED, "method not allowed\n");
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
    response
}

pub(crate) fn overloaded() -> Response<ResponseBody> {
    let mut response = response::text(StatusCode::SERVICE_UNAVAILABLE, "server overloaded\n");
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
    response
}
