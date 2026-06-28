//! Deterministic runtime configuration for CLI fixture execution.

use crate::{ArrayKey, FilesystemCapabilities, IniRegistry, PhpArray, PhpString, Value};
use std::path::PathBuf;

/// Minimal ini-like runtime options carried by the VM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeIniOptions {
    /// Placeholder for PHP's `error_reporting` bitmask.
    pub error_reporting: ErrorReporting,
    /// Placeholder for display_errors-style behavior.
    pub display_errors: bool,
    /// Maximum decoded input variables materialized into each superglobal.
    pub max_input_vars: usize,
    /// Maximum PHP-style bracket nesting materialized for input names.
    pub max_input_nesting_level: usize,
}

impl Default for RuntimeIniOptions {
    fn default() -> Self {
        Self {
            error_reporting: ErrorReporting::default(),
            display_errors: true,
            max_input_vars: 1000,
            max_input_nesting_level: 64,
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
    pub server_port: u16,
    pub server_protocol: String,
    pub request_uri: String,
    pub path: String,
    pub query_string: String,
    pub script_name: String,
    pub php_self: String,
    pub script_filename: String,
    pub document_root: String,
    pub path_info: Option<String>,
    pub remote_addr: String,
    pub request_time: i64,
    pub headers: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    pub parsed_get: Vec<(String, String)>,
    pub parsed_post: Vec<(String, String)>,
    pub parsed_cookie: Vec<(String, String)>,
    pub raw_body: Vec<u8>,
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
        Self {
            method: method.into(),
            scheme: "http".to_string(),
            host: host.into(),
            server_port: 80,
            server_protocol: "HTTP/1.1".to_string(),
            request_uri,
            path,
            query_string: query_string.clone(),
            script_name: script_name.into(),
            php_self: String::new(),
            script_filename: script_filename.into(),
            document_root: document_root.into(),
            path_info: None,
            remote_addr: String::new(),
            request_time: 0,
            headers: Vec::new(),
            content_type: None,
            content_length: None,
            parsed_get: parse_query_string(&query_string),
            parsed_post: Vec::new(),
            parsed_cookie: Vec::new(),
            raw_body: Vec::new(),
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

/// Owned deterministic runtime context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeContext {
    /// Current working directory for future relative-path/runtime behavior.
    pub cwd: PathBuf,
    /// PHP CLI argv vector. Element 0 is the script path when configured.
    pub argv: Vec<String>,
    /// Controlled environment entries. Host env is never imported implicitly.
    pub env: Vec<(String, String)>,
    /// Deterministic bytes exposed through CLI stdin resources.
    pub stdin: Vec<u8>,
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
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self {
            cwd: PathBuf::from("."),
            argv: Vec::new(),
            env: Vec::new(),
            stdin: Vec::new(),
            include_path: vec![PathBuf::from(".")],
            ini: RuntimeIniOptions::default(),
            ini_overrides: Vec::new(),
            strict_types: Vec::new(),
            filesystem: FilesystemCapabilities::none(),
            process: ProcessCapability::Disabled,
            request_mode: RuntimeRequestMode::Cli,
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
        let _ = registry.set("error_reporting", self.ini.error_reporting.mask.to_string());
        let _ = registry.set(
            "display_errors",
            if self.ini.display_errors { "1" } else { "0" },
        );
        let _ = registry.set("max_input_vars", self.ini.max_input_vars.to_string());
        let _ = registry.set(
            "max_input_nesting_level",
            self.ini.max_input_nesting_level.to_string(),
        );
        for (name, value) in &self.ini_overrides {
            let _ = registry.set(name, value.clone());
        }
        registry
    }

    /// Sets controlled environment entries in stable key order.
    #[must_use]
    pub fn with_env(mut self, mut env: Vec<(String, String)>) -> Self {
        env.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        self.env = env;
        self
    }

    /// Sets deterministic stdin bytes for CLI-style execution.
    #[must_use]
    pub fn with_stdin(mut self, stdin: Vec<u8>) -> Self {
        self.stdin = stdin;
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

    /// Sets deterministic HTTP request metadata.
    #[must_use]
    pub fn with_http_request(mut self, request: RuntimeHttpRequestContext) -> Self {
        self.request_mode = RuntimeRequestMode::Http(Box::new(request));
        self
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
            "_FILES" | "_SESSION" | "GLOBALS" => Some(Value::Array(PhpArray::new())),
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

    fn env_array(&self) -> PhpArray {
        let mut array = PhpArray::new();
        for (key, value) in &self.env {
            array.insert(string_key(key), Value::string(value.as_bytes().to_vec()));
        }
        array
    }

    fn get_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => input_pairs_array(&request.parsed_get, &self.ini),
            RuntimeRequestMode::Cli => PhpArray::new(),
        }
    }

    fn post_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => input_pairs_array(&request.parsed_post, &self.ini),
            RuntimeRequestMode::Cli => PhpArray::new(),
        }
    }

    fn cookie_array(&self) -> PhpArray {
        match &self.request_mode {
            RuntimeRequestMode::Http(request) => {
                flat_pairs_array(&request.parsed_cookie, &self.ini)
            }
            RuntimeRequestMode::Cli => PhpArray::new(),
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
            RuntimeRequestMode::Cli => PhpArray::new(),
        }
    }
}

fn string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_test_str(value))
}

