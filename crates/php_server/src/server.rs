use crate::{
    config::{ConfigError, ServerConfig},
    multipart::{
        MultipartConfig, MultipartError, cleanup_uploaded_files, multipart_boundary,
        parse_multipart_into_context,
    },
    response::{self, ResponseBody},
    routing::{ResolvedRoute, RouteConfig, resolve_route},
    session_store::{SessionStore, generate_session_id, valid_session_id},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::{
    Method, Request, Response, StatusCode,
    body::Incoming,
    header::{self, HeaderName, HeaderValue},
    http::{HeaderMap, request::Parts},
    service::service_fn,
};
use hyper_util::{rt::TokioExecutor, rt::TokioIo, server::conn::auto::Builder};
use php_executor::{
    CompiledScriptCache, CompiledScriptCacheLookup, IncludeCache, IncludeCacheStats,
    OptimizationLevel, PhpExecutionError, PhpExecutionOutput, PhpExecutionStatus, PhpExecutor,
    PhpExecutorOptions, PhpRequestExecutionInput, PhpScriptCacheInput, VmOptions,
};
use php_runtime::api::{
    PHP_SESSION_ACTIVE, RuntimeContext, RuntimeHttpRequestContext, RuntimeHttpResponseState,
    SessionState, parse_cookie_header, parse_form_urlencoded_body,
};
use std::{
    convert::Infallible,
    fmt,
    fs::Metadata,
    io::SeekFrom,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    net::TcpListener,
    sync::Semaphore,
    task::{self, JoinSet},
    time::timeout,
};
use tracing::{debug, warn};

#[derive(Clone, Debug)]
struct AppState {
    route_config: RouteConfig,
    max_body_bytes: usize,
    multipart_config: MultipartConfig,
    request_timeout: Duration,
    execution_time_limit: Option<Duration>,
    in_flight: Arc<Semaphore>,
    max_in_flight: usize,
    metrics: Arc<ServerMetrics>,
    script_cache: Arc<CompiledScriptCache>,
    include_cache: Arc<IncludeCache>,
    session_config: SessionConfig,
    session_store: Arc<SessionStore>,
    session_lock: Arc<Mutex<()>>,
    local_addr: SocketAddr,
}

impl AppState {
    fn compile_script(
        &self,
        script_path: &Path,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
        let executor = PhpExecutor::new();
        self.script_cache.get_or_compile_script(
            &executor,
            PhpScriptCacheInput {
                path: script_path.to_path_buf(),
                source_path: script_path.to_string_lossy().into_owned(),
                optimization_level: OptimizationLevel::O0,
            },
        )
    }
}

#[derive(Clone, Debug)]
struct SessionConfig {
    enabled: bool,
    cookie_name: String,
    cookie_path: String,
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
    uploads_total: AtomicU64,
    upload_parse_errors: AtomicU64,
    upload_bytes_accepted: AtomicU64,
    upload_files_rejected: AtomicU64,
    execution_timeouts: AtomicU64,
    execution_deadline_disabled: AtomicU64,
    static_streamed_bytes: AtomicU64,
    static_not_modified: AtomicU64,
    static_partial_responses: AtomicU64,
    static_precompressed_hits: AtomicU64,
    script_cache_preload_successes: AtomicU64,
    script_cache_preload_failures: AtomicU64,
}

impl ServerMetrics {
    fn record_response(&self, status: StatusCode) {
        if status.is_client_error() {
            self.four_xx.fetch_add(1, Ordering::Relaxed);
        } else if status.is_server_error() {
            self.five_xx.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn render(
        &self,
        in_flight: u64,
        cache: php_executor::CompiledScriptCacheStats,
        include_cache: IncludeCacheStats,
    ) -> String {
        let shard_entries = cache
            .entries_by_shard
            .iter()
            .enumerate()
            .map(|(shard, entries)| {
                format!("phrust_server_script_cache_shard_entries{{shard=\"{shard}\"}} {entries}\n")
            })
            .collect::<String>();
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
phrust_server_uploads_total {}\n\
phrust_server_upload_parse_errors_total {}\n\
phrust_server_upload_bytes_accepted_total {}\n\
phrust_server_upload_files_rejected_total {}\n\
phrust_server_execution_timeouts_total {}\n\
phrust_server_execution_deadline_disabled_total {}\n\
phrust_server_static_streamed_bytes_total {}\n\
phrust_server_static_not_modified_total {}\n\
phrust_server_static_partial_responses_total {}\n\
phrust_server_static_precompressed_hits_total {}\n\
phrust_server_script_cache_hits_total {}\n\
phrust_server_script_cache_misses_total {}\n\
phrust_server_script_cache_stale_invalidations_total {}\n\
phrust_server_script_cache_compile_errors_total {}\n\
phrust_server_script_cache_entries {}\n\
phrust_server_script_cache_evictions_total {}\n\
phrust_server_script_cache_compile_in_progress {}\n\
{}\
phrust_server_script_cache_preload_successes_total {}\n\
phrust_server_script_cache_preload_failures_total {}\n\
phrust_server_include_resolution_hits_total {}\n\
phrust_server_include_resolution_misses_total {}\n\
phrust_server_include_compile_hits_total {}\n\
phrust_server_include_compile_misses_total {}\n\
phrust_server_include_stale_invalidations_total {}\n\
phrust_server_include_compile_errors_total {}\n",
            self.requests_total.load(Ordering::Relaxed),
            self.static_responses.load(Ordering::Relaxed),
            self.php_responses.load(Ordering::Relaxed),
            self.four_xx.load(Ordering::Relaxed),
            self.five_xx.load(Ordering::Relaxed),
            in_flight,
            self.body_too_large.load(Ordering::Relaxed),
            self.overload.load(Ordering::Relaxed),
            self.uploads_total.load(Ordering::Relaxed),
            self.upload_parse_errors.load(Ordering::Relaxed),
            self.upload_bytes_accepted.load(Ordering::Relaxed),
            self.upload_files_rejected.load(Ordering::Relaxed),
            self.execution_timeouts.load(Ordering::Relaxed),
            self.execution_deadline_disabled.load(Ordering::Relaxed),
            self.static_streamed_bytes.load(Ordering::Relaxed),
            self.static_not_modified.load(Ordering::Relaxed),
            self.static_partial_responses.load(Ordering::Relaxed),
            self.static_precompressed_hits.load(Ordering::Relaxed),
            cache.hits,
            cache.misses,
            cache.stale_invalidations,
            cache.compile_errors,
            cache.entries,
            cache.evictions,
            cache.compile_in_progress,
            shard_entries,
            self.script_cache_preload_successes.load(Ordering::Relaxed),
            self.script_cache_preload_failures.load(Ordering::Relaxed),
            include_cache.resolution_hits,
            include_cache.resolution_misses,
            include_cache.compile_hits,
            include_cache.compile_misses,
            include_cache.stale_invalidations,
            include_cache.compile_errors,
        )
    }
}

#[derive(Debug)]
pub enum ServerError {
    Config(ConfigError),
    Io(std::io::Error),
    Preload(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(f, "{error}"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Preload(error) => write!(f, "{error}"),
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
    let script_cache_preload = config.script_cache_preload.clone();
    let strict_preload = config.strict_preload;
    let session_store = Arc::new(SessionStore::new(config.session_save_path));
    if config.sessions_enabled {
        session_store
            .ensure_ready()
            .map_err(std::io::Error::other)?;
    }
    println!("listening http://{local_addr}");
    debug!(%local_addr, docroot=%docroot.display(), "starting phrust server");
    let state = Arc::new(AppState {
        route_config: RouteConfig {
            docroot,
            index: config.index,
            front_controller: config.front_controller,
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
        script_cache: Arc::new(if config.script_cache_enabled {
            CompiledScriptCache::new_with_limits(
                config.script_cache_shards,
                config.script_cache_max_entries,
                Duration::from_millis(config.script_cache_check_interval_ms),
            )
        } else {
            CompiledScriptCache::disabled()
        }),
        include_cache: Arc::new(IncludeCache::new(config.script_cache_shards)),
        session_config: SessionConfig {
            enabled: config.sessions_enabled,
            cookie_name: config.session_cookie_name,
            cookie_path: config.session_cookie_path,
        },
        session_store,
        session_lock: Arc::new(Mutex::new(())),
        local_addr,
    });
    preload_script_cache(&state, script_cache_preload.as_deref(), strict_preload)?;
    serve_until_shutdown(listener, state).await;
    Ok(())
}

fn preload_script_cache(
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
    let route =
        match resolve_route_on_blocking_thread(method.as_str(), parts.uri.path(), &state).await {
            Ok(route) => route,
            Err(error) => {
                warn!(%peer, %error, "route resolution task failed");
                let response = response::text(StatusCode::INTERNAL_SERVER_ERROR, "server error\n");
                state.metrics.record_response(response.status());
                return response;
            }
        };
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
                state.include_cache.cache_stats(),
            ),
            "text/plain; charset=UTF-8",
        ),
        ResolvedRoute::CacheClear => clear_cache_response(&state, peer),
        ResolvedRoute::StaticFile { path, metadata } => {
            state
                .metrics
                .static_responses
                .fetch_add(1, Ordering::Relaxed);
            static_file_response(&parts, &state, path, metadata).await
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

async fn static_file_response(
    parts: &Parts,
    state: &AppState,
    original_path: PathBuf,
    original_metadata: Metadata,
) -> Response<ResponseBody> {
    let selection = select_static_file(
        &state.route_config.docroot,
        original_path,
        original_metadata,
        &parts.headers,
    );
    let etag = weak_etag(&selection.metadata);
    let last_modified = selection
        .metadata
        .modified()
        .ok()
        .map(httpdate::fmt_http_date);
    if static_not_modified(&parts.headers, &etag, selection.metadata.modified().ok()) {
        state
            .metrics
            .static_not_modified
            .fetch_add(1, Ordering::Relaxed);
        return static_empty_response(
            StatusCode::NOT_MODIFIED,
            &selection,
            &etag,
            last_modified.as_deref(),
            None,
            None,
        );
    }

    let full_len = selection.metadata.len();
    let mut status = StatusCode::OK;
    let mut start = 0;
    let mut content_len = full_len;
    let mut content_range = None;
    if let Some(range_value) = parts.headers.get(header::RANGE) {
        match range_value
            .to_str()
            .ok()
            .and_then(|value| parse_single_byte_range(value, full_len).ok())
        {
            Some(range) => {
                status = StatusCode::PARTIAL_CONTENT;
                start = range.start;
                content_len = range.len();
                content_range = Some(format!("bytes {}-{}/{}", range.start, range.end, full_len));
                state
                    .metrics
                    .static_partial_responses
                    .fetch_add(1, Ordering::Relaxed);
            }
            None => {
                let content_range = format!("bytes */{full_len}");
                return static_empty_response(
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    &selection,
                    &etag,
                    last_modified.as_deref(),
                    Some(0),
                    Some(&content_range),
                );
            }
        }
    }

    if selection.content_encoding.is_some() {
        state
            .metrics
            .static_precompressed_hits
            .fetch_add(1, Ordering::Relaxed);
    }

    let content_range = content_range.as_deref();
    if parts.method == Method::HEAD {
        return static_empty_response(
            status,
            &selection,
            &etag,
            last_modified.as_deref(),
            Some(content_len),
            content_range,
        );
    }

    let mut file = match File::open(&selection.path).await {
        Ok(file) => file,
        Err(_) => return response::text(StatusCode::NOT_FOUND, "not found\n"),
    };
    if start > 0 && file.seek(SeekFrom::Start(start)).await.is_err() {
        return response::text(StatusCode::INTERNAL_SERVER_ERROR, "static file failed\n");
    }
    state
        .metrics
        .static_streamed_bytes
        .fetch_add(content_len, Ordering::Relaxed);
    static_stream_response(
        status,
        &selection,
        &etag,
        last_modified.as_deref(),
        content_len,
        content_range,
        file.take(content_len),
    )
}

#[derive(Clone, Debug)]
struct StaticFileSelection {
    path: PathBuf,
    metadata: Metadata,
    content_type: &'static str,
    content_encoding: Option<&'static str>,
}

fn select_static_file(
    docroot: &Path,
    original_path: PathBuf,
    original_metadata: Metadata,
    headers: &HeaderMap,
) -> StaticFileSelection {
    let content_type = content_type_for(&original_path);
    for candidate in [
        ("br", ".br", "br"),
        ("zstd", ".zst", "zstd"),
        ("gzip", ".gz", "gzip"),
    ] {
        let (accepted_encoding, suffix, content_encoding) = candidate;
        if !accepts_encoding(headers, accepted_encoding) {
            continue;
        }
        let compressed_path = append_suffix(&original_path, suffix);
        let Ok(canonical) = compressed_path.canonicalize() else {
            continue;
        };
        if !canonical.starts_with(docroot) {
            continue;
        }
        let Ok(metadata) = canonical.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        return StaticFileSelection {
            path: canonical,
            metadata,
            content_type,
            content_encoding: Some(content_encoding),
        };
    }
    StaticFileSelection {
        path: original_path,
        metadata: original_metadata,
        content_type,
        content_encoding: None,
    }
}

fn static_stream_response<R>(
    status: StatusCode,
    selection: &StaticFileSelection,
    etag: &str,
    last_modified: Option<&str>,
    content_len: u64,
    content_range: Option<&str>,
    reader: R,
) -> Response<ResponseBody>
where
    R: tokio::io::AsyncRead + Send + Sync + 'static,
{
    let builder = static_response_builder(
        status,
        selection,
        etag,
        last_modified,
        Some(content_len),
        content_range,
    );
    builder
        .body(response::reader_body(reader))
        .expect("static stream response builder is valid")
}

fn static_empty_response(
    status: StatusCode,
    selection: &StaticFileSelection,
    etag: &str,
    last_modified: Option<&str>,
    content_len: Option<u64>,
    content_range: Option<&str>,
) -> Response<ResponseBody> {
    static_response_builder(
        status,
        selection,
        etag,
        last_modified,
        content_len,
        content_range,
    )
    .body(response::full_body(Bytes::new()))
    .expect("static empty response builder is valid")
}

fn static_response_builder(
    status: StatusCode,
    selection: &StaticFileSelection,
    etag: &str,
    last_modified: Option<&str>,
    content_len: Option<u64>,
    content_range: Option<&str>,
) -> hyper::http::response::Builder {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, selection.content_type)
        .header(header::ETAG, etag)
        .header(header::ACCEPT_RANGES, "bytes");
    if let Some(content_len) = content_len {
        builder = builder.header(header::CONTENT_LENGTH, content_len.to_string());
    }
    if let Some(last_modified) = last_modified {
        builder = builder.header(header::LAST_MODIFIED, last_modified);
    }
    if let Some(content_encoding) = selection.content_encoding {
        builder = builder
            .header(header::CONTENT_ENCODING, content_encoding)
            .header(header::VARY, "Accept-Encoding");
    }
    if let Some(content_range) = content_range {
        builder = builder.header(header::CONTENT_RANGE, content_range);
    }
    builder
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    fn len(self) -> u64 {
        self.end - self.start + 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RangeParseError {
    Invalid,
    Unsatisfiable,
}

fn parse_single_byte_range(value: &str, full_len: u64) -> Result<ByteRange, RangeParseError> {
    let Some(range) = value.trim().strip_prefix("bytes=") else {
        return Err(RangeParseError::Invalid);
    };
    if range.contains(',') || full_len == 0 {
        return Err(RangeParseError::Unsatisfiable);
    }
    let Some((start, end)) = range.split_once('-') else {
        return Err(RangeParseError::Invalid);
    };
    if start.is_empty() {
        let suffix_len = end.parse::<u64>().map_err(|_| RangeParseError::Invalid)?;
        if suffix_len == 0 {
            return Err(RangeParseError::Invalid);
        }
        let start = full_len.saturating_sub(suffix_len);
        return Ok(ByteRange {
            start,
            end: full_len - 1,
        });
    }
    let start = start.parse::<u64>().map_err(|_| RangeParseError::Invalid)?;
    if start >= full_len {
        return Err(RangeParseError::Unsatisfiable);
    }
    let end = if end.is_empty() {
        full_len - 1
    } else {
        end.parse::<u64>().map_err(|_| RangeParseError::Invalid)?
    };
    if end < start {
        return Err(RangeParseError::Invalid);
    }
    Ok(ByteRange {
        start,
        end: end.min(full_len - 1),
    })
}

fn static_not_modified(headers: &HeaderMap, etag: &str, modified: Option<SystemTime>) -> bool {
    if let Some(if_none_match) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
    {
        return if_none_match_matches(if_none_match, etag);
    }
    let Some(modified) = modified else {
        return false;
    };
    let Some(if_modified_since) = headers
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| httpdate::parse_http_date(value).ok())
    else {
        return false;
    };
    unix_seconds(modified) <= unix_seconds(if_modified_since)
}

fn if_none_match_matches(value: &str, etag: &str) -> bool {
    value.split(',').any(|candidate| {
        let candidate = candidate.trim();
        candidate == "*" || candidate == etag || weak_etag_value(candidate) == weak_etag_value(etag)
    })
}

fn weak_etag_value(value: &str) -> &str {
    value.strip_prefix("W/").unwrap_or(value)
}

fn weak_etag(metadata: &Metadata) -> String {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    match metadata_inode(metadata) {
        Some(inode) => format!("W/\"{:x}-{:x}-{:x}\"", metadata.len(), modified, inode),
        None => format!("W/\"{:x}-{:x}\"", metadata.len(), modified),
    }
}

#[cfg(unix)]
fn metadata_inode(metadata: &Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.ino())
}

#[cfg(not(unix))]
fn metadata_inode(_metadata: &Metadata) -> Option<u64> {
    None
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn accepts_encoding(headers: &HeaderMap, encoding: &str) -> bool {
    headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(',').any(|part| {
                let mut parameters = part.split(';');
                let token = parameters
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                let accepted = token == encoding || (encoding == "zstd" && token == "zst");
                accepted
                    && !parameters.any(|parameter| {
                        let Some((name, value)) = parameter.trim().split_once('=') else {
                            return false;
                        };
                        name.trim().eq_ignore_ascii_case("q") && value.trim() == "0"
                    })
            })
        })
}

fn clear_cache_response(state: &AppState, peer: SocketAddr) -> Response<ResponseBody> {
    if !peer.ip().is_loopback() {
        return response::text(StatusCode::FORBIDDEN, "forbidden\n");
    }
    state.script_cache.clear();
    state.include_cache.clear();
    response::text(StatusCode::OK, "cache cleared\n")
}

struct PartsAndBody {
    parts: Parts,
    body: Incoming,
}

async fn resolve_route_on_blocking_thread(
    method: &str,
    path: &str,
    state: &AppState,
) -> Result<ResolvedRoute, task::JoinError> {
    let method = method.to_string();
    let path = path.to_string();
    let route_config = state.route_config.clone();
    task::spawn_blocking(move || resolve_route(&method, &path, &route_config)).await
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
        Err(error) => return multipart_error_response(error, &state, peer),
    } {
        match parse_multipart_into_context(
            &mut request_context,
            &body,
            &boundary,
            &state.multipart_config,
        ) {
            Ok(stats) => {
                state
                    .metrics
                    .uploads_total
                    .fetch_add(stats.uploads_total, Ordering::Relaxed);
                state
                    .metrics
                    .upload_bytes_accepted
                    .fetch_add(stats.upload_bytes_accepted, Ordering::Relaxed);
            }
            Err(error) => return multipart_error_response(error, &state, peer),
        }
    }
    let upload_cleanup = request_context.uploaded_files.clone();
    let _session_guard = if state.session_config.enabled {
        Some(state.session_lock.lock().expect("session lock poisoned"))
    } else {
        None
    };
    let session_state = match seed_session_state(&request_context, &state) {
        Ok(session) => session,
        Err(error) => {
            warn!(%peer, error=%error, "session state preparation failed");
            return response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session storage failed\n",
            );
        }
    };
    let mut runtime_context = RuntimeContext::controlled_http(request_context)
        .with_cwd(state.route_config.docroot.clone())
        .with_include_path(vec![state.route_config.docroot.clone()])
        .with_session_state(session_state)
        .with_execution_time_limit(state.execution_time_limit);
    if state.execution_time_limit.is_none() {
        state
            .metrics
            .execution_deadline_disabled
            .fetch_add(1, Ordering::Relaxed);
    }
    runtime_context = runtime_context.with_stdin(body.clone());
    let is_head = parts.method == Method::HEAD;
    let script_log_path = script_path.clone();
    let result = execute_php_in_blocking_region(Arc::clone(&state), script_path, runtime_context);
    match result {
        Ok((lookup, mut output)) => {
            output.upload_registry.cleanup_unmoved();
            if let Err(error) = finalize_session_state(&mut output, &state) {
                warn!(%peer, error=%error, "session state finalization failed");
                return response::text(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session storage failed\n",
                );
            }
            if php_execution_timed_out(&output) {
                state
                    .metrics
                    .execution_timeouts
                    .fetch_add(1, Ordering::Relaxed);
                return php_timeout_response(is_head, &output.http_response);
            }
            debug!(script=%script_log_path.display(), hit=lookup.hit, "compiled script cache lookup");
            php_output_response(output, is_head)
        }
        Err(PhpExecutionError::Compile(output)) => {
            cleanup_uploaded_files(&upload_cleanup);
            php_output_response(*output, is_head)
        }
        Err(PhpExecutionError::Engine(error)) => {
            cleanup_uploaded_files(&upload_cleanup);
            warn!(script=%script_log_path.display(), %error, "php execution engine error");
            response::text(StatusCode::INTERNAL_SERVER_ERROR, "php execution failed\n")
        }
    }
}

fn execute_php_in_blocking_region(
    state: Arc<AppState>,
    script_path: PathBuf,
    runtime_context: RuntimeContext,
) -> Result<(CompiledScriptCacheLookup, PhpExecutionOutput), PhpExecutionError> {
    task::block_in_place(move || {
        let lookup = state.compile_script(&script_path)?;
        let executor = PhpExecutor::with_options(PhpExecutorOptions {
            vm_options: VmOptions {
                include_cache: Some(Arc::clone(&state.include_cache)),
                ..VmOptions::default()
            },
            ..PhpExecutorOptions::default()
        });
        let output = executor.execute_compiled(
            &lookup.compiled,
            PhpRequestExecutionInput {
                real_path: Some(script_path),
                cwd: state.route_config.docroot.clone(),
                include_roots: include_roots_for_docroot(&state.route_config.docroot),
                runtime_context,
                collect_counters: false,
            },
        );
        Ok((lookup, output))
    })
}

fn seed_session_state(
    request: &RuntimeHttpRequestContext,
    state: &AppState,
) -> Result<SessionState, String> {
    if !state.session_config.enabled {
        return Ok(SessionState::default());
    }
    let incoming_id = request
        .parsed_cookie
        .iter()
        .rev()
        .find(|(name, _)| name == &state.session_config.cookie_name)
        .map(|(_, value)| value.as_str())
        .filter(|value| valid_session_id(value))
        .unwrap_or("");
    let data = if incoming_id.is_empty() {
        php_runtime::PhpArray::new()
    } else {
        state
            .session_store
            .load(incoming_id)
            .map_err(|error| error.to_string())?
    };
    let generated_id = generate_session_id().map_err(|error| error.to_string())?;
    Ok(SessionState::seeded(
        state.session_config.cookie_name.clone(),
        incoming_id.to_string(),
        data,
        Some(generated_id),
    ))
}

fn finalize_session_state(output: &mut PhpExecutionOutput, state: &AppState) -> Result<(), String> {
    if !state.session_config.enabled {
        return Ok(());
    }
    if output.session.destroyed() {
        if let Some(id) = output.session.destroyed_id() {
            state
                .session_store
                .delete(id)
                .map_err(|error| error.to_string())?;
        }
        return Ok(());
    }
    if output.session.status() != PHP_SESSION_ACTIVE || output.session.id().is_empty() {
        return Ok(());
    }
    state
        .session_store
        .save(output.session.id(), &output.session.data())
        .map_err(|error| error.to_string())?;
    if output.session.newly_created() {
        output
            .http_response
            .add_header_line(
                &format!(
                    "Set-Cookie: {}={}; Path={}; HttpOnly",
                    state.session_config.cookie_name,
                    output.session.id(),
                    state.session_config.cookie_path
                ),
                false,
                None,
            )
            .map_err(|message| format!("session cookie header failed: {message}"))?;
    }
    Ok(())
}

fn multipart_error_response(
    error: MultipartError,
    state: &AppState,
    peer: SocketAddr,
) -> Response<ResponseBody> {
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

fn php_execution_timed_out(output: &PhpExecutionOutput) -> bool {
    output
        .runtime_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.id() == "E_PHP_VM_EXECUTION_TIMEOUT")
}

fn php_timeout_response(
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

fn php_transport_response(
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

fn include_roots_for_docroot(docroot: &Path) -> Vec<PathBuf> {
    let mut roots = vec![docroot.to_path_buf()];
    if let Some(parent) = docroot.parent()
        && parent != docroot
    {
        roots.push(parent.to_path_buf());
    }
    roots
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
            script_cache: cache,
            include_cache: Arc::new(IncludeCache::new(1)),
            session_config: SessionConfig {
                enabled: false,
                cookie_name: "PHPSESSID".to_string(),
                cookie_path: "/".to_string(),
            },
            session_store: Arc::new(SessionStore::new(fixture.root.join("sessions"))),
            session_lock: Arc::new(Mutex::new(())),
            local_addr: "127.0.0.1:8080".parse().expect("local addr"),
        }
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
}
