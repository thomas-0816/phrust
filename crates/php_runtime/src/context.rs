//! Deterministic runtime configuration for CLI fixture execution.

use crate::{
    ArrayKey, FilesystemCapabilities, IniRegistry, PhpArray, PhpString, SessionState, Value,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Minimal ini-like runtime options carried by the VM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeIniOptions {
    /// Characters accepted as query-string input separators.
    pub arg_separator_input: String,
    /// Placeholder for PHP's `error_reporting` bitmask.
    pub error_reporting: ErrorReporting,
    /// Placeholder for display_errors-style behavior.
    pub display_errors: bool,
    /// Default input filter applied while materializing request superglobals.
    pub default_input_filter: RuntimeInputFilter,
    /// Flags applied by PHP's deprecated `filter.default_flags` directive.
    pub default_input_filter_flags: i64,
    /// Maximum decoded input variables materialized into each superglobal.
    pub max_input_vars: usize,
    /// Maximum PHP-style bracket nesting materialized for input names.
    pub max_input_nesting_level: usize,
}

impl Default for RuntimeIniOptions {
    fn default() -> Self {
        Self {
            arg_separator_input: "&".to_string(),
            error_reporting: ErrorReporting::default(),
            display_errors: true,
            default_input_filter: RuntimeInputFilter::UnsafeRaw,
            default_input_filter_flags: 0,
            max_input_vars: 1000,
            max_input_nesting_level: 64,
        }
    }
}

/// Runtime subset of PHP's `filter.default` INI directive for request input.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RuntimeInputFilter {
    /// Preserve decoded input values without additional filtering.
    #[default]
    UnsafeRaw,
    /// Strip simple HTML/XML tags from decoded input values.
    Stripped,
    /// Encode special characters using decimal HTML entities.
    SpecialChars,
}

impl RuntimeInputFilter {
    /// Parses the stable filter names accepted by PHP's `filter.default`.
    #[must_use]
    pub fn from_ini_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "unsafe_raw" => Some(Self::UnsafeRaw),
            "string" | "stripped" => Some(Self::Stripped),
            "special_chars" | "full_special_chars" => Some(Self::SpecialChars),
            _ => None,
        }
    }
}

/// Minimal error_reporting placeholder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErrorReporting {
    /// Stored mask. The runtime VM does not interpret it yet.
    pub mask: i64,
}

impl Default for ErrorReporting {
    fn default() -> Self {
        Self { mask: -1 }
    }
}

/// Per-file or per-function strict_types metadata placeholder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictTypesInfo {
    /// Stable file or function key.
    pub subject: String,
    /// Whether strict_types is enabled for the subject.
    pub enabled: bool,
}

/// Runtime request mode used to seed deterministic superglobals.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum RuntimeRequestMode {
    /// CLI execution with argv-derived `_SERVER` values.
    #[default]
    Cli,
    /// HTTP request execution with request-derived superglobals.
    Http(Box<RuntimeHttpRequestContext>),
}

/// Owned HTTP request metadata carried by the runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeHttpRequestContext {
    pub method: String,
    pub scheme: String,
    pub host: String,
    pub server_name: String,
    pub server_addr: String,
    pub server_port: u16,
    pub server_protocol: String,
    pub server_software: String,
    pub gateway_interface: String,
    pub https: bool,
    pub request_uri: String,
    pub path: String,
    pub query_string: String,
    pub script_name: String,
    pub php_self: String,
    pub script_filename: String,
    pub document_root: String,
    pub path_info: Option<String>,
    pub remote_addr: String,
    pub remote_port: Option<u16>,
    pub auth_type: Option<String>,
    pub remote_user: Option<String>,
    pub php_auth_user: Option<String>,
    pub php_auth_pw: Option<String>,
    pub request_time: i64,
    pub request_time_float_micros: i64,
    pub headers: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    pub parsed_get: Vec<(String, String)>,
    pub parsed_post: Vec<(String, String)>,
    pub parsed_cookie: Vec<(String, String)>,
    pub uploaded_files: Vec<RuntimeUploadedFile>,
    pub raw_body: Arc<[u8]>,
}

/// One uploaded file accepted by the integrated HTTP server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeUploadedFile {
    pub field_name: String,
    pub client_filename: String,
    pub content_type: String,
    pub temp_path: String,
    pub error: i64,
    pub size: u64,
}

/// Request-local registry of temp files accepted by the upload parser.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UploadRegistry {
    entries: Vec<UploadRegistryEntry>,
}

/// One tracked upload temp file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UploadRegistryEntry {
    temp_path: String,
    moved: bool,
}

impl UploadRegistry {
    #[must_use]
    pub fn from_uploaded_files(files: &[RuntimeUploadedFile]) -> Self {
        Self {
            entries: files
                .iter()
                .map(|file| UploadRegistryEntry {
                    temp_path: file.temp_path.clone(),
                    moved: false,
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn is_active_upload(&self, path: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.temp_path == path && !entry.moved)
    }

    pub fn mark_moved(&mut self, path: &str) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.temp_path == path && !entry.moved)
        else {
            return false;
        };
        entry.moved = true;
        true
    }

    #[must_use]
    pub fn unmoved_temp_paths(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|entry| !entry.moved)
            .map(|entry| entry.temp_path.as_str())
            .collect()
    }

    pub fn cleanup_unmoved(&self) {
        for path in self.unmoved_temp_paths() {
            let _ = fs::remove_file(path);
        }
    }
}

/// One HTTP response header set by PHP code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeHttpHeader {
    pub name: String,
    pub value: String,
}

/// Request-local HTTP response state for web execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeHttpResponseState {
    pub status_code: u16,
    pub headers: Vec<RuntimeHttpHeader>,
    pub headers_sent: bool,
}

impl Default for RuntimeHttpResponseState {
    fn default() -> Self {
        Self {
            status_code: 200,
            headers: Vec::new(),
            headers_sent: false,
        }
    }
}

impl RuntimeHttpResponseState {
    #[must_use]
    pub fn headers_list(&self) -> Vec<String> {
        self.headers
            .iter()
            .map(|header| format!("{}: {}", header.name, header.value))
            .collect()
    }

    pub fn set_status_code(&mut self, status_code: u16) -> bool {
        if !(100..=599).contains(&status_code) {
            return false;
        }
        self.status_code = status_code;
        true
    }

    pub fn add_header_line(
        &mut self,
        line: &str,
        replace: bool,
        response_code: Option<u16>,
    ) -> Result<(), String> {
        reject_response_splitting(line)?;
        if let Some(status) = response_code.filter(|status| *status != 0)
            && !self.set_status_code(status)
        {
            return Err(format!("invalid HTTP response code {status}"));
        }
        if let Some(status) = parse_status_line(line) {
            if !self.set_status_code(status) {
                return Err(format!("invalid HTTP response code {status}"));
            }
            return Ok(());
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "header line must contain `:`".to_string())?;
        let name = name.trim();
        let value = value.trim();
        validate_header_name(name)?;
        if replace {
            self.headers
                .retain(|header| !header.name.eq_ignore_ascii_case(name));
        }
        self.headers.push(RuntimeHttpHeader {
            name: name.to_string(),
            value: value.to_string(),
        });
        Ok(())
    }

    pub fn remove_header(&mut self, name: Option<&str>) -> Result<(), String> {
        let Some(name) = name else {
            self.headers.clear();
            return Ok(());
        };
        reject_response_splitting(name)?;
        validate_header_name(name)?;
        self.headers
            .retain(|header| !header.name.eq_ignore_ascii_case(name));
        Ok(())
    }
}

impl RuntimeHttpRequestContext {
    #[must_use]
    pub fn new(
        method: impl Into<String>,
        host: impl Into<String>,
        request_uri: impl Into<String>,
        script_name: impl Into<String>,
        script_filename: impl Into<String>,
        document_root: impl Into<String>,
    ) -> Self {
        let request_uri = request_uri.into();
        let query_string = request_uri
            .split_once('?')
            .map_or("", |(_, query)| query)
            .to_string();
        let path = request_uri
            .split_once('?')
            .map_or(request_uri.as_str(), |(path, _)| path)
            .to_string();
        let host = host.into();
        Self {
            method: method.into(),
            scheme: "http".to_string(),
            server_name: server_name_from_host(&host),
            server_addr: String::new(),
            host,
            server_port: 80,
            server_protocol: "HTTP/1.1".to_string(),
            server_software: "phrust-server".to_string(),
            gateway_interface: "CGI/1.1".to_string(),
            https: false,
            request_uri,
            path,
            query_string: query_string.clone(),
            script_name: script_name.into(),
            php_self: String::new(),
            script_filename: script_filename.into(),
            document_root: document_root.into(),
            path_info: None,
            remote_addr: String::new(),
            remote_port: None,
            auth_type: None,
            remote_user: None,
            php_auth_user: None,
            php_auth_pw: None,
            request_time: 0,
            request_time_float_micros: 0,
            headers: Vec::new(),
            content_type: None,
            content_length: None,
            parsed_get: parse_query_string(&query_string),
            parsed_post: Vec::new(),
            parsed_cookie: Vec::new(),
            uploaded_files: Vec::new(),
            raw_body: Arc::from([]),
        }
    }

    #[must_use]
    pub fn php_self(&self) -> &str {
        if self.php_self.is_empty() {
            &self.script_name
        } else {
            &self.php_self
        }
    }
}

