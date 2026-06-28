//! cURL-compatible HTTP client builtin slice.

use super::core::{argument_type_error, expect_arity, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{
    ArrayKey, ClassEntry, ClassFlags, FloatValue, ObjectRef, PhpArray, PhpString, Value,
    normalize_class_name,
};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("curl_close", builtin_curl_close, BuiltinCompatibility::Php),
    BuiltinEntry::new("curl_errno", builtin_curl_errno, BuiltinCompatibility::Php),
    BuiltinEntry::new("curl_error", builtin_curl_error, BuiltinCompatibility::Php),
    BuiltinEntry::new("curl_exec", builtin_curl_exec, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "curl_getinfo",
        builtin_curl_getinfo,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("curl_init", builtin_curl_init, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "curl_setopt",
        builtin_curl_setopt,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_version",
        builtin_curl_version,
        BuiltinCompatibility::Php,
    ),
];

pub const PHRUST_NET_TESTS_ENV: &str = "PHRUST_NET_TESTS";
const CURLOPT_URL: i64 = 10002;
const CURLOPT_RETURNTRANSFER: i64 = 19913;
const CURLOPT_TIMEOUT: i64 = 13;
const CURLOPT_TIMEOUT_MS: i64 = 155;
const CURLOPT_FOLLOWLOCATION: i64 = 52;
const CURLOPT_HTTPHEADER: i64 = 10023;
const CURLOPT_POST: i64 = 47;
const CURLOPT_POSTFIELDS: i64 = 10015;
const CURLOPT_CUSTOMREQUEST: i64 = 10036;
const CURLOPT_SSL_VERIFYPEER: i64 = 64;
const CURLOPT_SSL_VERIFYHOST: i64 = 81;
const CURLINFO_EFFECTIVE_URL: i64 = 1048577;
const CURLINFO_RESPONSE_CODE: i64 = 2097154;
const CURLINFO_TOTAL_TIME: i64 = 3145731;

pub(in crate::builtins::modules) fn builtin_curl_version(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_version", &args, 0)?;
    let mut out = PhpArray::new();
    out.insert(
        ArrayKey::String(PhpString::from("version")),
        Value::String(PhpString::from("phrust-curl-mvp")),
    );
    out.insert(
        ArrayKey::String(PhpString::from("ssl_version")),
        Value::String(PhpString::from("none")),
    );
    out.insert(
        ArrayKey::String(PhpString::from("protocols")),
        Value::packed_array(vec![Value::String(PhpString::from("http"))]),
    );
    Ok(Value::Array(out))
}

pub(in crate::builtins::modules) fn builtin_curl_init(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin curl_init expects zero or one argument(s)",
        ));
    }
    let handle = curl_handle_object();
    if let Some(url) = args.first() {
        let url = string_arg("curl_init", url)?;
        handle.set_property("__curl_url", Value::String(url));
    }
    Ok(Value::Object(handle))
}

