use super::state::AppState;
use crate::routing::ResolvedRoute;
use hyper::http::{HeaderMap, HeaderName, request::Parts};
use php_diagnostics::{DebugEvent, DiagnosticLayer, DiagnosticOutputFormat, DiagnosticPhase};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};
use tracing::warn;

pub(crate) fn route_debug_name(route: &ResolvedRoute) -> &'static str {
    match route {
        ResolvedRoute::Health => "health",
        ResolvedRoute::Metrics => "metrics",
        ResolvedRoute::CacheClear => "cache-clear",
        ResolvedRoute::StaticFile { .. } => "static",
        ResolvedRoute::PhpScript { path_info, .. } if path_info.is_some() => "front-controller",
        ResolvedRoute::PhpScript { .. } => "php",
        ResolvedRoute::NotFound => "not-found",
        ResolvedRoute::Forbidden => "forbidden",
        ResolvedRoute::BadRequest => "bad-request",
        ResolvedRoute::MethodNotAllowed => "method-not-allowed",
    }
}

pub(crate) fn header_debug_value(headers: &HeaderMap, name: HeaderName) -> String {
    if headers.contains_key(name) {
        "[redacted]".to_string()
    } else {
        "absent".to_string()
    }
}

pub(crate) struct RequestDiagnostic<'a> {
    pub(crate) code: &'a str,
    pub(crate) phase: &'a str,
    pub(crate) message: &'a str,
    pub(crate) function_name: &'a str,
    pub(crate) path_attempted: &'a str,
    pub(crate) script_filename: &'a str,
}

impl<'a> RequestDiagnostic<'a> {
    pub(crate) fn new(
        code: &'a str,
        phase: &'a str,
        message: &'a str,
        function_name: &'a str,
        path_attempted: &'a str,
        script_filename: &'a str,
    ) -> Self {
        Self {
            code,
            phase,
            message,
            function_name,
            path_attempted,
            script_filename,
        }
    }
}

pub(crate) fn emit_request_diagnostic(
    state: &AppState,
    parts: &Parts,
    request_id: Option<&str>,
    diagnostic: RequestDiagnostic<'_>,
) {
    if !state.debug {
        return;
    }
    let uri = parts.uri.path_and_query().map_or_else(
        || parts.uri.path().to_string(),
        |value| value.as_str().to_string(),
    );
    let document_root = state.route_config.docroot.display().to_string();
    let allowed_roots = include_roots_for_docroot(&state.route_config.docroot)
        .into_iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(",");
    emit_server_debug(
        state,
        request_id,
        diagnostic.code,
        diagnostic.phase,
        diagnostic.message,
        BTreeMap::from([
            ("method".to_string(), parts.method.to_string()),
            ("uri".to_string(), uri),
            (
                "script_filename".to_string(),
                diagnostic.script_filename.to_string(),
            ),
            ("document_root".to_string(), document_root.clone()),
            ("cwd".to_string(), document_root),
            (
                "path_attempted".to_string(),
                diagnostic.path_attempted.to_string(),
            ),
            ("allowed_roots".to_string(), allowed_roots),
            (
                "function_name".to_string(),
                diagnostic.function_name.to_string(),
            ),
        ]),
    );
}

pub(crate) fn emit_server_debug(
    state: &AppState,
    request_id: Option<&str>,
    code: &str,
    phase: &str,
    message: &str,
    mut context: BTreeMap<String, String>,
) {
    if !state.debug {
        return;
    }
    if let Some(request_id) = request_id {
        context.insert("request_id".to_string(), request_id.to_string());
    }
    let event = DebugEvent::new(
        code,
        DiagnosticLayer::server(),
        DiagnosticPhase::new(phase),
        message,
    )
    .with_context(context);
    let rendered = match state.error_format {
        DiagnosticOutputFormat::Text => {
            let mut line = event.text_line();
            line.push('\n');
            line
        }
        DiagnosticOutputFormat::Json => match event.json_line() {
            Ok(line) => line,
            Err(error) => {
                warn!(%error, "failed to serialize server debug event");
                return;
            }
        },
    };
    if let Some(path) = &state.debug_log {
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(rendered.as_bytes()) {
                    warn!(path=%path.display(), %error, "failed to write server debug log");
                }
            }
            Err(error) => warn!(path=%path.display(), %error, "failed to open server debug log"),
        }
    } else {
        eprint!("{rendered}");
    }
}

pub(crate) fn emit_server_debug_lazy<F>(
    state: &AppState,
    request_id: Option<&str>,
    code: &str,
    phase: &str,
    message: &str,
    context: F,
) where
    F: FnOnce() -> BTreeMap<String, String>,
{
    if !state.debug {
        return;
    }
    emit_server_debug(state, request_id, code, phase, message, context());
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