fn reject_response_splitting(value: &str) -> Result<(), String> {
    if value.contains('\r') || value.contains('\n') {
        Err("header line must not contain CR or LF".to_string())
    } else {
        Ok(())
    }
}

fn validate_header_name(name: &str) -> Result<(), String> {
    if name.is_empty() || !name.bytes().all(is_header_name_byte) {
        return Err(format!("invalid HTTP header name `{name}`"));
    }
    Ok(())
}

fn is_header_name_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
            | b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
    )
}

fn parse_status_line(line: &str) -> Option<u16> {
    let rest = line.strip_prefix("HTTP/")?;
    let (_, status_and_reason) = rest.split_once(' ')?;
    let status = status_and_reason
        .split_whitespace()
        .next()?
        .parse::<u16>()
        .ok()?;
    Some(status)
}

/// Default-off process execution policy carried by deterministic VM contexts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ProcessCapability {
    /// Process and shell APIs return PHP-visible failure values and warnings.
    #[default]
    Disabled,
    /// Test-only mock result for shell-like process APIs. No host process is
    /// launched; callers receive this deterministic output and status.
    Mock {
        /// Bytes exposed as process output.
        output: String,
        /// Exit status exposed through by-reference result-code arguments.
        exit_status: i64,
    },
}

/// Callback signature for loading web-session data by session id.
type SessionLoadFn = dyn Fn(&str) -> Result<PhpArray, String> + Send + Sync + 'static;
type SessionIdGenerateFn = dyn Fn() -> Result<String, String> + Send + Sync + 'static;

/// Optional transport callback for loading web-session data on `session_start()`.
#[derive(Clone)]
pub struct SessionLoadCallback(Arc<SessionLoadFn>);

impl SessionLoadCallback {
    #[must_use]
    pub fn new(
        callback: impl Fn(&str) -> Result<PhpArray, String> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(callback))
    }

    pub fn load(&self, id: &str) -> Result<PhpArray, String> {
        (self.0)(id)
    }
}

impl std::fmt::Debug for SessionLoadCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SessionLoadCallback")
    }
}

impl PartialEq for SessionLoadCallback {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SessionLoadCallback {}

/// Optional transport callback for generating a session id only when PHP
/// activates a new session.
#[derive(Clone)]
pub struct SessionIdGenerateCallback(Arc<SessionIdGenerateFn>);

impl SessionIdGenerateCallback {
    #[must_use]
    pub fn new(callback: impl Fn() -> Result<String, String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(callback))
    }

    pub fn generate(&self) -> Result<String, String> {
        (self.0)()
    }
}

impl std::fmt::Debug for SessionIdGenerateCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SessionIdGenerateCallback")
    }
}

impl PartialEq for SessionIdGenerateCallback {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SessionIdGenerateCallback {}

/// Owned deterministic runtime context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeContext {
    /// Current working directory for future relative-path/runtime behavior.
    pub cwd: PathBuf,
    /// PHP CLI argv vector. Element 0 is the script path when configured.
    pub argv: Vec<String>,
    /// Controlled environment entries. Host env is never imported implicitly.
    pub env: Arc<Vec<(String, String)>>,
    /// Deterministic bytes exposed through CLI stdin resources.
    pub stdin: Arc<[u8]>,
    /// Minimal include path placeholder.
    pub include_path: Vec<PathBuf>,
    /// Minimal ini-like options.
    pub ini: RuntimeIniOptions,
    /// Generic `-d name=value` ini overrides applied to the per-request registry
    /// (e.g. `serialize_precision`), in addition to the typed options above.
    pub ini_overrides: Vec<(String, String)>,
    /// Strict-types metadata collected by future frontend integration.
    pub strict_types: Vec<StrictTypesInfo>,
    /// Host filesystem capability policy for stream and filesystem builtins.
    pub filesystem: FilesystemCapabilities,
    /// Host process/shell execution policy.
    pub process: ProcessCapability,
    /// Request mode for deterministic superglobal seeding.
    pub request_mode: RuntimeRequestMode,
    /// Request-local session state seed.
    pub session: SessionState,
    /// Optional transport session loader used only when PHP starts a session.
    pub session_loader: Option<SessionLoadCallback>,
    /// Optional transport session-id generator used only when PHP needs a new id.
    pub session_id_generator: Option<SessionIdGenerateCallback>,
    /// Optional cooperative PHP execution budget for the VM.
    pub execution_time_limit: Option<Duration>,
    /// Runtime SAPI name visible through PHP_SAPI and php_sapi_name().
    pub sapi_name: String,
    /// Runtime binary path visible through PHP_BINARY.
    pub php_binary: String,
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self {
            cwd: PathBuf::from("."),
            argv: Vec::new(),
            env: Arc::new(Vec::new()),
            stdin: Arc::from([]),
            include_path: vec![PathBuf::from(".")],
            ini: RuntimeIniOptions::default(),
            ini_overrides: Vec::new(),
            strict_types: Vec::new(),
            filesystem: FilesystemCapabilities::none(),
            process: ProcessCapability::Disabled,
            request_mode: RuntimeRequestMode::Cli,
            session: SessionState::default(),
            session_loader: None,
            session_id_generator: None,
            execution_time_limit: None,
            sapi_name: "cli".to_string(),
            php_binary: "phrust-php".to_string(),
        }
    }
}

impl RuntimeContext {
    /// Creates a context for deterministic CLI fixture execution.
    #[must_use]
    pub fn controlled_cli(script_path: impl Into<String>, script_args: Vec<String>) -> Self {
        let mut argv = vec![script_path.into()];
        argv.extend(script_args);
        Self {
            argv,
            request_mode: RuntimeRequestMode::Cli,
            ..Self::default()
        }
    }

    /// Creates a context for deterministic HTTP request execution.
    #[must_use]
    pub fn controlled_http(request: RuntimeHttpRequestContext) -> Self {
        Self {
            request_mode: RuntimeRequestMode::Http(Box::new(request)),
            sapi_name: "cli-server".to_string(),
            ..Self::default()
        }
    }