pub(in crate::builtins::modules) fn builtin_curl_setopt(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_setopt", &args, 3)?;
    let handle = curl_handle_arg("curl_setopt", args.first())?;
    let option = int_arg("curl_setopt", &args[1])?;
    let value = args[2].clone();
    let property = match option {
        CURLOPT_URL => {
            let value = string_arg("curl_setopt", &value)?;
            handle.set_property("__curl_url", Value::String(value));
            return Ok(Value::Bool(true));
        }
        CURLOPT_RETURNTRANSFER => "__curl_returntransfer",
        CURLOPT_TIMEOUT => "__curl_timeout",
        CURLOPT_TIMEOUT_MS => "__curl_timeout_ms",
        CURLOPT_FOLLOWLOCATION => "__curl_followlocation",
        CURLOPT_HTTPHEADER => "__curl_httpheader",
        CURLOPT_POST => "__curl_post",
        CURLOPT_POSTFIELDS => "__curl_postfields",
        CURLOPT_CUSTOMREQUEST => "__curl_customrequest",
        CURLOPT_SSL_VERIFYPEER => "__curl_ssl_verifypeer",
        CURLOPT_SSL_VERIFYHOST => "__curl_ssl_verifyhost",
        _ => {
            set_curl_error(&handle, 48, "unsupported cURL option");
            return Ok(Value::Bool(false));
        }
    };
    handle.set_property(property, value);
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_curl_exec(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_exec", &args, 1)?;
    let handle = curl_handle_arg("curl_exec", args.first())?;
    if std::env::var(PHRUST_NET_TESTS_ENV).as_deref() != Ok("1") {
        set_curl_error(
            &handle,
            1,
            format!("network cURL requests require {PHRUST_NET_TESTS_ENV}=1"),
        );
        return Ok(Value::Bool(false));
    }
    let request = match build_request(&handle) {
        Ok(request) => request,
        Err((code, message)) => {
            set_curl_error(&handle, code, message);
            return Ok(Value::Bool(false));
        }
    };
    let start = Instant::now();
    let response = match execute_http_request(&request) {
        Ok(response) => response,
        Err((code, message)) => {
            set_curl_error(&handle, code, message);
            return Ok(Value::Bool(false));
        }
    };
    set_curl_error(&handle, 0, "");
    handle.set_property("__curl_http_code", Value::Int(i64::from(response.status)));
    handle.set_property(
        "__curl_effective_url",
        Value::String(PhpString::from(request.url.into_bytes())),
    );
    handle.set_property(
        "__curl_total_time",
        Value::Float(FloatValue::from_f64(start.elapsed().as_secs_f64())),
    );
    if curl_bool_property(&handle, "__curl_returntransfer") {
        Ok(Value::string(response.body))
    } else {
        context.output().write_bytes(&response.body);
        Ok(Value::Bool(true))
    }
}

pub(in crate::builtins::modules) fn builtin_curl_getinfo(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin curl_getinfo expects one or two argument(s)",
        ));
    }
    let handle = curl_handle_arg("curl_getinfo", args.first())?;
    if let Some(option) = args.get(1) {
        let option = int_arg("curl_getinfo", option)?;
        return Ok(match option {
            CURLINFO_RESPONSE_CODE => curl_int_property(&handle, "__curl_http_code"),
            CURLINFO_EFFECTIVE_URL => curl_string_property(&handle, "__curl_effective_url"),
            CURLINFO_TOTAL_TIME => curl_float_property(&handle, "__curl_total_time"),
            _ => Value::Bool(false),
        });
    }
    let mut out = PhpArray::new();
    out.insert(
        ArrayKey::String(PhpString::from("http_code")),
        curl_int_property(&handle, "__curl_http_code"),
    );
    out.insert(
        ArrayKey::String(PhpString::from("url")),
        curl_string_property(&handle, "__curl_effective_url"),
    );
    out.insert(
        ArrayKey::String(PhpString::from("total_time")),
        curl_float_property(&handle, "__curl_total_time"),
    );
    Ok(Value::Array(out))
}

pub(in crate::builtins::modules) fn builtin_curl_errno(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_errno", &args, 1)?;
    let handle = curl_handle_arg("curl_errno", args.first())?;
    Ok(curl_int_property(&handle, "__curl_errno"))
}

pub(in crate::builtins::modules) fn builtin_curl_error(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_error", &args, 1)?;
    let handle = curl_handle_arg("curl_error", args.first())?;
    Ok(curl_string_property(&handle, "__curl_error"))
}

pub(in crate::builtins::modules) fn builtin_curl_close(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_close", &args, 1)?;
    let handle = curl_handle_arg("curl_close", args.first())?;
    handle.set_property("__curl_closed", Value::Bool(true));
    Ok(Value::Null)
}

