use super::{
    access_log::AccessLogEntry,
    diagnostics::{
        RequestDiagnostic, emit_request_diagnostic, emit_server_debug, header_debug_value,
        route_debug_name,
    },
    metrics::metrics_response,
    php_request::{PartsAndBody, execute_php_request, request_time},
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

pub(crate) async fn serve_until_shutdown(
    listener: TcpListener,
    state: Arc<AppState>,
    tls_acceptor: Option<TlsAcceptor>,
) {
    let mut tasks = JoinSet::new();
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
            signal = tokio::signal::ctrl_c() => {
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
    let Ok(_permit) = Arc::clone(&state.in_flight).try_acquire_owned() else {
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
        debug!(%peer, "request rejected because max in-flight limit is exhausted");
        return response;
    };
    let route = resolve_route(method.as_str(), parts.uri.path(), &state.route_config);
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
            )
            .await;
            (response, route_kind, cache_hit)
        }
        ResolvedRoute::NotFound => {
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