    /// Sets a deterministic current working directory.
    #[must_use]
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = cwd.into();
        self
    }

    /// Sets a deterministic include path.
    #[must_use]
    pub fn with_include_path(mut self, include_path: Vec<PathBuf>) -> Self {
        self.include_path = include_path;
        self
    }

    /// Sets generic ini overrides (e.g. from CLI `-d name=value`).
    #[must_use]
    pub fn with_ini_overrides(mut self, overrides: Vec<(String, String)>) -> Self {
        self.ini_overrides = overrides;
        self
    }

    /// Builds the per-request INI registry from deterministic context fields.
    #[must_use]
    pub fn ini_registry(&self) -> IniRegistry {
        let mut registry = IniRegistry::default();
        let include_path = self
            .include_path
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join(":");
        let _ = registry.set("include_path", include_path);
        let _ = registry.set("arg_separator.input", self.ini.arg_separator_input.clone());
        let _ = registry.set("error_reporting", self.ini.error_reporting.mask.to_string());
        let _ = registry.set(
            "display_errors",
            if self.ini.display_errors { "1" } else { "0" },
        );
        let _ = registry.set(
            "filter.default",
            match self.ini.default_input_filter {
                RuntimeInputFilter::UnsafeRaw => "unsafe_raw",
                RuntimeInputFilter::Stripped => "stripped",
                RuntimeInputFilter::SpecialChars => "special_chars",
            },
        );
        let _ = registry.set(
            "filter.default_flags",
            self.ini.default_input_filter_flags.to_string(),
        );
        let _ = registry.set("max_input_vars", self.ini.max_input_vars.to_string());
        let _ = registry.set(
            "max_input_nesting_level",
            self.ini.max_input_nesting_level.to_string(),
        );
        for (name, value) in &self.ini_overrides {
            let _ = registry.set_startup(name, unquote_ini_override_value(value));
        }
        registry
    }

    /// Sets controlled environment entries in stable key order.
    #[must_use]
    pub fn with_env(mut self, mut env: Vec<(String, String)>) -> Self {
        env.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        self.env = Arc::new(env);
        self
    }

    /// Sets already sorted controlled environment entries.
    #[must_use]
    pub fn with_sorted_env_arc(mut self, env: Arc<Vec<(String, String)>>) -> Self {
        self.env = env;
        self
    }

    /// Sets deterministic stdin bytes for CLI-style execution.
    #[must_use]
    pub fn with_stdin(mut self, stdin: impl Into<Arc<[u8]>>) -> Self {
        self.stdin = stdin.into();
        self
    }

    /// Sets host filesystem capabilities for streams and filesystem builtins.
    #[must_use]
    pub fn with_filesystem_capabilities(mut self, filesystem: FilesystemCapabilities) -> Self {
        self.filesystem = filesystem;
        self
    }

    /// Enables a deterministic process mock for isolated tests.
    #[must_use]
    pub fn with_process_mock(mut self, output: impl Into<String>, exit_status: i64) -> Self {
        self.process = ProcessCapability::Mock {
            output: output.into(),
            exit_status,
        };
        self
    }

    /// Seeds request-local session state for web execution.
    #[must_use]
    pub fn with_session_state(mut self, session: SessionState) -> Self {
        self.session = session;
        self
    }

    /// Sets a transport callback for loading session data when PHP activates it.
    #[must_use]
    pub fn with_session_loader(mut self, loader: SessionLoadCallback) -> Self {
        self.session_loader = Some(loader);
        self
    }

    /// Sets a transport callback for generating a new id at session activation.
    #[must_use]
    pub fn with_session_id_generator(mut self, generator: SessionIdGenerateCallback) -> Self {
        self.session_id_generator = Some(generator);
        self
    }

    /// Sets an optional cooperative PHP execution budget for this request.
    #[must_use]
    pub fn with_execution_time_limit(mut self, limit: Option<Duration>) -> Self {
        self.execution_time_limit = limit;
        self
    }

    /// Sets the runtime SAPI name.
    #[must_use]
    pub fn with_sapi_name(mut self, sapi_name: impl Into<String>) -> Self {
        self.sapi_name = sapi_name.into();
        self
    }

    /// Sets the runtime PHP binary path.
    #[must_use]
    pub fn with_php_binary(mut self, php_binary: impl Into<String>) -> Self {
        self.php_binary = php_binary.into();
        self
    }

    /// Sets deterministic HTTP request metadata.
    #[must_use]
    pub fn with_http_request(mut self, request: RuntimeHttpRequestContext) -> Self {
        self.request_mode = RuntimeRequestMode::Http(Box::new(request));
        self
    }

    #[must_use]
    pub fn upload_registry(&self) -> UploadRegistry {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => {
                UploadRegistry::from_uploaded_files(&request.uploaded_files)
            }
            RuntimeRequestMode::Cli => UploadRegistry::default(),
        }
    }

    /// Returns the `$argc` value derived from configured argv.
    #[must_use]
    pub fn argc(&self) -> i64 {
        self.argv.len() as i64
    }

    /// Returns a controlled global/superglobal value by local name.
    #[must_use]
    pub fn global_value(&self, name: &str) -> Option<Value> {
        match name {
            "argc" => Some(Value::Int(self.argc())),
            "argv" => Some(self.argv_array()),
            "_SERVER" => Some(Value::Array(self.server_array())),
            "_ENV" => Some(Value::Array(self.env_array())),
            "_GET" => Some(Value::Array(self.get_array())),
            "_POST" => Some(Value::Array(self.post_array())),
            "_COOKIE" => Some(Value::Array(self.cookie_array())),
            "_REQUEST" => Some(Value::Array(self.request_array())),
            "_FILES" => Some(Value::Array(self.files_array())),
            "_SESSION" => {
                if self.session.status() == crate::PHP_SESSION_ACTIVE
                    || self.session.started()
                    || !self.session.id().is_empty()
                {
                    Some(self.session.data_value())
                } else {
                    None
                }
            }
            "GLOBALS" => Some(Value::Array(PhpArray::new())),
            _ => None,
        }
    }

    /// Returns the request input array used by `filter_input`.
    #[must_use]
    pub fn filter_input_array(&self, source: i64) -> Option<PhpArray> {
        match source {
            0 => Some(self.filter_post_array()),
            1 => Some(self.filter_get_array()),
            2 => Some(self.filter_cookie_array()),
            4 => Some(self.env_array()),
            5 => Some(self.filter_server_array()),
            _ => None,
        }
    }

    fn argv_array(&self) -> Value {
        Value::packed_array(
            self.argv
                .iter()
                .map(|value| Value::string(value.as_bytes().to_vec()))
                .collect(),
        )
    }

    fn server_array(&self) -> PhpArray {
        if let RuntimeRequestMode::Http(request) = &self.request_mode {
            return http_server_array(request);
        }
        let mut array = PhpArray::new();
        for (key, value) in self.env.iter() {
            insert_string(&mut array, key, value);
        }
        array.insert(string_key("argc"), Value::Int(self.argc()));
        array.insert(string_key("argv"), self.argv_array());
        let script = self.argv.first().cloned().unwrap_or_default();
        array.insert(string_key("PHP_SELF"), Value::string(script.clone()));
        array.insert(string_key("SCRIPT_FILENAME"), Value::string(script.clone()));
        array.insert(string_key("SCRIPT_NAME"), Value::string(script));
        array.insert(string_key("DOCUMENT_ROOT"), Value::string(""));
        array.insert(string_key("REQUEST_TIME"), Value::Int(0));
        array
    }

    fn filter_get_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => {
                raw_input_pairs_array(&request.parsed_get, &self.ini)
            }
            RuntimeRequestMode::Cli => self
                .env_value("QUERY_STRING")
                .map_or_else(PhpArray::new, |query| {
                    raw_input_pairs_array(&parse_query_string(query), &self.ini)
                }),
        }
    }

    fn filter_post_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => {
                raw_input_pairs_array(&request.parsed_post, &self.ini)
            }
            RuntimeRequestMode::Cli => {
                self.env_value("PHPT_REQUEST_BODY")
                    .map_or_else(PhpArray::new, |body| {
                        raw_input_pairs_array(
                            &parse_form_urlencoded_body(body.as_bytes()),
                            &self.ini,
                        )
                    })
            }
        }
    }

    fn filter_cookie_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => {
                raw_flat_pairs_array(&request.parsed_cookie, &self.ini)
            }
            RuntimeRequestMode::Cli => self
                .env_value("HTTP_COOKIE")
                .map_or_else(PhpArray::new, |cookie| {
                    raw_flat_pairs_array(&parse_cookie_header(cookie), &self.ini)
                }),
        }
    }

    fn filter_server_array(&self) -> PhpArray {
        let mut array = self.server_array();
        for name in [
            "REQUEST_METHOD",
            "QUERY_STRING",
            "HTTP_COOKIE",
            "CONTENT_TYPE",
            "CONTENT_LENGTH",
        ] {
            if let Some(value) = self.env_value(name) {
                insert_string(&mut array, name, value);
            }
        }
        array
    }

    fn env_value(&self, name: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn env_array(&self) -> PhpArray {
        let mut array = PhpArray::new();
        for (key, value) in self.env.iter() {
            array.insert(string_key(key), Value::string(value.as_bytes().to_vec()));
        }
        array
    }

    fn get_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => input_pairs_array(&request.parsed_get, &self.ini),
            RuntimeRequestMode::Cli => self
                .env_value("QUERY_STRING")
                .map_or_else(PhpArray::new, |query| {
                    input_pairs_array(&parse_query_string(query), &self.ini)
                }),
        }
    }

    fn post_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => input_pairs_array(&request.parsed_post, &self.ini),
            RuntimeRequestMode::Cli => self
                .env_value("PHPT_REQUEST_BODY")
                .map_or_else(PhpArray::new, |body| {
                    input_pairs_array(&parse_form_urlencoded_body(body.as_bytes()), &self.ini)
                }),
        }
    }

    fn cookie_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => {
                flat_pairs_array(&request.parsed_cookie, &self.ini)
            }
            RuntimeRequestMode::Cli => self
                .env_value("HTTP_COOKIE")
                .map_or_else(PhpArray::new, |cookie| {
                    flat_pairs_array(&parse_cookie_header(cookie), &self.ini)
                }),
        }
    }

    fn request_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => {
                let mut array = PhpArray::new();
                let mut builder = InputArrayBuilder::new(&self.ini);
                builder.insert_pairs(&mut array, &request.parsed_get);
                builder.insert_pairs(&mut array, &request.parsed_post);
                builder.insert_flat_pairs(&mut array, &request.parsed_cookie);
                array
            }
            RuntimeRequestMode::Cli => {
                let mut array = PhpArray::new();
                let mut builder = InputArrayBuilder::new(&self.ini);
                if let Some(query) = self.env_value("QUERY_STRING") {
                    builder.insert_pairs(&mut array, &parse_query_string(query));
                }
                if let Some(body) = self.env_value("PHPT_REQUEST_BODY") {
                    builder.insert_pairs(&mut array, &parse_form_urlencoded_body(body.as_bytes()));
                }
                if let Some(cookie) = self.env_value("HTTP_COOKIE") {
                    builder.insert_flat_pairs(&mut array, &parse_cookie_header(cookie));
                }
                array
            }
        }
    }

    fn files_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => {
                uploaded_files_array(&request.uploaded_files, &self.ini)
            }
            RuntimeRequestMode::Cli => PhpArray::new(),
        }
    }
}

fn unquote_ini_override_value(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_test_str(value))
}

fn input_key(value: &str) -> ArrayKey {
    ArrayKey::from_php_string(PhpString::from_test_str(value))
}

fn input_root_key(value: &str) -> ArrayKey {
    let normalized = value
        .chars()
        .map(|ch| match ch {
            ' ' | '.' | '[' => '_',
            _ => ch,
        })
        .collect::<String>();
    input_key(&normalized)
}

