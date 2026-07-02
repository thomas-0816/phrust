use super::state::AppState;
use crate::response::{self, ResponseBody};
use bytes::Bytes;
use hyper::{
    Method, Response, StatusCode, header,
    http::{HeaderMap, request::Parts},
};
use std::{
    fs::Metadata,
    io::SeekFrom,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};

pub(crate) async fn static_file_response(
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
pub(crate) struct StaticFileSelection {
    pub(crate) path: PathBuf,
    pub(crate) metadata: Metadata,
    pub(crate) content_type: &'static str,
    pub(crate) content_encoding: Option<&'static str>,
}

pub(crate) fn select_static_file(
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

pub(crate) fn static_stream_response<R>(
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

pub(crate) fn static_empty_response(
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

pub(crate) fn static_response_builder(
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
pub(crate) struct ByteRange {
    pub(crate) start: u64,
    pub(crate) end: u64,
}

impl ByteRange {
    pub(crate) fn len(self) -> u64 {
        self.end - self.start + 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RangeParseError {
    Invalid,
    Unsatisfiable,
}

pub(crate) fn parse_single_byte_range(
    value: &str,
    full_len: u64,
) -> Result<ByteRange, RangeParseError> {
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

pub(crate) fn static_not_modified(
    headers: &HeaderMap,
    etag: &str,
    modified: Option<SystemTime>,
) -> bool {
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

pub(crate) fn if_none_match_matches(value: &str, etag: &str) -> bool {
    value.split(',').any(|candidate| {
        let candidate = candidate.trim();
        candidate == "*" || candidate == etag || weak_etag_value(candidate) == weak_etag_value(etag)
    })
}

pub(crate) fn weak_etag_value(value: &str) -> &str {
    value.strip_prefix("W/").unwrap_or(value)
}

pub(crate) fn weak_etag(metadata: &Metadata) -> String {
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

pub(crate) fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(crate) fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

pub(crate) fn accepts_encoding(headers: &HeaderMap, encoding: &str) -> bool {
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
pub(crate) fn content_type_for(path: &std::path::Path) -> &'static str {
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
