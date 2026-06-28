use crate::{
    config::{ConfigError, ServerConfig},
    response::{self, ResponseBody},
    routing::{ResolvedRoute, RouteConfig, resolve_route},
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{
    Method, Request, Response, StatusCode,
    body::Incoming,
    header::{self, HeaderName, HeaderValue},
    http::{HeaderMap, request::Parts},
    service::service_fn,
};
use hyper_util::{rt::TokioExecutor, rt::TokioIo, server::conn::auto::Builder};
use php_executor::{
    CompiledScriptCache, CompiledScriptCacheLookup, OptimizationLevel, PhpExecutionError,
    PhpExecutionOutput, PhpExecutionStatus, PhpExecutor, PhpRequestExecutionInput,
    PhpScriptCacheInput,
};
use php_runtime::{
    RuntimeContext, RuntimeHttpRequestContext, RuntimeHttpResponseState, parse_cookie_header,
    parse_form_urlencoded_body,
};
use std::{
    convert::Infallible,
    fmt,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinSet, time::timeout};
use tracing::{debug, warn};

#[derive(Clone, Debug)]
struct AppState {
    route_config: RouteConfig,
    max_body_bytes: usize,
    request_timeout: Duration,
    in_flight: Arc<Semaphore>,
    max_in_flight: usize,
    metrics: Arc<ServerMetrics>,
    executor: PhpExecutor,
    script_cache: Arc<CompiledScriptCache>,
    local_addr: SocketAddr,
}

impl AppState {
    fn compile_script(
        &self,
        script_path: &Path,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
        self.script_cache.get_or_compile_script(
            &self.executor,
            PhpScriptCacheInput {
                path: script_path.to_path_buf(),
                source_path: script_path.to_string_lossy().into_owned(),
                optimization_level: OptimizationLevel::O0,
            },
        )
    }
}

#[derive(Debug, Default)]
struct ServerMetrics {
    requests_total: AtomicU64,
    static_responses: AtomicU64,
    php_responses: AtomicU64,
    four_xx: AtomicU64,
    five_xx: AtomicU64,
    body_too_large: AtomicU64,
    overload: AtomicU64,
}

impl ServerMetrics {
    fn record_response(&self, status: StatusCode) {
        if status.is_client_error() {
            self.four_xx.fetch_add(1, Ordering::Relaxed);
        } else if status.is_server_error() {
            self.five_xx.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn render(&self, in_flight: u64, cache: php_executor::CompiledScriptCacheStats) -> String {
        format!(
            "# phrust-server MVP internal metrics\n\
phrust_server_requests_total {}\n\
phrust_server_static_responses_total {}\n\
phrust_server_php_responses_total {}\n\
phrust_server_4xx_total {}\n\
phrust_server_5xx_total {}\n\
phrust_server_in_flight {}\n\
phrust_server_body_too_large_total {}\n\
phrust_server_overload_total {}\n\
phrust_server_script_cache_hits_total {}\n\
phrust_server_script_cache_misses_total {}\n\
phrust_server_script_cache_entries {}\n",
            self.requests_total.load(Ordering::Relaxed),
            self.static_responses.load(Ordering::Relaxed),
            self.php_responses.load(Ordering::Relaxed),
            self.four_xx.load(Ordering::Relaxed),
            self.five_xx.load(Ordering::Relaxed),
            in_flight,
            self.body_too_large.load(Ordering::Relaxed),
            self.overload.load(Ordering::Relaxed),
            cache.hits,
            cache.misses,
            cache.entries,
        )
    }
}

#[derive(Debug)]
pub enum ServerError {
    Config(ConfigError),
    Io(std::io::Error),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(f, "{error}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ServerError {}

impl From<ConfigError> for ServerError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
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
    println!("listening http://{local_addr}");
    debug!(%local_addr, docroot=%docroot.display(), "starting phrust server");
    let state = Arc::new(AppState {
        route_config: RouteConfig {
            docroot,
            index: config.index,
            front_controller: config.front_controller,
            metrics_endpoint_enabled: config.metrics_endpoint_enabled,
        },
        max_body_bytes: config.max_body_bytes,
        request_timeout: Duration::from_millis(config.request_timeout_ms),
        in_flight: Arc::new(Semaphore::new(config.max_in_flight)),
        max_in_flight: config.max_in_flight,
        metrics: Arc::new(ServerMetrics::default()),
        executor: PhpExecutor::new(),
        script_cache: Arc::new(if config.script_cache_enabled {
            CompiledScriptCache::new(config.script_cache_shards)
        } else {
            CompiledScriptCache::disabled()
        }),
        local_addr,
    });
    serve_until_shutdown(listener, state).await;
    Ok(())
}

async fn serve_until_shutdown(listener: TcpListener, state: Arc<AppState>) {
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            accept = listener.accept() => {
                let Ok((stream, peer)) = accept else {
                    continue;
                };
                let state = Arc::clone(&state);
                tasks.spawn(async move {
                    let service = service_fn(move |request| {
                        let state = Arc::clone(&state);
                        async move { Ok::<_, Infallible>(handle(request, state, peer).await) }
                    });
                    let io = TokioIo::new(stream);
                    let builder = Builder::new(TokioExecutor::new());
                    let _ = builder.serve_connection(io, service).await;
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

async fn handle(
    request: Request<Incoming>,
    state: Arc<AppState>,
    peer: SocketAddr,
) -> Response<ResponseBody> {
    state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
    let Ok(_permit) = Arc::clone(&state.in_flight).try_acquire_owned() else {
        state.metrics.overload.fetch_add(1, Ordering::Relaxed);
        let response = overloaded();
        state.metrics.record_response(response.status());
        debug!(%peer, "request rejected because max in-flight limit is exhausted");
        return response;
    };
    let (parts, body) = request.into_parts();
    let method = parts.method.clone();
    let route = resolve_route(method.as_str(), parts.uri.path(), &state.route_config);
    debug!(
        %peer,
        method=%method,
        path=%parts.uri.path(),
        route=?route,
        "classified request"
    );
    let response = match route {
        ResolvedRoute::Health => match method {
            Method::GET => response::text(StatusCode::OK, "ok\n"),
            Method::HEAD => response::empty(StatusCode::OK),
            _ => method_not_allowed(),
        },
        ResolvedRoute::Metrics => response::text_dynamic(
            StatusCode::OK,
            state.metrics.render(
                state
                    .max_in_flight
                    .saturating_sub(state.in_flight.available_permits()) as u64,
                state.script_cache.cache_stats(),
            ),
            "text/plain; charset=UTF-8",
        ),
        ResolvedRoute::StaticFile { path, metadata } => {
            state
                .metrics
                .static_responses
                .fetch_add(1, Ordering::Relaxed);
            let content_type = content_type_for(&path);
            if method == Method::HEAD {
                response::static_head(StatusCode::OK, metadata.len(), content_type)
            } else {
                match std::fs::read(&path) {
                    Ok(body) => response::bytes(StatusCode::OK, Bytes::from(body), content_type),
                    Err(_) => response::text(StatusCode::NOT_FOUND, "not found\n"),
                }
            }
        }
        ResolvedRoute::PhpScript {
            script_path,
            path_info,
        } => {
            state.metrics.php_responses.fetch_add(1, Ordering::Relaxed);
            execute_php_request(
                PartsAndBody { parts, body },
                Arc::clone(&state),
                script_path,
                path_info,
                peer,
            )
            .await
        }
        ResolvedRoute::NotFound => response::text(StatusCode::NOT_FOUND, "not found\n"),
        ResolvedRoute::Forbidden => response::text(StatusCode::FORBIDDEN, "forbidden\n"),
        ResolvedRoute::BadRequest => response::text(StatusCode::BAD_REQUEST, "bad request\n"),
        ResolvedRoute::MethodNotAllowed => method_not_allowed(),
    };
    state.metrics.record_response(response.status());
    response
}

struct PartsAndBody {
    parts: Parts,
    body: Incoming,
}

async fn execute_php_request(
    request: PartsAndBody,
    state: Arc<AppState>,
    script_path: PathBuf,
    path_info: Option<String>,
    peer: SocketAddr,
) -> Response<ResponseBody> {
    let PartsAndBody { parts, body } = request;
    let body = match timeout(
        state.request_timeout,
        read_limited_body(body, state.max_body_bytes),
    )
    .await
    {
        Err(_) => {
            return response::text(StatusCode::REQUEST_TIMEOUT, "request timeout\n");
        }
        Ok(Ok(body)) => body,
        Ok(Err(BodyReadError::TooLarge)) => {
            state.metrics.body_too_large.fetch_add(1, Ordering::Relaxed);
            debug!(%peer, max_body_bytes=state.max_body_bytes, "request body too large");
            return response::text(StatusCode::PAYLOAD_TOO_LARGE, "payload too large\n");
        }
        Ok(Err(BodyReadError::Invalid)) => {
            warn!(%peer, "failed to read request body");
            return response::text(StatusCode::BAD_REQUEST, "bad request\n");
        }
    };
    let lookup = match state.compile_script(&script_path) {
        Ok(lookup) => {
            debug!(script=%script_path.display(), hit=lookup.hit, "compiled script cache lookup");
            lookup
        }
        Err(PhpExecutionError::Compile(output)) => {
            return php_output_response(*output, parts.method == Method::HEAD);
        }
        Err(PhpExecutionError::Engine(_)) => {
            warn!(script=%script_path.display(), "php execution engine error");
            return response::text(StatusCode::INTERNAL_SERVER_ERROR, "php execution failed\n");
        }
    };
    let script_name = script_name_for(&state.route_config.docroot, &script_path);
    let request_context = http_runtime_context(
        &parts,
        &state,
        &script_path,
        &script_name,
        path_info,
        &body,
        peer,
    );
    let mut runtime_context = RuntimeContext::controlled_http(request_context)
        .with_cwd(state.route_config.docroot.clone())
        .with_include_path(vec![state.route_config.docroot.clone()]);
    runtime_context = runtime_context.with_stdin(body.clone());
    let output = state.executor.execute_compiled(
        &lookup.compiled,
        PhpRequestExecutionInput {
            real_path: Some(script_path),
            cwd: state.route_config.docroot.clone(),
            include_roots: vec![state.route_config.docroot.clone()],
            runtime_context,
            collect_counters: false,
        },
    );
    php_output_response(output, parts.method == Method::HEAD)
}

fn php_output_response(output: PhpExecutionOutput, is_head: bool) -> Response<ResponseBody> {
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

fn php_transport_response(
    status: StatusCode,
    body: Bytes,
    content_length: usize,
    http_response: &RuntimeHttpResponseState,
) -> Response<ResponseBody> {
    let mut response = Response::builder()
        .status(status)
        .body(Full::new(body).boxed())
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

const PHP_CONTENT_TYPE: &str = "text/html; charset=UTF-8";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BodyReadError {
    TooLarge,
    Invalid,
}

async fn read_limited_body(
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

fn http_runtime_context(
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
    let mut context = RuntimeHttpRequestContext::new(
        parts.method.as_str(),
        host.clone(),
        request_uri,
        script_name.to_string(),
        script_path.to_string_lossy().into_owned(),
        state.route_config.docroot.to_string_lossy().into_owned(),
    );
    context.host = host;
    context.server_port = state.local_addr.port();
    context.server_protocol = format!("{:?}", parts.version);
    context.php_self = php_self_for(script_name, path_info.as_deref());
    context.path_info = path_info;
    context.remote_addr = peer.to_string();
    context.request_time = request_time();
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

fn header_value(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn runtime_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            Some((name.as_str().to_string(), value.to_str().ok()?.to_string()))
        })
        .collect()
}

fn is_form_urlencoded_content_type(value: &str) -> bool {
    value.split(';').next().is_some_and(|media_type| {
        media_type
            .trim()
            .eq_ignore_ascii_case("application/x-www-form-urlencoded")
    })
}

fn script_name_for(docroot: &Path, script_path: &Path) -> String {
    let relative = script_path.strip_prefix(docroot).unwrap_or(script_path);
    let mut value = String::from("/");
    value.push_str(
        &relative
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"),
    );
    value
}

fn php_self_for(script_name: &str, path_info: Option<&str>) -> String {
    path_info.map_or_else(
        || script_name.to_string(),
        |path_info| format!("{script_name}{path_info}"),
    )
}

fn request_time() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

fn method_not_allowed() -> Response<ResponseBody> {
    let mut response = response::text(StatusCode::METHOD_NOT_ALLOWED, "method not allowed\n");
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
    response
}

fn overloaded() -> Response<ResponseBody> {
    let mut response = response::text(StatusCode::SERVICE_UNAVAILABLE, "server overloaded\n");
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
    response
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => "text/css; charset=UTF-8",
        Some("html" | "htm") => "text/html; charset=UTF-8",
        Some("js") => "application/javascript; charset=UTF-8",
        Some("json") => "application/json",
        Some("txt") => "text/plain; charset=UTF-8",
        Some("xml") => "application/xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_cache_records_hit_after_repeated_compile() {
        let fixture = ServerCacheFixture::new();
        fixture.write("<?php echo \"cached\";");
        let cache = Arc::new(CompiledScriptCache::new(1));
        let state = AppState {
            route_config: RouteConfig {
                docroot: fixture.root.clone(),
                index: "index.php".to_string(),
                front_controller: None,
                metrics_endpoint_enabled: true,
            },
            max_body_bytes: 1024,
            request_timeout: Duration::from_secs(30),
            in_flight: Arc::new(Semaphore::new(1)),
            max_in_flight: 1,
            metrics: Arc::new(ServerMetrics::default()),
            executor: PhpExecutor::new(),
            script_cache: Arc::clone(&cache),
            local_addr: "127.0.0.1:8080".parse().expect("local addr"),
        };

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

    struct ServerCacheFixture {
        root: PathBuf,
        path: PathBuf,
    }

    impl ServerCacheFixture {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "phrust-server-cache-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir(&root).expect("create server cache fixture");
            let path = root.join("index.php");
            Self { root, path }
        }

        fn write(&self, source: &str) {
            std::fs::write(&self.path, source).expect("write server cache fixture");
        }
    }

    impl Drop for ServerCacheFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}