fn http_server_array(request: &RuntimeHttpRequestContext) -> PhpArray {
    let mut array = PhpArray::new();
    insert_string(&mut array, "REQUEST_METHOD", &request.method);
    insert_string(&mut array, "REQUEST_SCHEME", &request.scheme);
    insert_string(&mut array, "HTTP_HOST", &request.host);
    insert_string(&mut array, "SERVER_NAME", &request.server_name);
    insert_string(&mut array, "SERVER_ADDR", &request.server_addr);
    insert_string(&mut array, "SERVER_PORT", &request.server_port.to_string());
    insert_string(&mut array, "SERVER_PROTOCOL", &request.server_protocol);
    insert_string(&mut array, "SERVER_SOFTWARE", &request.server_software);
    insert_string(&mut array, "GATEWAY_INTERFACE", &request.gateway_interface);
    insert_string(
        &mut array,
        "HTTPS",
        if request.https { "on" } else { "off" },
    );
    insert_string(&mut array, "REQUEST_URI", &request.request_uri);
    insert_string(&mut array, "DOCUMENT_URI", &request.path);
    insert_string(&mut array, "SCRIPT_NAME", &request.script_name);
    insert_string(&mut array, "PHP_SELF", request.php_self());
    insert_string(&mut array, "SCRIPT_FILENAME", &request.script_filename);
    insert_string(&mut array, "DOCUMENT_ROOT", &request.document_root);
    insert_string(&mut array, "QUERY_STRING", &request.query_string);
    insert_string(&mut array, "REMOTE_ADDR", &request.remote_addr);
    if let Some(remote_port) = request.remote_port {
        insert_string(&mut array, "REMOTE_PORT", &remote_port.to_string());
    }
    if let Some(auth_type) = &request.auth_type {
        insert_string(&mut array, "AUTH_TYPE", auth_type);
    }
    if let Some(remote_user) = &request.remote_user {
        insert_string(&mut array, "REMOTE_USER", remote_user);
    }
    if let Some(user) = &request.php_auth_user {
        insert_string(&mut array, "PHP_AUTH_USER", user);
    }
    if let Some(password) = &request.php_auth_pw {
        insert_string(&mut array, "PHP_AUTH_PW", password);
    }
    array.insert(string_key("REQUEST_TIME"), Value::Int(request.request_time));
    array.insert(
        string_key("REQUEST_TIME_FLOAT"),
        Value::float(request.request_time_float_micros as f64 / 1_000_000.0),
    );
    if let Some(path_info) = &request.path_info {
        insert_string(&mut array, "PATH_INFO", path_info);
    }
    if let Some(content_type) = &request.content_type {
        insert_string(&mut array, "CONTENT_TYPE", content_type);
    }
    if let Some(content_length) = request.content_length {
        insert_string(&mut array, "CONTENT_LENGTH", &content_length.to_string());
    }
    for (name, value) in &request.headers {
        if let Some(server_name) = header_server_name(name) {
            insert_string(&mut array, &server_name, value);
        }
    }
    array
}

fn server_name_from_host(host: &str) -> String {
    if let Some(rest) = host.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        return rest[..end].to_string();
    }
    host.rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|byte| byte.is_ascii_digit()))
        .map_or_else(|| host.to_string(), |(name, _)| name.to_string())
}

fn header_server_name(name: &str) -> Option<String> {
    if name.eq_ignore_ascii_case("host") {
        return None;
    }
    if name.eq_ignore_ascii_case("content-type") {
        return Some("CONTENT_TYPE".to_string());
    }
    if name.eq_ignore_ascii_case("content-length") {
        return Some("CONTENT_LENGTH".to_string());
    }
    let mut normalized = String::from("HTTP_");
    for byte in name.bytes() {
        match byte {
            b'a'..=b'z' => normalized.push(char::from(byte.to_ascii_uppercase())),
            b'A'..=b'Z' | b'0'..=b'9' => normalized.push(char::from(byte)),
            b'-' => normalized.push('_'),
            _ => return None,
        }
    }
    Some(normalized)
}

#[must_use]
pub fn input_pairs_array(pairs: &[(String, String)], ini: &RuntimeIniOptions) -> PhpArray {
    let mut array = PhpArray::new();
    InputArrayBuilder::new(ini).insert_pairs(&mut array, pairs);
    array
}

fn raw_input_pairs_array(pairs: &[(String, String)], ini: &RuntimeIniOptions) -> PhpArray {
    let mut array = PhpArray::new();
    InputArrayBuilder::raw(ini).insert_pairs(&mut array, pairs);
    array
}

fn flat_pairs_array(pairs: &[(String, String)], ini: &RuntimeIniOptions) -> PhpArray {
    let mut array = PhpArray::new();
    InputArrayBuilder::new(ini).insert_flat_pairs(&mut array, pairs);
    array
}

fn raw_flat_pairs_array(pairs: &[(String, String)], ini: &RuntimeIniOptions) -> PhpArray {
    let mut array = PhpArray::new();
    InputArrayBuilder::raw(ini).insert_flat_pairs(&mut array, pairs);
    array
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InputKeySegment {
    Key(ArrayKey),
    Append,
}

struct InputArrayBuilder {
    remaining_vars: usize,
    max_input_nesting_level: usize,
    default_filter: RuntimeInputFilter,
    default_filter_flags: i64,
}

impl InputArrayBuilder {
    fn new(ini: &RuntimeIniOptions) -> Self {
        Self {
            remaining_vars: ini.max_input_vars,
            max_input_nesting_level: ini.max_input_nesting_level,
            default_filter: ini.default_input_filter,
            default_filter_flags: ini.default_input_filter_flags,
        }
    }

    fn raw(ini: &RuntimeIniOptions) -> Self {
        Self {
            default_filter: RuntimeInputFilter::UnsafeRaw,
            default_filter_flags: 0,
            ..Self::new(ini)
        }
    }

    fn insert_pairs(&mut self, array: &mut PhpArray, pairs: &[(String, String)]) {
        for (key, value) in pairs {
            if !self.consume_var() {
                break;
            }
            let Some(segments) = parse_input_key_segments(key, self.max_input_nesting_level) else {
                continue;
            };
            insert_input_value(array, &segments, self.filter_value(value));
        }
    }

    fn insert_flat_pairs(&mut self, array: &mut PhpArray, pairs: &[(String, String)]) {
        let mut seen = HashSet::new();
        for (key, value) in pairs {
            if !self.consume_var() {
                break;
            }
            if seen.insert(key) {
                array.insert(string_key(key), self.filter_value(value));
            }
        }
    }

    fn consume_var(&mut self) -> bool {
        if self.remaining_vars == 0 {
            return false;
        }
        self.remaining_vars -= 1;
        true
    }

    fn filter_value(&self, value: &str) -> Value {
        match self.default_filter {
            RuntimeInputFilter::UnsafeRaw => Value::string(filter_unsafe_raw_input(
                value,
                self.default_input_filter_flags(),
            )),
            RuntimeInputFilter::Stripped => Value::string(encode_input_stripped(value)),
            RuntimeInputFilter::SpecialChars => Value::string(encode_input_special_chars(value)),
        }
    }

    fn default_input_filter_flags(&self) -> i64 {
        self.default_filter_flags
    }
}

const FILTER_FLAG_STRIP_LOW: i64 = 4;
const FILTER_FLAG_STRIP_HIGH: i64 = 8;
const FILTER_FLAG_ENCODE_LOW: i64 = 16;
const FILTER_FLAG_ENCODE_HIGH: i64 = 32;
const FILTER_FLAG_ENCODE_AMP: i64 = 64;
const FILTER_FLAG_STRIP_BACKTICK: i64 = 512;

fn filter_unsafe_raw_input(value: &str, flags: i64) -> Vec<u8> {
    let relevant_flags = FILTER_FLAG_STRIP_LOW
        | FILTER_FLAG_STRIP_HIGH
        | FILTER_FLAG_ENCODE_LOW
        | FILTER_FLAG_ENCODE_HIGH
        | FILTER_FLAG_ENCODE_AMP
        | FILTER_FLAG_STRIP_BACKTICK;
    if flags & relevant_flags == 0 {
        return value.as_bytes().to_vec();
    }
    let strip_low = flags & FILTER_FLAG_STRIP_LOW != 0;
    let strip_high = flags & FILTER_FLAG_STRIP_HIGH != 0;
    let strip_backtick = flags & FILTER_FLAG_STRIP_BACKTICK != 0;
    let mut output = Vec::with_capacity(value.len());
    for byte in value.bytes() {
        if strip_low && byte < 0x20 || strip_high && byte >= 0x7f || strip_backtick && byte == b'`'
        {
            continue;
        }
        if flags & FILTER_FLAG_ENCODE_AMP != 0 && byte == b'&'
            || flags & FILTER_FLAG_ENCODE_LOW != 0 && byte < 0x20
            || flags & FILTER_FLAG_ENCODE_HIGH != 0 && byte >= 0x7f
        {
            output.extend_from_slice(format!("&#{};", byte).as_bytes());
        } else {
            output.push(byte);
        }
    }
    output
}

fn insert_string(array: &mut PhpArray, key: &str, value: &str) {
    array.insert(string_key(key), Value::string(value.as_bytes().to_vec()));
}

fn parse_input_key_segments(key: &str, max_nesting_level: usize) -> Option<Vec<InputKeySegment>> {
    if key.is_empty() {
        return None;
    }
    let Some(first_bracket) = key.find('[') else {
        return Some(vec![InputKeySegment::Key(input_root_key(key))]);
    };
    if first_bracket == 0 {
        return None;
    }

    let mut segments = vec![InputKeySegment::Key(input_root_key(&key[..first_bracket]))];
    let mut rest = &key[first_bracket..];
    while !rest.is_empty() {
        if !rest.starts_with('[') {
            return Some(segments);
        }
        let Some(close) = rest.find(']') else {
            if segments.len() == 1 {
                return Some(vec![InputKeySegment::Key(input_root_key(key))]);
            }
            return Some(segments);
        };
        let part = &rest[1..close];
        segments.push(if part.is_empty() {
            InputKeySegment::Append
        } else {
            InputKeySegment::Key(input_key(part))
        });
        if segments.len().saturating_sub(1) > max_nesting_level {
            return None;
        }
        rest = &rest[close + 1..];
    }
    Some(segments)
}

fn insert_input_value(array: &mut PhpArray, segments: &[InputKeySegment], value: Value) {
    insert_input_at(array, segments, value);
}

fn strip_input_tags(value: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(value.len());
    let mut in_tag = false;
    for byte in value.bytes() {
        match byte {
            b'<' => in_tag = true,
            b'>' if in_tag => in_tag = false,
            _ if !in_tag => output.push(byte),
            _ => {}
        }
    }
    output
}

fn encode_input_stripped(value: &str) -> Vec<u8> {
    let stripped = strip_input_tags(value);
    let mut output = Vec::with_capacity(stripped.len());
    for byte in stripped {
        match byte {
            b'"' | b'\'' => {
                output.extend_from_slice(format!("&#{};", byte).as_bytes());
            }
            _ => output.push(byte),
        }
    }
    output
}

fn encode_input_special_chars(value: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'"' | b'\'' | b'<' | b'>' | b'&' => {
                output.extend_from_slice(format!("&#{};", byte).as_bytes());
            }
            _ => output.push(byte),
        }
    }
    output
}