fn build_request(handle: &ObjectRef) -> Result<CurlRequest, (i64, String)> {
    if handle.get_property("__curl_closed") == Some(Value::Bool(true)) {
        return Err((3, "cURL handle is closed".to_owned()));
    }
    let url = match handle.get_property("__curl_url") {
        Some(Value::String(value)) if !value.is_empty() => value.to_string_lossy(),
        _ => return Err((3, "cURL URL is empty".to_owned())),
    };
    let parsed = parse_http_url(&url)?;
    if !matches!(parsed.host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        return Err((
            7,
            "cURL MVP only permits local loopback hosts when network tests are enabled".to_owned(),
        ));
    }
    let post = curl_bool_property(handle, "__curl_post")
        || matches!(
            handle.get_property("__curl_postfields"),
            Some(Value::String(_))
        );
    let method = match handle.get_property("__curl_customrequest") {
        Some(Value::String(value)) if !value.is_empty() => value.to_string_lossy(),
        _ if post => "POST".to_owned(),
        _ => "GET".to_owned(),
    };
    let body = match handle.get_property("__curl_postfields") {
        Some(Value::String(value)) => value.as_bytes().to_vec(),
        Some(value) => string_arg("curl_exec", &value)
            .map(|value| value.as_bytes().to_vec())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    Ok(CurlRequest {
        url,
        host: parsed.host,
        port: parsed.port,
        path: parsed.path,
        method,
        headers: curl_header_lines(handle),
        body,
        timeout: curl_timeout(handle),
    })
}

fn execute_http_request(request: &CurlRequest) -> Result<CurlResponse, (i64, String)> {
    let mut addrs = (request.host.as_str(), request.port)
        .to_socket_addrs()
        .map_err(|error| (7, format!("failed to resolve local cURL host: {error}")))?;
    let Some(addr) = addrs.next() else {
        return Err((7, "failed to resolve local cURL host".to_owned()));
    };
    let mut stream = TcpStream::connect_timeout(&addr, request.timeout)
        .map_err(|error| (7, format!("failed to connect local cURL host: {error}")))?;
    stream
        .set_read_timeout(Some(request.timeout))
        .map_err(|error| (28, format!("failed to set cURL read timeout: {error}")))?;
    stream
        .set_write_timeout(Some(request.timeout))
        .map_err(|error| (28, format!("failed to set cURL write timeout: {error}")))?;
    let mut payload = Vec::new();
    write!(
        payload,
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
        request.method, request.path, request.host
    )
    .expect("write to vec");
    for header in &request.headers {
        payload.extend_from_slice(header.as_bytes());
        payload.extend_from_slice(b"\r\n");
    }
    if !request.body.is_empty() {
        write!(payload, "Content-Length: {}\r\n", request.body.len()).expect("write to vec");
    }
    payload.extend_from_slice(b"\r\n");
    payload.extend_from_slice(&request.body);
    stream
        .write_all(&payload)
        .map_err(|error| (55, format!("failed to write cURL request: {error}")))?;
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| (56, format!("failed to read cURL response: {error}")))?;
    parse_http_response(&bytes)
}

fn parse_http_response(bytes: &[u8]) -> Result<CurlResponse, (i64, String)> {
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| (56, "invalid HTTP response".to_owned()))?;
    let header = String::from_utf8_lossy(&bytes[..header_end]);
    let status = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| (56, "invalid HTTP status line".to_owned()))?;
    Ok(CurlResponse {
        status,
        body: bytes[header_end + 4..].to_vec(),
    })
}

fn parse_http_url(url: &str) -> Result<ParsedUrl, (i64, String)> {
    let Some(rest) = url.strip_prefix("http://") else {
        if url.starts_with("https://") {
            return Err((
                1,
                "cURL MVP does not implement HTTPS transport yet".to_owned(),
            ));
        }
        return Err((3, "cURL MVP only supports http:// URLs".to_owned()));
    };
    let (authority, path) = rest.split_once('/').map_or((rest, "/"), |(host, path)| {
        (host, if path.is_empty() { "/" } else { "" })
    });
    let path = if let Some((_, path)) = rest.split_once('/') {
        format!("/{path}")
    } else {
        path.to_owned()
    };
    let (host, port) = authority
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
        .map_or((authority, 80), |(host, port)| (host, port));
    if host.is_empty() {
        return Err((3, "cURL URL host is empty".to_owned()));
    }
    Ok(ParsedUrl {
        host: host.trim_matches(['[', ']']).to_owned(),
        port,
        path,
    })
}

fn curl_header_lines(handle: &ObjectRef) -> Vec<String> {
    match handle.get_property("__curl_httpheader") {
        Some(Value::Array(array)) => array
            .iter()
            .filter_map(|(_, value)| string_arg("curl_exec", value).ok())
            .map(|value| value.to_string_lossy())
            .collect(),
        _ => Vec::new(),
    }
}

fn curl_timeout(handle: &ObjectRef) -> Duration {
    if let Some(Value::Int(ms)) = handle.get_property("__curl_timeout_ms") {
        return Duration::from_millis(ms.clamp(1, 30_000) as u64);
    }
    if let Some(Value::Int(seconds)) = handle.get_property("__curl_timeout") {
        return Duration::from_secs(seconds.clamp(1, 30) as u64);
    }
    Duration::from_secs(5)
}

fn curl_handle_object() -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&curl_runtime_class("CurlHandle"), "CurlHandle");
    object.set_property("__curl_errno", Value::Int(0));
    object.set_property("__curl_error", Value::String(PhpString::from("")));
    object.set_property("__curl_returntransfer", Value::Bool(false));
    object.set_property("__curl_http_code", Value::Int(0));
    object.set_property("__curl_effective_url", Value::String(PhpString::from("")));
    object.set_property("__curl_total_time", Value::Float(FloatValue::from_f64(0.0)));
    object
}