fn input_key(value: &str) -> ArrayKey {
    ArrayKey::from_php_string(PhpString::from_test_str(value))
}

fn http_server_array(request: &RuntimeHttpRequestContext) -> PhpArray {
    let mut array = PhpArray::new();
    insert_string(&mut array, "REQUEST_METHOD", &request.method);
    insert_string(&mut array, "REQUEST_SCHEME", &request.scheme);
    insert_string(&mut array, "HTTP_HOST", &request.host);
    insert_string(&mut array, "SERVER_PORT", &request.server_port.to_string());
    insert_string(&mut array, "SERVER_PROTOCOL", &request.server_protocol);
    insert_string(&mut array, "REQUEST_URI", &request.request_uri);
    insert_string(&mut array, "DOCUMENT_URI", &request.path);
    insert_string(&mut array, "SCRIPT_NAME", &request.script_name);
    insert_string(&mut array, "PHP_SELF", request.php_self());
    insert_string(&mut array, "SCRIPT_FILENAME", &request.script_filename);
    insert_string(&mut array, "DOCUMENT_ROOT", &request.document_root);
    insert_string(&mut array, "QUERY_STRING", &request.query_string);
    insert_string(&mut array, "REMOTE_ADDR", &request.remote_addr);
    array.insert(string_key("REQUEST_TIME"), Value::Int(request.request_time));
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

fn header_server_name(name: &str) -> Option<String> {
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

fn input_pairs_array(pairs: &[(String, String)], ini: &RuntimeIniOptions) -> PhpArray {
    let mut array = PhpArray::new();
    InputArrayBuilder::new(ini).insert_pairs(&mut array, pairs);
    array
}

fn flat_pairs_array(pairs: &[(String, String)], ini: &RuntimeIniOptions) -> PhpArray {
    let mut array = PhpArray::new();
    InputArrayBuilder::new(ini).insert_flat_pairs(&mut array, pairs);
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
}

impl InputArrayBuilder {
    fn new(ini: &RuntimeIniOptions) -> Self {
        Self {
            remaining_vars: ini.max_input_vars,
            max_input_nesting_level: ini.max_input_nesting_level,
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
            insert_input_value(array, &segments, value);
        }
    }

    fn insert_flat_pairs(&mut self, array: &mut PhpArray, pairs: &[(String, String)]) {
        for (key, value) in pairs {
            if !self.consume_var() {
                break;
            }
            insert_string(array, key, value);
        }
    }

    fn consume_var(&mut self) -> bool {
        if self.remaining_vars == 0 {
            return false;
        }
        self.remaining_vars -= 1;
        true
    }
}

fn insert_string(array: &mut PhpArray, key: &str, value: &str) {
    array.insert(string_key(key), Value::string(value.as_bytes().to_vec()));
}

fn parse_input_key_segments(key: &str, max_nesting_level: usize) -> Option<Vec<InputKeySegment>> {
    if key.is_empty() {
        return None;
    }
    let Some(first_bracket) = key.find('[') else {
        return Some(vec![InputKeySegment::Key(input_key(key))]);
    };
    if first_bracket == 0 {
        return Some(vec![InputKeySegment::Key(input_key(key))]);
    }

    let mut segments = vec![InputKeySegment::Key(input_key(&key[..first_bracket]))];
    let mut rest = &key[first_bracket..];
    while !rest.is_empty() {
        if !rest.starts_with('[') {
            return Some(vec![InputKeySegment::Key(input_key(key))]);
        }
        let Some(close) = rest.find(']') else {
            return Some(vec![InputKeySegment::Key(input_key(key))]);
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

fn insert_input_value(array: &mut PhpArray, segments: &[InputKeySegment], value: &str) {
    insert_input_at(array, segments, Value::string(value.as_bytes().to_vec()));
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
            let Some(Value::Array(child)) = array.get_mut(key) else {
                unreachable!("input child was just initialized as an array")
            };
            insert_input_at(child, tail, value);
        }
        InputKeySegment::Append => {
            let key = array.append(Value::Array(PhpArray::new()));
            let Some(Value::Array(child)) = array.get_mut(&key) else {
                unreachable!("input append child was just initialized as an array")
            };
            insert_input_at(child, tail, value);
        }
    }
}

#[must_use]
pub fn parse_query_string(query: &str) -> Vec<(String, String)> {
    parse_form_urlencoded(query.as_bytes())
}

#[must_use]
pub fn parse_form_urlencoded_body(body: &[u8]) -> Vec<(String, String)> {
    parse_form_urlencoded(body)
}

fn parse_form_urlencoded(input: &[u8]) -> Vec<(String, String)> {
    input
        .split(|byte| *byte == b'&')
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (name, value) = split_bytes_once(part, b'=').unwrap_or((part, &[]));
            Some((decode_component(name)?, decode_component(value)?))
        })
        .collect()
}

fn split_bytes_once(input: &[u8], delimiter: u8) -> Option<(&[u8], &[u8])> {
    let index = input.iter().position(|byte| *byte == delimiter)?;
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
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect()
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
                let high = *input.get(index + 1)?;
                let low = *input.get(index + 2)?;
                output.push(hex_value(high)? << 4 | hex_value(low)?);
                index += 3;
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
        StrictTypesInfo, input_pairs_array, parse_cookie_header, parse_form_urlencoded_body,
        parse_query_string,
    };
    use crate::{ArrayKey, PhpString, Value};

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
            context.ini_registry().get("max_input_nesting_level"),
            Some("64")
        );
        assert_eq!(context.process, super::ProcessCapability::Disabled);
        assert!(context.strict_types.is_empty());
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
        assert_eq!(
            RuntimeContext::default().env,
            Vec::<(String, String)>::new()
        );
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
        assert_string(&server, "SERVER_PORT", "8080");
        assert_string(&server, "SERVER_PROTOCOL", "HTTP/1.1");
        assert_string(&server, "REQUEST_URI", "/submit.php?name=phrust");
        assert_string(&server, "SCRIPT_NAME", "/submit.php");
        assert_string(&server, "PHP_SELF", "/submit.php/extra");
        assert_string(&server, "SCRIPT_FILENAME", "/srv/app/submit.php");
        assert_string(&server, "DOCUMENT_ROOT", "/srv/app");
        assert_string(&server, "PATH_INFO", "/extra");
        assert_string(&server, "QUERY_STRING", "name=phrust");
        assert_string(&server, "REMOTE_ADDR", "127.0.0.1");
        assert_string(&server, "CONTENT_TYPE", "application/x-www-form-urlencoded");
        assert_string(&server, "CONTENT_LENGTH", "7");
        assert_string(&server, "HTTP_X_TEST_HEADER", "yes");
        assert_eq!(
            server.get(&ArrayKey::String(PhpString::from_test_str("REQUEST_TIME"))),
            Some(&Value::Int(123))
        );
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
            vec![("ok".to_string(), "yes".to_string())]
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
        request.path_info = Some("/extra".to_string());
        request.php_self = "/submit.php/extra".to_string();
        request.remote_addr = "127.0.0.1".to_string();
        request.request_time = 123;
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
        request.raw_body = b"posted=yes".to_vec();
        request.parsed_post = parse_form_urlencoded_body(&request.raw_body);
        request.parsed_cookie = parse_cookie_header("sid=abc; theme=dark");
        request
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

    fn assert_path_string(array: &crate::PhpArray, path: &[ArrayKey], expected: &str) {
        assert_eq!(
            value_at_path(array, path),
            Some(&Value::string(expected.as_bytes().to_vec()))
        );
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