fn insert_input_at(array: &mut PhpArray, segments: &[InputKeySegment], value: Value) {
    let Some((head, tail)) = segments.split_first() else {
        return;
    };
    if tail.is_empty() {
        match head {
            InputKeySegment::Key(key) => {
                array.insert(key.clone(), value);
            }
            InputKeySegment::Append => {
                array.append(value);
            }
        }
        return;
    }

    match head {
        InputKeySegment::Key(key) => {
            if !matches!(array.get(key), Some(Value::Array(_))) {
                array.insert(key.clone(), Value::Array(PhpArray::new()));
            }
            let Some(mut child_slot) = array.get_mut(key) else {
                unreachable!("input child was just initialized as an array")
            };
            let Value::Array(child) = &mut *child_slot else {
                unreachable!("input child was just initialized as an array")
            };
            insert_input_at(child, tail, value);
        }
        InputKeySegment::Append => {
            let key = array.append(Value::Array(PhpArray::new()));
            let Some(mut child_slot) = array.get_mut(&key) else {
                unreachable!("input append child was just initialized as an array")
            };
            let Value::Array(child) = &mut *child_slot else {
                unreachable!("input append child was just initialized as an array")
            };
            insert_input_at(child, tail, value);
        }
    }
}

fn uploaded_files_array(files: &[RuntimeUploadedFile], ini: &RuntimeIniOptions) -> PhpArray {
    let mut array = PhpArray::new();
    let mut builder = InputArrayBuilder::new(ini);
    for file in files {
        if !builder.consume_var() {
            break;
        }
        let Some(segments) =
            parse_input_key_segments(&file.field_name, ini.max_input_nesting_level)
        else {
            continue;
        };
        insert_uploaded_file(&mut array, &segments, file);
    }
    array
}

fn insert_uploaded_file(
    array: &mut PhpArray,
    segments: &[InputKeySegment],
    file: &RuntimeUploadedFile,
) {
    let Some((root, tail)) = segments.split_first() else {
        return;
    };
    let InputKeySegment::Key(root_key) = root else {
        return;
    };
    if tail.is_empty() {
        array.insert(root_key.clone(), Value::Array(uploaded_file_entry(file)));
        return;
    }

    if !matches!(array.get(root_key), Some(Value::Array(_))) {
        array.insert(root_key.clone(), Value::Array(uploaded_file_group()));
    }
    let Some(mut group_slot) = array.get_mut(root_key) else {
        unreachable!("uploaded file root was just initialized as an array")
    };
    let Value::Array(group) = &mut *group_slot else {
        unreachable!("uploaded file root was just initialized as an array")
    };
    insert_uploaded_file_attribute(
        group,
        "name",
        tail,
        Value::string(file.client_filename.as_bytes().to_vec()),
    );
    insert_uploaded_file_attribute(
        group,
        "type",
        tail,
        Value::string(file.content_type.as_bytes().to_vec()),
    );
    insert_uploaded_file_attribute(
        group,
        "tmp_name",
        tail,
        Value::string(file.temp_path.as_bytes().to_vec()),
    );
    insert_uploaded_file_attribute(group, "error", tail, Value::Int(file.error));
    insert_uploaded_file_attribute(group, "size", tail, Value::Int(file.size as i64));
}

fn uploaded_file_group() -> PhpArray {
    let mut array = PhpArray::new();
    array.insert(string_key("name"), Value::Array(PhpArray::new()));
    array.insert(string_key("type"), Value::Array(PhpArray::new()));
    array.insert(string_key("tmp_name"), Value::Array(PhpArray::new()));
    array.insert(string_key("error"), Value::Array(PhpArray::new()));
    array.insert(string_key("size"), Value::Array(PhpArray::new()));
    array
}

fn uploaded_file_entry(file: &RuntimeUploadedFile) -> PhpArray {
    let mut array = PhpArray::new();
    array.insert(
        string_key("name"),
        Value::string(file.client_filename.as_bytes().to_vec()),
    );
    array.insert(
        string_key("type"),
        Value::string(file.content_type.as_bytes().to_vec()),
    );
    array.insert(
        string_key("tmp_name"),
        Value::string(file.temp_path.as_bytes().to_vec()),
    );
    array.insert(string_key("error"), Value::Int(file.error));
    array.insert(string_key("size"), Value::Int(file.size as i64));
    array
}

fn insert_uploaded_file_attribute(
    group: &mut PhpArray,
    attribute: &str,
    tail: &[InputKeySegment],
    value: Value,
) {
    let key = string_key(attribute);
    if !matches!(group.get(&key), Some(Value::Array(_))) {
        group.insert(key.clone(), Value::Array(PhpArray::new()));
    }
    let Some(mut values_slot) = group.get_mut(&key) else {
        unreachable!("uploaded file attribute was just initialized as an array")
    };
    let Value::Array(values) = &mut *values_slot else {
        unreachable!("uploaded file attribute was just initialized as an array")
    };
    insert_input_at(values, tail, value);
}

#[must_use]
pub fn parse_query_string(query: &str) -> Vec<(String, String)> {
    parse_query_string_with_separators(query, "&")
}

#[must_use]
pub fn parse_query_string_with_separators(
    query: &str,
    arg_separator_input: &str,
) -> Vec<(String, String)> {
    parse_form_urlencoded(query.as_bytes(), input_separator_bytes(arg_separator_input))
}

#[must_use]
pub fn parse_form_urlencoded_body(body: &[u8]) -> Vec<(String, String)> {
    parse_form_urlencoded(body, b"&")
}

fn input_separator_bytes(arg_separator_input: &str) -> &[u8] {
    let separators = arg_separator_input.as_bytes();
    if separators.is_empty() {
        b"&"
    } else {
        separators
    }
}

fn parse_form_urlencoded(input: &[u8], separators: &[u8]) -> Vec<(String, String)> {
    input
        .split(|byte| separators.contains(byte))
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (name, value) = split_bytes_once(part, b'=').unwrap_or((part, &[]));
            Some((decode_component(name)?, decode_component(value)?))
        })
        .collect()
}

fn split_bytes_once(input: &[u8], delimiter: u8) -> Option<(&[u8], &[u8])> {
    let index = php_source::byte_kernel::find_byte(input, delimiter)?;
    Some((&input[..index], &input[index + 1..]))
}

#[must_use]
pub fn parse_cookie_header(cookie: &str) -> Vec<(String, String)> {
    cookie
        .split(';')
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                return None;
            }
            let (name, value) = trimmed.split_once('=')?;
            Some((name.trim().to_string(), decode_cookie_value(value.trim())))
        })
        .collect()
}