fn curl_runtime_class(name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: ClassFlags::default(),
    }
}

fn curl_handle_arg(name: &str, value: Option<&Value>) -> Result<ObjectRef, BuiltinError> {
    match value {
        Some(Value::Object(object)) if object.class_name() == "curlhandle" => Ok(object.clone()),
        Some(value) => Err(argument_type_error(name, "1", "CurlHandle", value)),
        None => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            format!("builtin {name} expects CurlHandle argument"),
        )),
    }
}

fn set_curl_error(handle: &ObjectRef, errno: i64, error: impl Into<String>) {
    handle.set_property("__curl_errno", Value::Int(errno));
    handle.set_property(
        "__curl_error",
        Value::String(PhpString::from(error.into().into_bytes())),
    );
}

fn curl_bool_property(handle: &ObjectRef, name: &str) -> bool {
    handle
        .get_property(name)
        .and_then(|value| crate::convert::to_bool(&value).ok())
        .unwrap_or(false)
}

fn curl_int_property(handle: &ObjectRef, name: &str) -> Value {
    match handle.get_property(name) {
        Some(Value::Int(value)) => Value::Int(value),
        _ => Value::Int(0),
    }
}

fn curl_float_property(handle: &ObjectRef, name: &str) -> Value {
    match handle.get_property(name) {
        Some(Value::Float(value)) => Value::Float(value),
        _ => Value::Float(FloatValue::from_f64(0.0)),
    }
}

fn curl_string_property(handle: &ObjectRef, name: &str) -> Value {
    match handle.get_property(name) {
        Some(Value::String(value)) => Value::String(value),
        _ => Value::String(PhpString::from("")),
    }
}

struct ParsedUrl {
    host: String,
    port: u16,
    path: String,
}

struct CurlRequest {
    url: String,
    host: String,
    port: u16,
    path: String,
    method: String,
    headers: Vec<String>,
    body: Vec<u8>,
    timeout: Duration,
}

struct CurlResponse {
    status: u16,
    body: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::thread;

    static NET_TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn curl_exec_is_network_disabled_by_default() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let old_env = std::env::var(PHRUST_NET_TESTS_ENV).ok();
        unsafe {
            std::env::remove_var(PHRUST_NET_TESTS_ENV);
        }
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string("http://127.0.0.1:1/")],
            RuntimeSourceSpan::default(),
        )
        .expect("init");

        assert_eq!(
            builtin_curl_exec(
                &mut context,
                vec![handle.clone()],
                RuntimeSourceSpan::default()
            )
            .expect("exec"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_curl_errno(&mut context, vec![handle], RuntimeSourceSpan::default())
                .expect("errno"),
            Value::Int(1)
        );
        restore_net_tests_env(old_env);
    }

    #[test]
    fn curl_exec_gets_local_http_when_net_tests_are_enabled() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let old_env = std::env::var(PHRUST_NET_TESTS_ENV).ok();
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).expect("read request");
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /wp-json"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                .expect("write response");
        });

        unsafe {
            std::env::set_var(PHRUST_NET_TESTS_ENV, "1");
        }
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("http://127.0.0.1:{port}/wp-json"))],
            RuntimeSourceSpan::default(),
        )
        .expect("init");
        builtin_curl_setopt(
            &mut context,
            vec![
                handle.clone(),
                Value::Int(CURLOPT_RETURNTRANSFER),
                Value::Bool(true),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("set return transfer");

        assert_eq!(
            builtin_curl_exec(
                &mut context,
                vec![handle.clone()],
                RuntimeSourceSpan::default()
            )
            .expect("exec"),
            Value::string("OK")
        );
        assert_eq!(
            builtin_curl_getinfo(
                &mut context,
                vec![handle, Value::Int(CURLINFO_RESPONSE_CODE)],
                RuntimeSourceSpan::default(),
            )
            .expect("info"),
            Value::Int(200)
        );
        restore_net_tests_env(old_env);
        server.join().expect("server");
    }

    fn restore_net_tests_env(value: Option<String>) {
        unsafe {
            match value {
                Some(value) => std::env::set_var(PHRUST_NET_TESTS_ENV, value),
                None => std::env::remove_var(PHRUST_NET_TESTS_ENV),
            }
        }
    }
}