fn decode_cookie_value(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let (Some(high), Some(low)) = (bytes.get(index + 1), bytes.get(index + 2))
            && let (Some(high), Some(low)) = (hex_value(*high), hex_value(*low))
        {
            output.push(high << 4 | low);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(output).unwrap_or_else(|_| input.to_string())
}

fn decode_component(input: &[u8]) -> Option<String> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        match input[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' => {
                if let (Some(high), Some(low)) = (input.get(index + 1), input.get(index + 2))
                    && let (Some(high), Some(low)) = (hex_value(*high), hex_value(*low))
                {
                    output.push(high << 4 | low);
                    index += 3;
                } else {
                    output.push(input[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeContext, RuntimeHttpRequestContext, RuntimeHttpResponseState, RuntimeIniOptions,
        RuntimeInputFilter, RuntimeUploadedFile, StrictTypesInfo, UploadRegistry,
        input_pairs_array, parse_cookie_header, parse_form_urlencoded_body, parse_query_string,
        parse_query_string_with_separators,
    };
    use crate::{ArrayKey, PhpString, Value};
    use std::sync::Arc;

    #[test]
    fn context_defaults_are_deterministic() {
        let context = RuntimeContext::default();

        assert_eq!(context.cwd.to_string_lossy(), ".");
        assert!(context.argv.is_empty());
        assert!(context.env.is_empty());
        assert_eq!(context.include_path.len(), 1);
        assert_eq!(context.ini.error_reporting.mask, -1);
        assert!(context.ini.display_errors);
        assert_eq!(context.ini.max_input_vars, 1000);
        assert_eq!(context.ini.max_input_nesting_level, 64);
        assert_eq!(context.ini_registry().get("include_path"), Some("."));
        assert_eq!(context.ini_registry().get("max_input_vars"), Some("1000"));
        assert_eq!(
            context.ini_registry().get("upload_max_filesize"),
            Some("2M")
        );
        assert_eq!(context.ini_registry().get("post_max_size"), Some("8M"));
        assert_eq!(context.ini_registry().get("max_file_uploads"), Some("20"));
        assert_eq!(
            context.ini_registry().get("max_input_nesting_level"),
            Some("64")
        );
        assert_eq!(context.process, super::ProcessCapability::Disabled);
        assert!(context.strict_types.is_empty());
    }

    #[test]
    fn context_ini_overrides_parse_quoted_phpt_values() {
        let context = RuntimeContext::default().with_ini_overrides(vec![
            ("session.cookie_path".to_string(), "\"/\"".to_string()),
            ("session.cookie_domain".to_string(), "\"\"".to_string()),
            ("session.cookie_samesite".to_string(), "'Lax'".to_string()),
        ]);
        let registry = context.ini_registry();

        assert_eq!(registry.get("session.cookie_path"), Some("/"));
        assert_eq!(registry.get("session.cookie_domain"), Some(""));
        assert_eq!(registry.get("session.cookie_samesite"), Some("Lax"));
    }

    #[test]
    fn context_cli_argv_and_server_are_controlled() {
        let context = RuntimeContext::controlled_cli(
            "fixtures/runtime/valid/superglobals/argv.php",
            vec!["alpha".to_string(), "beta".to_string()],
        );

        assert_eq!(context.argc(), 3);
        assert_eq!(context.global_value("argc"), Some(Value::Int(3)));
        let Some(Value::Array(server)) = context.global_value("_SERVER") else {
            panic!("expected server array");
        };
        assert_eq!(
            server.get(&ArrayKey::String(PhpString::from_test_str("argc"))),
            Some(&Value::Int(3))
        );
        assert!(matches!(
            server.get(&ArrayKey::String(PhpString::from_test_str("argv"))),
            Some(Value::Array(_))
        ));
        assert_eq!(
            server.get(&ArrayKey::String(PhpString::from_test_str("SCRIPT_NAME"))),
            Some(&Value::string(
                "fixtures/runtime/valid/superglobals/argv.php"
            ))
        );
        assert_eq!(
            server.get(&ArrayKey::String(PhpString::from_test_str("REQUEST_TIME"))),
            Some(&Value::Int(0))
        );
    }

    #[test]
    fn context_env_is_sorted_and_host_independent() {
        let context = RuntimeContext::default().with_env(vec![
            ("ZED".to_string(), "last".to_string()),
            ("ALPHA".to_string(), "first".to_string()),
        ]);

        assert_eq!(context.env[0].0, "ALPHA");
        assert_eq!(context.env[1].0, "ZED");
        assert!(context.global_value("_ENV").is_some());
        assert!(RuntimeContext::default().env.is_empty());
    }

    #[test]
    fn context_accepts_shared_sorted_environment() {
        let env = Arc::new(vec![("ALPHA".to_string(), "first".to_string())]);
        let context = RuntimeContext::default().with_sorted_env_arc(Arc::clone(&env));

        assert!(Arc::ptr_eq(&context.env, &env));
        assert_eq!(context.env[0].0, "ALPHA");
    }

    #[test]
    fn context_strict_types_placeholder_is_explicit_metadata() {
        let context = RuntimeContext {
            strict_types: vec![StrictTypesInfo {
                subject: "fixture.php".to_string(),
                enabled: true,
            }],
            ..RuntimeContext::default()
        };

        assert_eq!(context.strict_types[0].subject, "fixture.php");
        assert!(context.strict_types[0].enabled);
    }

    #[test]
    fn http_response_state_defaults_to_ok() {
        let state = RuntimeHttpResponseState::default();

        assert_eq!(state.status_code, 200);
        assert!(state.headers.is_empty());
        assert!(!state.headers_sent);
    }

    #[test]
    fn http_response_state_replaces_headers_by_default() {
        let mut state = RuntimeHttpResponseState::default();

        state.add_header_line("X-Test: one", true, None).unwrap();
        state.add_header_line("x-test: two", true, None).unwrap();

        assert_eq!(state.headers_list(), vec!["x-test: two"]);
    }

    #[test]
    fn http_response_state_preserves_duplicate_headers_without_replace() {
        let mut state = RuntimeHttpResponseState::default();

        state
            .add_header_line("Set-Cookie: a=1", false, None)
            .unwrap();
        state
            .add_header_line("Set-Cookie: b=2", false, None)
            .unwrap();

        assert_eq!(
            state.headers_list(),
            vec!["Set-Cookie: a=1", "Set-Cookie: b=2"]
        );
    }

    #[test]
    fn cookie_header_parser_raw_decodes_incoming_cookie_values() {
        assert_eq!(
            parse_cookie_header(
                "sid=abc; theme=dark%20mode; auth=user%7Ctoken; plus=a+b%2Bc; bad=%xx"
            ),
            vec![
                ("sid".to_string(), "abc".to_string()),
                ("theme".to_string(), "dark mode".to_string()),
                ("auth".to_string(), "user|token".to_string()),
                ("plus".to_string(), "a+b+c".to_string()),
                ("bad".to_string(), "%xx".to_string()),
            ]
        );

        let state = RuntimeHttpResponseState::default();
        assert!(state.headers.is_empty());
    }

    #[test]
    fn http_response_state_accepts_status_lines_and_response_code() {
        let mut state = RuntimeHttpResponseState::default();

        state
            .add_header_line("HTTP/1.1 404 Not Found", true, None)
            .unwrap();
        assert_eq!(state.status_code, 404);

        state
            .add_header_line("X-Status: yes", true, Some(201))
            .unwrap();
        assert_eq!(state.status_code, 201);
    }

    #[test]
    fn http_response_state_rejects_splitting_and_bad_names() {
        let mut state = RuntimeHttpResponseState::default();

        assert!(
            state
                .add_header_line("X-Test: ok\r\nX-Evil: yes", true, None)
                .is_err()
        );
        assert!(state.add_header_line("Bad Name: ok", true, None).is_err());
        assert!(state.headers.is_empty());
    }

    #[test]
    fn http_server_array_includes_required_keys() {
        let context = RuntimeContext::controlled_http(http_request());

        let server = global_array(&context, "_SERVER");
        assert_string(&server, "REQUEST_METHOD", "POST");
        assert_string(&server, "REQUEST_SCHEME", "http");
        assert_string(&server, "HTTP_HOST", "example.test");
        assert_string(&server, "SERVER_NAME", "example.test");
        assert_string(&server, "SERVER_ADDR", "127.0.0.1");
        assert_string(&server, "SERVER_PORT", "8080");
        assert_string(&server, "SERVER_PROTOCOL", "HTTP/1.1");
        assert_string(&server, "SERVER_SOFTWARE", "phrust-server");
        assert_string(&server, "GATEWAY_INTERFACE", "CGI/1.1");
        assert_string(&server, "HTTPS", "off");
        assert_string(&server, "REQUEST_URI", "/submit.php?name=phrust");
        assert_string(&server, "SCRIPT_NAME", "/submit.php");
        assert_string(&server, "PHP_SELF", "/submit.php/extra");
        assert_string(&server, "SCRIPT_FILENAME", "/srv/app/submit.php");
        assert_string(&server, "DOCUMENT_ROOT", "/srv/app");
        assert_string(&server, "PATH_INFO", "/extra");
        assert_string(&server, "QUERY_STRING", "name=phrust");
        assert_string(&server, "REMOTE_ADDR", "127.0.0.1");
        assert_string(&server, "REMOTE_PORT", "50123");
        assert_string(&server, "AUTH_TYPE", "Basic");
        assert_string(&server, "REMOTE_USER", "alice");
        assert_string(&server, "PHP_AUTH_USER", "alice");
        assert_string(&server, "PHP_AUTH_PW", "s3cret");
        assert_string(&server, "CONTENT_TYPE", "application/x-www-form-urlencoded");
        assert_string(&server, "CONTENT_LENGTH", "7");
        assert_string(&server, "HTTP_X_TEST_HEADER", "yes");
        assert_eq!(
            server.get(&ArrayKey::String(PhpString::from_test_str("REQUEST_TIME"))),
            Some(&Value::Int(123))
        );
        assert_float(&server, "REQUEST_TIME_FLOAT", 123.456789);
    }

    #[test]
    fn server_name_strips_host_port_without_changing_http_host() {
        let mut request = http_request();
        request.host = "example.test:8443".to_string();
        request.server_name = super::server_name_from_host(&request.host);
        request.scheme = "https".to_string();
        request.https = true;
        request.server_port = 8443;
        let context = RuntimeContext::controlled_http(request);

        let server = global_array(&context, "_SERVER");
        assert_string(&server, "HTTP_HOST", "example.test:8443");
        assert_string(&server, "SERVER_NAME", "example.test");
        assert_string(&server, "HTTPS", "on");
    }

    #[test]
    fn server_name_handles_bracketed_ipv6_hosts() {
        let mut request = http_request();
        request.host = "[::1]:8080".to_string();
        request.server_name = super::server_name_from_host(&request.host);
        let context = RuntimeContext::controlled_http(request);

        let server = global_array(&context, "_SERVER");
        assert_string(&server, "HTTP_HOST", "[::1]:8080");
        assert_string(&server, "SERVER_NAME", "::1");
    }

    #[test]
    fn http_query_string_populates_get() {
        let context = RuntimeContext::controlled_http(http_request());

        let get = global_array(&context, "_GET");
        assert_string(&get, "name", "phrust");
    }

    #[test]
    fn http_form_body_populates_post() {
        let context = RuntimeContext::controlled_http(http_request());

        let post = global_array(&context, "_POST");
        assert_string(&post, "posted", "yes");
    }

    #[test]
    fn http_cookie_header_populates_cookie() {
        let context = RuntimeContext::controlled_http(http_request());

        let cookie = global_array(&context, "_COOKIE");
        assert_string(&cookie, "sid", "abc");
        assert_string(&cookie, "theme", "dark");
    }

    #[test]
    fn duplicate_cookie_names_keep_first_cookie_value() {
        let mut request = http_request();
        request.parsed_cookie = parse_cookie_header("abc=dir; def=true; abc=root");
        let context = RuntimeContext::controlled_http(request);

        let cookie = global_array(&context, "_COOKIE");
        assert_string(&cookie, "abc", "dir");
        assert_string(&cookie, "def", "true");
    }

    #[test]
    fn http_request_merge_order_is_get_post_cookie() {
        let mut request = http_request();
        request.parsed_get = vec![("same".to_string(), "get".to_string())];
        request.parsed_post = vec![("same".to_string(), "post".to_string())];
        request.parsed_cookie = vec![("same".to_string(), "cookie".to_string())];
        let context = RuntimeContext::controlled_http(request);

        let request = global_array(&context, "_REQUEST");
        assert_string(&request, "same", "cookie");
    }

    #[test]
    fn http_nested_inputs_populate_get_post_and_request() {
        let mut request = http_request();
        request.parsed_get =
            parse_query_string("user[name]=Ada&ids[]=1&ids[]=2&user[address][city]=Berlin");
        request.parsed_post = parse_form_urlencoded_body(b"form[title]=Hello");
        let context = RuntimeContext::controlled_http(request);

        let get = global_array(&context, "_GET");
        assert_path_string(&get, &[str_key("user"), str_key("name")], "Ada");
        assert_path_string(&get, &[str_key("ids"), int_key(0)], "1");
        assert_path_string(&get, &[str_key("ids"), int_key(1)], "2");
        assert_path_string(
            &get,
            &[str_key("user"), str_key("address"), str_key("city")],
            "Berlin",
        );

        let post = global_array(&context, "_POST");
        assert_path_string(&post, &[str_key("form"), str_key("title")], "Hello");

        let request = global_array(&context, "_REQUEST");
        assert_path_string(&request, &[str_key("user"), str_key("name")], "Ada");
        assert_path_string(&request, &[str_key("ids"), int_key(0)], "1");
        assert_path_string(&request, &[str_key("form"), str_key("title")], "Hello");
    }

    #[test]
    fn cli_input_superglobals_remain_empty() {
        let context = RuntimeContext::controlled_cli("script.php", Vec::new());

        assert!(global_array(&context, "_GET").is_empty());
        assert!(global_array(&context, "_POST").is_empty());
        assert!(global_array(&context, "_COOKIE").is_empty());
        assert!(global_array(&context, "_REQUEST").is_empty());
        assert!(global_array(&context, "_FILES").is_empty());
    }

    #[test]
    fn cli_phpt_environment_populates_input_superglobals() {
        let context = RuntimeContext::controlled_cli("script.php", Vec::new()).with_env(vec![
            ("QUERY_STRING".to_string(), "a=1&b=&c=3".to_string()),
            ("REQUEST_METHOD".to_string(), "POST".to_string()),
            ("PHPT_REQUEST_BODY".to_string(), "d=4&e=5".to_string()),
            ("HTTP_COOKIE".to_string(), "sid=abc".to_string()),
        ]);

        let get = global_array(&context, "_GET");
        assert_string(&get, "a", "1");
        assert_string(&get, "b", "");
        assert_string(&get, "c", "3");

        let post = global_array(&context, "_POST");
        assert_string(&post, "d", "4");
        assert_string(&post, "e", "5");

        let cookie = global_array(&context, "_COOKIE");
        assert_string(&cookie, "sid", "abc");

        let env = context
            .filter_input_array(4)
            .expect("INPUT_ENV source should be available");
        assert_string(&env, "QUERY_STRING", "a=1&b=&c=3");
        assert_string(&env, "HTTP_COOKIE", "sid=abc");

        let request = global_array(&context, "_REQUEST");
        assert_string(&request, "a", "1");
        assert_string(&request, "b", "");
        assert_string(&request, "c", "3");
        assert_string(&request, "d", "4");
        assert_string(&request, "e", "5");
        assert_string(&request, "sid", "abc");
    }

    #[test]
    fn cli_filter_default_flags_apply_to_superglobals_not_raw_filter_input() {
        let mut context = RuntimeContext::controlled_cli("script.php", Vec::new()).with_env(vec![
            ("QUERY_STRING".to_string(), "a=1%00".to_string()),
            (
                "HTTP_X_FORWARDED_FOR".to_string(),
                "example.com".to_string(),
            ),
        ]);
        context.ini.default_input_filter = RuntimeInputFilter::UnsafeRaw;
        context.ini.default_input_filter_flags = 4;

        let get = global_array(&context, "_GET");
        assert_string(&get, "a", "1");

        let raw_get = context
            .filter_input_array(1)
            .expect("INPUT_GET source should be available");
        assert_string(&raw_get, "a", "1\0");

        let server = global_array(&context, "_SERVER");
        assert_string(&server, "HTTP_X_FORWARDED_FOR", "example.com");

        let raw_server = context
            .filter_input_array(5)
            .expect("INPUT_SERVER source should be available");
        assert_string(&raw_server, "HTTP_X_FORWARDED_FOR", "example.com");
    }

    #[test]
    fn cli_input_superglobals_apply_filter_default_special_chars() {
        let mut context = RuntimeContext::controlled_cli("script.php", Vec::new()).with_env(vec![
            (
                "QUERY_STRING".to_string(),
                "a=O%27Henry&c=%3Cb%3EBold%3C%2Fb%3E".to_string(),
            ),
            (
                "PHPT_REQUEST_BODY".to_string(),
                "d=%22quotes%22&e=%5Cslash".to_string(),
            ),
        ]);
        context.ini.default_input_filter = RuntimeInputFilter::SpecialChars;

        let get = global_array(&context, "_GET");
        assert_string(&get, "a", "O&#39;Henry");
        assert_string(&get, "c", "&#60;b&#62;Bold&#60;/b&#62;");

        let post = global_array(&context, "_POST");
        assert_string(&post, "d", "&#34;quotes&#34;");
        assert_string(&post, "e", "\\slash");

        let request = global_array(&context, "_REQUEST");
        assert_string(&request, "a", "O&#39;Henry");
        assert_string(&request, "c", "&#60;b&#62;Bold&#60;/b&#62;");
        assert_string(&request, "d", "&#34;quotes&#34;");
        assert_string(&request, "e", "\\slash");
    }

    #[test]
    fn filter_input_cookie_array_uses_raw_request_snapshot() {
        let mut context =
            RuntimeContext::controlled_cli("script.php", Vec::new()).with_env(vec![(
                "HTTP_COOKIE".to_string(),
                "xyz=\"foo bar\"".to_string(),
            )]);
        context.ini.default_input_filter = RuntimeInputFilter::Stripped;

        let cookie = global_array(&context, "_COOKIE");
        assert_string(&cookie, "xyz", "&#34;foo bar&#34;");

        let filter_cookie = context
            .filter_input_array(2)
            .expect("cookie filter input exists");
        assert_string(&filter_cookie, "xyz", "\"foo bar\"");
    }

    #[test]
    fn http_uploaded_file_populates_files_superglobal() {
        let mut request = http_request();
        request
            .uploaded_files
            .push(uploaded_file("avatar", "me.png", 7));
        let context = RuntimeContext::controlled_http(request);

        let files = global_array(&context, "_FILES");
        assert_path_string(&files, &[str_key("avatar"), str_key("name")], "me.png");
        assert_path_string(&files, &[str_key("avatar"), str_key("type")], "image/png");
        assert_path_string(
            &files,
            &[str_key("avatar"), str_key("tmp_name")],
            "/tmp/phrust-upload",
        );
        assert_path_int(&files, &[str_key("avatar"), str_key("error")], 0);
        assert_path_int(&files, &[str_key("avatar"), str_key("size")], 7);
    }

    #[test]
    fn http_uploaded_file_array_fields_are_transposed_like_php() {
        let mut request = http_request();
        request
            .uploaded_files
            .push(uploaded_file("files[]", "one.txt", 3));
        request
            .uploaded_files
            .push(uploaded_file("files[]", "two.txt", 4));
        let context = RuntimeContext::controlled_http(request);

        let files = global_array(&context, "_FILES");
        assert_path_string(
            &files,
            &[str_key("files"), str_key("name"), int_key(0)],
            "one.txt",
        );
        assert_path_string(
            &files,
            &[str_key("files"), str_key("name"), int_key(1)],
            "two.txt",
        );
        assert_path_int(&files, &[str_key("files"), str_key("size"), int_key(0)], 3);
        assert_path_int(&files, &[str_key("files"), str_key("size"), int_key(1)], 4);
    }

    #[test]
    fn upload_registry_tracks_moved_and_unmoved_temps() {
        let root =
            std::env::temp_dir().join(format!("phrust-upload-registry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        let first = root.join("first.tmp");
        let second = root.join("second.tmp");
        std::fs::write(&first, b"first").expect("write first upload");
        std::fs::write(&second, b"second").expect("write second upload");
        let files = vec![
            RuntimeUploadedFile {
                temp_path: first.to_string_lossy().to_string(),
                ..uploaded_file("first", "first.txt", 5)
            },
            RuntimeUploadedFile {
                temp_path: second.to_string_lossy().to_string(),
                ..uploaded_file("second", "second.txt", 6)
            },
        ];
        let mut registry = UploadRegistry::from_uploaded_files(&files);

        assert!(registry.is_active_upload(&first.to_string_lossy()));
        assert!(registry.is_active_upload(&second.to_string_lossy()));
        assert!(registry.mark_moved(&first.to_string_lossy()));
        assert!(!registry.is_active_upload(&first.to_string_lossy()));
        assert!(registry.is_active_upload(&second.to_string_lossy()));
        assert!(!registry.mark_moved(&first.to_string_lossy()));

        registry.cleanup_unmoved();
        assert!(first.exists());
        assert!(!second.exists());

        let _ = std::fs::remove_file(first);
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn input_array_builder_supports_php_style_key_forms() {
        let pairs = parse_query_string(
            "a=1&a=2&list[]=1&list[]=2&indexed[0]=x&indexed[1]=y&user[name]=Ada&user[address][city]=Berlin",
        );
        let array = input_pairs_array(&pairs, &RuntimeIniOptions::default());

        assert_string(&array, "a", "2");
        assert_path_string(&array, &[str_key("list"), int_key(0)], "1");
        assert_path_string(&array, &[str_key("list"), int_key(1)], "2");
        assert_path_string(&array, &[str_key("indexed"), int_key(0)], "x");
        assert_path_string(&array, &[str_key("indexed"), int_key(1)], "y");
        assert_path_string(&array, &[str_key("user"), str_key("name")], "Ada");
        assert_path_string(
            &array,
            &[str_key("user"), str_key("address"), str_key("city")],
            "Berlin",
        );
    }

    #[test]
    fn input_array_builder_matches_php_malformed_key_recovery() {
        let pairs = parse_query_string(
            "arr[1=sid&arr[4][2=fred&arr1]=ok&arr[4]2]=bill&arr.test[1]=dot&arr test[4][two]=space",
        );
        let array = input_pairs_array(&pairs, &RuntimeIniOptions::default());

        assert_string(&array, "arr_1", "sid");
        assert_string(&array, "arr1]", "ok");
        assert_path_string(&array, &[str_key("arr"), int_key(4)], "bill");
        assert_path_string(&array, &[str_key("arr_test"), int_key(1)], "dot");
        assert_path_string(
            &array,
            &[str_key("arr_test"), int_key(4), str_key("two")],
            "space",
        );

        let invalid_root = input_pairs_array(
            &parse_query_string("[a]=ignored"),
            &RuntimeIniOptions::default(),
        );
        assert!(invalid_root.is_empty());
    }

    #[test]
    fn input_array_builder_applies_explicit_limits() {
        let ini = RuntimeIniOptions {
            max_input_vars: 2,
            max_input_nesting_level: 1,
            ..RuntimeIniOptions::default()
        };
        let pairs = parse_query_string("a=1&b=2&c=3");
        let array = input_pairs_array(&pairs, &ini);

        assert_string(&array, "a", "1");
        assert_string(&array, "b", "2");
        assert!(array.get(&str_key("c")).is_none());

        let nested =
            input_pairs_array(&parse_query_string("ok[name]=Ada&too[deep][name]=no"), &ini);
        assert_path_string(&nested, &[str_key("ok"), str_key("name")], "Ada");
        assert!(nested.get(&str_key("too")).is_none());
    }

    #[test]
    fn http_context_still_does_not_import_host_env() {
        let context = RuntimeContext::controlled_http(http_request());

        let env = global_array(&context, "_ENV");
        assert!(
            env.get(&ArrayKey::String(PhpString::from_test_str("PATH")))
                .is_none()
        );
    }

    #[test]
    fn malformed_percent_encoding_does_not_panic() {
        assert_eq!(
            parse_query_string("bad=%xx&ok=yes"),
            vec![
                ("bad".to_string(), "%xx".to_string()),
                ("ok".to_string(), "yes".to_string())
            ]
        );
        assert_eq!(
            parse_query_string("second=%a&third=%b&decoded=%41"),
            vec![
                ("second".to_string(), "%a".to_string()),
                ("third".to_string(), "%b".to_string()),
                ("decoded".to_string(), "A".to_string())
            ]
        );
    }

    #[test]
    fn query_string_respects_php_arg_separator_input_characters() {
        assert_eq!(
            parse_query_string_with_separators("first=val1/second=val2/third=val3", "/"),
            vec![
                ("first".to_string(), "val1".to_string()),
                ("second".to_string(), "val2".to_string()),
                ("third".to_string(), "val3".to_string())
            ]
        );
        assert_eq!(
            parse_query_string_with_separators("a=1;b=2&c=3", "&;"),
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string()),
                ("c".to_string(), "3".to_string())
            ]
        );
        assert_eq!(
            parse_query_string_with_separators("a=1&b=2", ""),
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string())
            ]
        );
    }

    fn http_request() -> RuntimeHttpRequestContext {
        let mut request = RuntimeHttpRequestContext::new(
            "POST",
            "example.test",
            "/submit.php?name=phrust",
            "/submit.php",
            "/srv/app/submit.php",
            "/srv/app",
        );
        request.server_port = 8080;
        request.server_addr = "127.0.0.1".to_string();
        request.path_info = Some("/extra".to_string());
        request.php_self = "/submit.php/extra".to_string();
        request.remote_addr = "127.0.0.1".to_string();
        request.remote_port = Some(50123);
        request.auth_type = Some("Basic".to_string());
        request.remote_user = Some("alice".to_string());
        request.php_auth_user = Some("alice".to_string());
        request.php_auth_pw = Some("s3cret".to_string());
        request.request_time = 123;
        request.request_time_float_micros = 123_456_789;
        request.content_type = Some("application/x-www-form-urlencoded".to_string());
        request.content_length = Some(7);
        request.headers = vec![
            ("Host".to_string(), "example.test".to_string()),
            (
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            ),
            ("Content-Length".to_string(), "7".to_string()),
            ("X-Test-Header".to_string(), "yes".to_string()),
            ("Bad Header".to_string(), "ignored".to_string()),
        ];
        request.raw_body = b"posted=yes".to_vec().into();
        request.parsed_post = parse_form_urlencoded_body(&request.raw_body);
        request.parsed_cookie = parse_cookie_header("sid=abc; theme=dark");
        request
    }

    fn uploaded_file(field_name: &str, client_filename: &str, size: u64) -> RuntimeUploadedFile {
        RuntimeUploadedFile {
            field_name: field_name.to_string(),
            client_filename: client_filename.to_string(),
            content_type: "image/png".to_string(),
            temp_path: "/tmp/phrust-upload".to_string(),
            error: 0,
            size,
        }
    }

    fn global_array(context: &RuntimeContext, name: &str) -> crate::PhpArray {
        let Some(Value::Array(array)) = context.global_value(name) else {
            panic!("expected {name} array");
        };
        array
    }

    fn assert_string(array: &crate::PhpArray, key: &str, expected: &str) {
        assert_eq!(
            array.get(&ArrayKey::String(PhpString::from_test_str(key))),
            Some(&Value::string(expected.as_bytes().to_vec()))
        );
    }

    fn assert_float(array: &crate::PhpArray, key: &str, expected: f64) {
        match array.get(&ArrayKey::String(PhpString::from_test_str(key))) {
            Some(Value::Float(value)) => {
                assert!(
                    (value.to_f64() - expected).abs() < f64::EPSILON,
                    "{} != {}",
                    value.to_f64(),
                    expected
                );
            }
            value => panic!("expected float for {key}, got {value:?}"),
        }
    }

    fn assert_path_string(array: &crate::PhpArray, path: &[ArrayKey], expected: &str) {
        assert_eq!(
            value_at_path(array, path),
            Some(&Value::string(expected.as_bytes().to_vec()))
        );
    }

    fn assert_path_int(array: &crate::PhpArray, path: &[ArrayKey], expected: i64) {
        assert_eq!(value_at_path(array, path), Some(&Value::Int(expected)));
    }

    fn value_at_path<'a>(array: &'a crate::PhpArray, path: &[ArrayKey]) -> Option<&'a Value> {
        let (first, rest) = path.split_first()?;
        let mut value = array.get(first)?;
        for key in rest {
            let Value::Array(child) = value else {
                return None;
            };
            value = child.get(key)?;
        }
        Some(value)
    }

    fn str_key(value: &str) -> ArrayKey {
        ArrayKey::String(PhpString::from_test_str(value))
    }

    fn int_key(value: i64) -> ArrayKey {
        ArrayKey::Int(value)
    }
}
