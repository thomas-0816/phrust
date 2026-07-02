//! cURL-compatible HTTP client builtin slice.

use super::core::{argument_type_error, expect_arity, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{
    ArrayKey, ClassEntry, ClassFlags, FloatValue, ObjectRef, PhpArray, PhpString,
    RuntimeDiagnostic, RuntimeDiagnosticPayload, RuntimeSeverity, Value,
    WordPressDiagnosticContext, normalize_class_name,
};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(test)]
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("curl_close", builtin_curl_close, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "curl_copy_handle",
        builtin_curl_copy_handle,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("curl_errno", builtin_curl_errno, BuiltinCompatibility::Php),
    BuiltinEntry::new("curl_error", builtin_curl_error, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "curl_escape",
        builtin_curl_escape,
        BuiltinCompatibility::Php,
    ),
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
        "curl_setopt_array",
        builtin_curl_setopt_array,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_multi_strerror",
        builtin_curl_multi_strerror,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_multi_init",
        builtin_curl_multi_init,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_multi_add_handle",
        builtin_curl_multi_add_handle,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_multi_exec",
        builtin_curl_multi_exec,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_multi_close",
        builtin_curl_multi_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("curl_reset", builtin_curl_reset, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "curl_unescape",
        builtin_curl_unescape,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_version",
        builtin_curl_version,
        BuiltinCompatibility::Php,
    ),
];

pub const PHRUST_NET_TESTS_ENV: &str = "PHRUST_NET_TESTS";
#[cfg(test)]
static NET_TESTS_OVERRIDE: Mutex<Option<bool>> = Mutex::new(None);
const CURLOPT_URL: i64 = 10002;
const CURLOPT_RETURNTRANSFER: i64 = 19913;
const CURLOPT_TIMEOUT: i64 = 13;
const CURLOPT_TIMEOUT_MS: i64 = 155;
const CURLOPT_FOLLOWLOCATION: i64 = 52;
const CURLOPT_HEADER: i64 = 42;
const CURLOPT_NOBODY: i64 = 44;
const CURLOPT_USERAGENT: i64 = 10018;
const CURLOPT_REFERER: i64 = 10016;
const CURLOPT_ENCODING: i64 = 10102;
const CURLOPT_HTTP_VERSION: i64 = 84;
const CURLOPT_CONNECTTIMEOUT: i64 = 78;
const CURLOPT_CONNECTTIMEOUT_MS: i64 = 156;
const CURLOPT_MAXREDIRS: i64 = 68;
const CURLOPT_FAILONERROR: i64 = 45;
const CURLOPT_HTTPHEADER: i64 = 10023;
const CURLOPT_HEADERFUNCTION: i64 = 20079;
const CURLOPT_WRITEFUNCTION: i64 = 20011;
const CURLOPT_BUFFERSIZE: i64 = 98;
const CURLOPT_CAINFO: i64 = 10065;
const CURLOPT_HTTPAUTH: i64 = 107;
const CURLOPT_PROTOCOLS: i64 = 181;
const CURLOPT_PROXY: i64 = 10004;
const CURLOPT_PROXYAUTH: i64 = 111;
const CURLOPT_PROXYPORT: i64 = 59;
const CURLOPT_PROXYTYPE: i64 = 101;
const CURLOPT_PROXYUSERPWD: i64 = 10006;
const CURLOPT_REDIR_PROTOCOLS: i64 = 182;
const CURLOPT_USERPWD: i64 = 10005;
const CURLOPT_POST: i64 = 47;
const CURLOPT_POSTFIELDS: i64 = 10015;
const CURLOPT_CUSTOMREQUEST: i64 = 10036;
const CURLOPT_SSL_VERIFYPEER: i64 = 64;
const CURLOPT_SSL_VERIFYHOST: i64 = 81;
const CURLINFO_EFFECTIVE_URL: i64 = 1048577;
const CURLINFO_RESPONSE_CODE: i64 = 2097154;
const CURLINFO_HEADER_SIZE: i64 = 2097163;
const CURLINFO_TOTAL_TIME: i64 = 3145731;
const CURLM_OK: i64 = 0;
const CURLM_BAD_HANDLE: i64 = 1;

type CurlTransportError = (i64, String);
type CurlPostBody = (Vec<u8>, Option<&'static str>);

pub(in crate::builtins::modules) fn builtin_curl_version(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_version", &args, 0)?;
    let mut out = PhpArray::new();
    out.insert(
        ArrayKey::String(PhpString::from("version_number")),
        Value::Int(0x080507),
    );
    out.insert(ArrayKey::String(PhpString::from("age")), Value::Int(0));
    out.insert(ArrayKey::String(PhpString::from("features")), Value::Int(0));
    out.insert(
        ArrayKey::String(PhpString::from("ssl_version_number")),
        Value::Int(0),
    );
    out.insert(
        ArrayKey::String(PhpString::from("version")),
        Value::String(PhpString::from("phrust-curl-mvp")),
    );
    out.insert(
        ArrayKey::String(PhpString::from("host")),
        Value::String(PhpString::from("phrust")),
    );
    out.insert(
        ArrayKey::String(PhpString::from("ssl_version")),
        Value::String(PhpString::from("none")),
    );
    out.insert(
        ArrayKey::String(PhpString::from("libz_version")),
        Value::String(PhpString::from("none")),
    );
    out.insert(
        ArrayKey::String(PhpString::from("protocols")),
        Value::packed_array(vec![Value::String(PhpString::from("http"))]),
    );
    Ok(Value::Array(out))
}

pub(in crate::builtins::modules) fn builtin_curl_escape(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_escape", &args, 2)?;
    let _ = curl_handle_arg("curl_escape", args.first())?;
    let input = string_arg("curl_escape", &args[1])?;
    Ok(Value::string(percent_encode_uri_component(
        input.as_bytes(),
    )))
}

pub(in crate::builtins::modules) fn builtin_curl_unescape(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_unescape", &args, 2)?;
    let _ = curl_handle_arg("curl_unescape", args.first())?;
    let input = string_arg("curl_unescape", &args[1])?;
    Ok(Value::string(percent_decode_uri_component(
        input.as_bytes(),
    )))
}

pub(in crate::builtins::modules) fn builtin_curl_multi_strerror(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_strerror", &args, 1)?;
    let code = int_arg("curl_multi_strerror", &args[0])?;
    let message = match code {
        CURLM_OK => "No error",
        CURLM_BAD_HANDLE => "Invalid multi handle",
        _ => "Unknown error",
    };
    Ok(Value::string(message))
}

pub(in crate::builtins::modules) fn builtin_curl_multi_init(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_init", &args, 0)?;
    Ok(Value::Object(ObjectRef::new_with_display_name(
        &curl_runtime_class("CurlMultiHandle"),
        "CurlMultiHandle",
    )))
}

pub(in crate::builtins::modules) fn builtin_curl_multi_add_handle(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_add_handle", &args, 2)?;
    let _multi = curl_multi_handle_arg("curl_multi_add_handle", args.first())?;
    let _handle = curl_handle_arg("curl_multi_add_handle", args.get(1))?;
    Ok(Value::Int(CURLM_OK))
}

pub(in crate::builtins::modules) fn builtin_curl_multi_exec(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_exec", &args, 2)?;
    let _multi = curl_multi_handle_arg("curl_multi_exec", args.first())?;
    if let Some(Value::Reference(cell)) = args.get(1) {
        cell.set(Value::Int(0));
    }
    Ok(Value::Int(CURLM_OK))
}

pub(in crate::builtins::modules) fn builtin_curl_multi_close(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_close", &args, 1)?;
    let multi = curl_multi_handle_arg("curl_multi_close", args.first())?;
    multi.set_property("__curl_multi_closed", Value::Bool(true));
    Ok(Value::Null)
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
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
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
        CURLOPT_HEADER => "__curl_header",
        CURLOPT_NOBODY => "__curl_nobody",
        CURLOPT_USERAGENT => "__curl_useragent",
        CURLOPT_REFERER => "__curl_referer",
        CURLOPT_ENCODING => "__curl_encoding",
        CURLOPT_HTTP_VERSION => "__curl_http_version",
        CURLOPT_CONNECTTIMEOUT => "__curl_connecttimeout",
        CURLOPT_CONNECTTIMEOUT_MS => "__curl_connecttimeout_ms",
        CURLOPT_MAXREDIRS => "__curl_maxredirs",
        CURLOPT_FAILONERROR => "__curl_failonerror",
        CURLOPT_HTTPHEADER => "__curl_httpheader",
        CURLOPT_HEADERFUNCTION => "__curl_headerfunction",
        CURLOPT_WRITEFUNCTION => "__curl_writefunction",
        CURLOPT_BUFFERSIZE => "__curl_buffersize",
        CURLOPT_CAINFO => "__curl_cainfo",
        CURLOPT_HTTPAUTH => "__curl_httpauth",
        CURLOPT_PROTOCOLS => "__curl_protocols",
        CURLOPT_PROXY => "__curl_proxy",
        CURLOPT_PROXYAUTH => "__curl_proxyauth",
        CURLOPT_PROXYPORT => "__curl_proxyport",
        CURLOPT_PROXYTYPE => "__curl_proxytype",
        CURLOPT_PROXYUSERPWD => "__curl_proxyuserpwd",
        CURLOPT_REDIR_PROTOCOLS => "__curl_redir_protocols",
        CURLOPT_USERPWD => "__curl_userpwd",
        CURLOPT_POST => "__curl_post",
        CURLOPT_POSTFIELDS => "__curl_postfields",
        CURLOPT_CUSTOMREQUEST => "__curl_customrequest",
        CURLOPT_SSL_VERIFYPEER => "__curl_ssl_verifypeer",
        CURLOPT_SSL_VERIFYHOST => "__curl_ssl_verifyhost",
        _ => {
            set_curl_error(&handle, 48, "unsupported cURL option");
            record_curl_diagnostic(
                context,
                &handle,
                CurlDiagnostic::new(
                    "E_PHP_CURL_OPTION_UNSUPPORTED",
                    "curl_setopt",
                    "set_option",
                    "enabled",
                    48,
                    "unsupported cURL option",
                ),
                span.clone(),
            );
            return Ok(Value::Bool(false));
        }
    };
    handle.set_property(property, value);
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_curl_setopt_array(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_setopt_array", &args, 2)?;
    let handle = curl_handle_arg("curl_setopt_array", args.first())?;
    let Value::Array(options) = &args[1] else {
        return Err(argument_type_error(
            "curl_setopt_array",
            "2",
            "array",
            &args[1],
        ));
    };
    for (key, value) in options.iter() {
        let option = match key {
            ArrayKey::Int(option) => *option,
            ArrayKey::String(option) => option.to_string_lossy().parse().unwrap_or(-1),
        };
        let ok = set_curl_option(&handle, option, value.clone())?;
        if !matches!(ok, Value::Bool(true)) {
            record_curl_diagnostic(
                context,
                &handle,
                CurlDiagnostic::new(
                    "E_PHP_CURL_OPTION_UNSUPPORTED",
                    "curl_setopt_array",
                    "set_option",
                    "enabled",
                    48,
                    "unsupported cURL option",
                ),
                span,
            );
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_curl_exec(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_exec", &args, 1)?;
    let handle = curl_handle_arg("curl_exec", args.first())?;
    if !curl_network_requests_enabled(context) && !curl_handle_targets_loopback(&handle) {
        set_curl_error(
            &handle,
            1,
            format!("network cURL requests require {PHRUST_NET_TESTS_ENV}=1"),
        );
        record_curl_diagnostic(
            context,
            &handle,
            CurlDiagnostic::new(
                "E_PHP_CURL_CAPABILITY_DISABLED",
                "curl_exec",
                "http_request",
                "disabled",
                1,
                format!("network cURL requests require {PHRUST_NET_TESTS_ENV}=1"),
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    let request = match build_request(&handle) {
        Ok(request) => request,
        Err((code, message)) => {
            set_curl_error(&handle, code, message.clone());
            record_curl_diagnostic(
                context,
                &handle,
                CurlDiagnostic::new(
                    "E_PHP_CURL_REQUEST_FAILED",
                    "curl_exec",
                    "build_request",
                    "enabled",
                    code,
                    message,
                ),
                span,
            );
            return Ok(Value::Bool(false));
        }
    };
    let start = Instant::now();
    let response = match execute_http_request(&request) {
        Ok(response) => response,
        Err((code, message)) => {
            set_curl_error(&handle, code, message.clone());
            record_curl_diagnostic(
                context,
                &handle,
                CurlDiagnostic::new(
                    "E_PHP_CURL_REQUEST_FAILED",
                    "curl_exec",
                    "execute_request",
                    "enabled",
                    code,
                    message,
                ),
                span,
            );
            return Ok(Value::Bool(false));
        }
    };
    if curl_bool_property(&handle, "__curl_failonerror") && response.status >= 400 {
        set_curl_error(&handle, 22, "HTTP response code said error");
        record_curl_diagnostic(
            context,
            &handle,
            CurlDiagnostic::new(
                "E_PHP_CURL_REQUEST_FAILED",
                "curl_exec",
                "http_status",
                "enabled",
                22,
                "HTTP response code said error",
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    set_curl_error(&handle, 0, "");
    handle.set_property("__curl_http_code", Value::Int(i64::from(response.status)));
    handle.set_property(
        "__curl_effective_url",
        Value::String(PhpString::from(response.effective_url.into_bytes())),
    );
    handle.set_property(
        "__curl_header_size",
        Value::Int(response.header_size as i64),
    );
    handle.set_property(
        "__curl_total_time",
        Value::Float(FloatValue::from_f64(start.elapsed().as_secs_f64())),
    );
    let response_headers = response.headers;
    let response_body = response.body;
    handle.set_property(
        "__curl_last_response_headers",
        Value::String(PhpString::from(response_headers.clone())),
    );
    handle.set_property(
        "__curl_last_response_body",
        Value::String(PhpString::from(response_body.clone())),
    );
    curl_apply_response_callbacks(&handle, &response_headers, &response_body);

    let body = if curl_bool_property(&handle, "__curl_header") {
        let mut bytes = response_headers;
        bytes.extend_from_slice(&response_body);
        bytes
    } else {
        response_body
    };
    if curl_bool_property(&handle, "__curl_returntransfer") {
        Ok(Value::string(body))
    } else {
        context.output().write_bytes(&body);
        Ok(Value::Bool(true))
    }
}

fn curl_network_requests_enabled(context: &BuiltinContext<'_>) -> bool {
    if context.network_requests_enabled() {
        return true;
    }

    #[cfg(test)]
    if let Some(enabled) = *NET_TESTS_OVERRIDE
        .lock()
        .expect("network test override lock")
    {
        return enabled;
    }

    std::env::var(PHRUST_NET_TESTS_ENV).as_deref() == Ok("1")
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
            CURLINFO_HEADER_SIZE => curl_int_property(&handle, "__curl_header_size"),
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
        ArrayKey::String(PhpString::from("header_size")),
        curl_int_property(&handle, "__curl_header_size"),
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

pub(in crate::builtins::modules) fn builtin_curl_reset(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_reset", &args, 1)?;
    let handle = curl_handle_arg("curl_reset", args.first())?;
    reset_curl_handle(&handle);
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_curl_copy_handle(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_copy_handle", &args, 1)?;
    let handle = curl_handle_arg("curl_copy_handle", args.first())?;
    let copy = curl_handle_object();
    for property in [
        "__curl_url",
        "__curl_returntransfer",
        "__curl_timeout",
        "__curl_timeout_ms",
        "__curl_followlocation",
        "__curl_header",
        "__curl_nobody",
        "__curl_useragent",
        "__curl_referer",
        "__curl_encoding",
        "__curl_http_version",
        "__curl_connecttimeout",
        "__curl_connecttimeout_ms",
        "__curl_maxredirs",
        "__curl_failonerror",
        "__curl_httpheader",
        "__curl_headerfunction",
        "__curl_writefunction",
        "__curl_buffersize",
        "__curl_cainfo",
        "__curl_httpauth",
        "__curl_protocols",
        "__curl_proxy",
        "__curl_proxyauth",
        "__curl_proxyport",
        "__curl_proxytype",
        "__curl_proxyuserpwd",
        "__curl_redir_protocols",
        "__curl_userpwd",
        "__curl_post",
        "__curl_postfields",
        "__curl_customrequest",
        "__curl_ssl_verifypeer",
        "__curl_ssl_verifyhost",
    ] {
        if let Some(value) = handle.get_property(property) {
            copy.set_property(property, value);
        }
    }
    Ok(Value::Object(copy))
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
    if !curl_host_is_loopback(&parsed.host) {
        return Err((
            7,
            "cURL MVP only permits local loopback hosts when network tests are enabled".to_owned(),
        ));
    }
    let (body, content_type) = curl_post_body(handle)?;
    let post = curl_bool_property(handle, "__curl_post") || !body.is_empty();
    let method = match handle.get_property("__curl_customrequest") {
        Some(Value::String(value)) if !value.is_empty() => value.to_string_lossy(),
        _ if curl_bool_property(handle, "__curl_nobody") => "HEAD".to_owned(),
        _ if post => "POST".to_owned(),
        _ => "GET".to_owned(),
    };
    let mut headers = curl_header_lines(handle);
    if let Some(Value::String(value)) = handle.get_property("__curl_useragent")
        && !value.is_empty()
    {
        headers.push(format!("User-Agent: {}", value.to_string_lossy()));
    }
    if let Some(Value::String(value)) = handle.get_property("__curl_referer")
        && !value.is_empty()
    {
        headers.push(format!("Referer: {}", value.to_string_lossy()));
    }
    if let Some(Value::String(value)) = handle.get_property("__curl_encoding")
        && !value.is_empty()
    {
        headers.push(format!("Accept-Encoding: {}", value.to_string_lossy()));
    }
    if let Some(content_type) = content_type
        && !headers
            .iter()
            .any(|header| header.to_ascii_lowercase().starts_with("content-type:"))
    {
        headers.push(format!("Content-Type: {content_type}"));
    }
    Ok(CurlRequest {
        url,
        host: parsed.host,
        port: parsed.port,
        path: parsed.path,
        method,
        headers,
        body,
        connect_timeout: curl_connect_timeout(handle),
        timeout: curl_timeout(handle).max(Duration::from_secs(120)),
        follow_redirects: curl_bool_property(handle, "__curl_followlocation"),
        max_redirects: curl_int_setting(handle, "__curl_maxredirs", 5).clamp(0, 20) as usize,
    })
}

fn curl_handle_targets_loopback(handle: &ObjectRef) -> bool {
    let Some(Value::String(url)) = handle.get_property("__curl_url") else {
        return false;
    };
    parse_http_url(&url.to_string_lossy())
        .map(|parsed| curl_host_is_loopback(&parsed.host))
        .unwrap_or(false)
}

fn curl_host_is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn execute_http_request(request: &CurlRequest) -> Result<CurlResponse, (i64, String)> {
    let mut request = request.clone();
    for _ in 0..=request.max_redirects {
        let response = execute_single_http_request(&request)?;
        if request.follow_redirects
            && matches!(response.status, 301 | 302 | 303 | 307 | 308)
            && let Some(location) = &response.location
        {
            request = request.redirect(location)?;
            continue;
        }
        return Ok(response);
    }
    Err((47, "cURL redirect limit exceeded".to_owned()))
}

fn execute_single_http_request(request: &CurlRequest) -> Result<CurlResponse, (i64, String)> {
    let mut addrs = (request.host.as_str(), request.port)
        .to_socket_addrs()
        .map_err(|error| (7, format!("failed to resolve local cURL host: {error}")))?;
    let Some(addr) = addrs.next() else {
        return Err((7, "failed to resolve local cURL host".to_owned()));
    };
    let mut stream = TcpStream::connect_timeout(&addr, request.connect_timeout)
        .map_err(|error| (7, format!("failed to connect local cURL host: {error}")))?;
    stream
        .set_read_timeout(Some(curl_read_poll_timeout(request.timeout)))
        .map_err(|error| (28, format!("failed to set cURL read timeout: {error}")))?;
    stream
        .set_write_timeout(Some(request.timeout))
        .map_err(|error| (28, format!("failed to set cURL write timeout: {error}")))?;
    let mut payload = Vec::new();
    write!(
        payload,
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
        request.method,
        request.path,
        request_host_header(&request.host, request.port)
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
    let bytes = read_http_response(&mut stream, &request.method, request.timeout)?;
    let mut response = parse_http_response(&bytes)?;
    response.effective_url = request.url.clone();
    Ok(response)
}

fn read_http_response(
    stream: &mut TcpStream,
    method: &str,
    timeout: Duration,
) -> Result<Vec<u8>, (i64, String)> {
    let start = Instant::now();
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(read) => {
                bytes.extend_from_slice(&buffer[..read]);
                if http_response_complete(&bytes, method)? {
                    return Ok(bytes);
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if http_response_complete(&bytes, method)? {
                    return Ok(bytes);
                }
                if start.elapsed() >= timeout {
                    return Err((28, "cURL operation timed out".to_owned()));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err((56, format!("failed to read cURL response: {error}"))),
        }
    }
}

fn http_response_complete(bytes: &[u8], method: &str) -> Result<bool, (i64, String)> {
    let Some(header_end) = response_header_end(bytes) else {
        return Ok(false);
    };
    let header = String::from_utf8_lossy(&bytes[..header_end]);
    let status = response_status(&header)?;
    if method.eq_ignore_ascii_case("HEAD") || matches!(status, 100..=199 | 204 | 304) {
        return Ok(true);
    }
    if header_has_chunked_transfer_encoding(&header) {
        return Ok(chunked_response_complete(&bytes[header_end + 4..]));
    }
    let Some(content_length) = response_content_length(&header) else {
        return Ok(false);
    };
    Ok(bytes.len() >= header_end + 4 + content_length)
}

fn response_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_http_response(bytes: &[u8]) -> Result<CurlResponse, (i64, String)> {
    let header_end =
        response_header_end(bytes).ok_or_else(|| (56, "invalid HTTP response".to_owned()))?;
    let headers = bytes[..header_end + 4].to_vec();
    let header = String::from_utf8_lossy(&bytes[..header_end]);
    let status = response_status(&header)?;
    let location = header.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("location")
            .then(|| value.trim().to_owned())
    });
    Ok(CurlResponse {
        status,
        effective_url: String::new(),
        header_size: headers.len(),
        headers,
        body: bytes[header_end + 4..].to_vec(),
        location,
    })
}

fn request_host_header(host: &str, port: u16) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    if port == 80 {
        host
    } else {
        format!("{host}:{port}")
    }
}

fn response_status(header: &str) -> Result<u16, (i64, String)> {
    header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| (56, "invalid HTTP status line".to_owned()))
}

fn response_content_length(header: &str) -> Option<usize> {
    header.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })
}

fn header_has_chunked_transfer_encoding(header: &str) -> bool {
    header.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
    })
}

fn chunked_response_complete(body: &[u8]) -> bool {
    let mut offset = 0;
    loop {
        let Some(line_end) = find_crlf(&body[offset..]) else {
            return false;
        };
        let size_line = &body[offset..offset + line_end];
        let size_text = String::from_utf8_lossy(size_line);
        let Some(size) = size_text
            .split(';')
            .next()
            .and_then(|value| usize::from_str_radix(value.trim(), 16).ok())
        else {
            return false;
        };
        offset += line_end + 2;
        if size == 0 {
            return body[offset..]
                .windows(4)
                .any(|window| window == b"\r\n\r\n")
                || body.get(offset..offset + 2) == Some(b"\r\n");
        }
        let next_offset = offset.saturating_add(size).saturating_add(2);
        if body.len() < next_offset || body.get(offset + size..next_offset) != Some(b"\r\n") {
            return false;
        }
        offset = next_offset;
    }
}

fn find_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|window| window == b"\r\n")
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

fn curl_post_body(handle: &ObjectRef) -> Result<CurlPostBody, CurlTransportError> {
    match handle.get_property("__curl_postfields") {
        Some(Value::String(value)) => Ok((value.as_bytes().to_vec(), None)),
        Some(Value::Array(array)) => Ok((
            form_encode_array(&array).into_bytes(),
            Some("application/x-www-form-urlencoded"),
        )),
        Some(value) => string_arg("curl_exec", &value)
            .map(|value| (value.as_bytes().to_vec(), None))
            .map_err(|error| (43, error.message().to_owned())),
        None => Ok((Vec::new(), None)),
    }
}

fn curl_apply_response_callbacks(handle: &ObjectRef, headers: &[u8], body: &[u8]) {
    if let Some(target) = curl_callback_target(handle, "__curl_headerfunction", "stream_headers") {
        target.set_property("headers", Value::String(PhpString::from(headers.to_vec())));
        target.set_property("done_headers", Value::Bool(true));
    }

    if let Some(target) = curl_callback_target(handle, "__curl_writefunction", "stream_body") {
        let previous = match target.get_property("response_data") {
            Some(Value::String(value)) => value.as_bytes().to_vec(),
            _ => Vec::new(),
        };
        let existing_bytes = match target.get_property("response_bytes") {
            Some(Value::Int(value)) if value > 0 => value as usize,
            _ => 0,
        };
        let limit = match target.get_property("response_byte_limit") {
            Some(Value::Int(value)) if value >= 0 => Some(value as usize),
            _ => None,
        };
        let allowed = limit.map_or(body.len(), |limit| limit.saturating_sub(existing_bytes));
        let kept = body.len().min(allowed);
        let mut response = previous;
        response.extend_from_slice(&body[..kept]);
        target.set_property("response_data", Value::String(PhpString::from(response)));
        target.set_property(
            "response_bytes",
            Value::Int(existing_bytes.saturating_add(kept) as i64),
        );
    }
}

fn curl_callback_target(handle: &ObjectRef, property: &str, method: &str) -> Option<ObjectRef> {
    let Value::Array(callback) = handle.get_property(property)? else {
        return None;
    };
    let Some(Value::Object(target)) = callback.get(&ArrayKey::Int(0)) else {
        return None;
    };
    let Some(Value::String(callback_method)) = callback.get(&ArrayKey::Int(1)) else {
        return None;
    };
    callback_method
        .to_string_lossy()
        .eq_ignore_ascii_case(method)
        .then_some(target.clone())
}

fn form_encode_array(array: &PhpArray) -> String {
    let mut fields = Vec::new();
    for (key, value) in array.iter() {
        let key = match key {
            ArrayKey::Int(value) => value.to_string(),
            ArrayKey::String(value) => value.to_string_lossy(),
        };
        let value = match value {
            Value::Array(_) => "Array".to_owned(),
            other => string_arg("curl_exec", other)
                .map(|value| value.to_string_lossy())
                .unwrap_or_default(),
        };
        fields.push(format!(
            "{}={}",
            percent_encode_form(&key),
            percent_encode_form(&value)
        ));
    }
    fields.join("&")
}

fn percent_encode_form(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_encode_uri_component(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &byte in bytes {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte),
            _ => out.extend_from_slice(format!("%{byte:02X}").as_bytes()),
        }
    }
    out
}

fn percent_decode_uri_component(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let (Some(&hi), Some(&lo)) = (bytes.get(index + 1), bytes.get(index + 2))
            && let (Some(hi), Some(lo)) = (hex_value(hi), hex_value(lo))
        {
            out.push((hi << 4) | lo);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn curl_connect_timeout(handle: &ObjectRef) -> Duration {
    if let Some(ms) = curl_duration_millis_setting(handle, "__curl_connecttimeout_ms") {
        return clamp_duration_millis(ms, 1, 30_000);
    }
    if let Some(seconds) = curl_duration_seconds_setting(handle, "__curl_connecttimeout") {
        return clamp_duration_millis(seconds * 1000.0, 1, 30_000);
    }
    Duration::from_secs(10)
}

fn curl_timeout(handle: &ObjectRef) -> Duration {
    if let Some(ms) = curl_duration_millis_setting(handle, "__curl_timeout_ms") {
        return clamp_duration_millis(ms, 1, 300_000);
    }
    if let Some(seconds) = curl_duration_seconds_setting(handle, "__curl_timeout") {
        return clamp_duration_millis(seconds * 1000.0, 1, 300_000);
    }
    Duration::from_secs(60)
}

fn curl_read_poll_timeout(timeout: Duration) -> Duration {
    timeout
        .min(Duration::from_millis(250))
        .max(Duration::from_millis(1))
}

fn curl_duration_seconds_setting(handle: &ObjectRef, name: &str) -> Option<f64> {
    curl_duration_numeric_setting(handle, name)
}

fn curl_duration_millis_setting(handle: &ObjectRef, name: &str) -> Option<f64> {
    curl_duration_numeric_setting(handle, name)
}

fn curl_duration_numeric_setting(handle: &ObjectRef, name: &str) -> Option<f64> {
    let value = handle.get_property(name)?;
    match value {
        Value::Int(value) => Some(value as f64),
        Value::Float(value) => Some(value.to_f64()),
        value => crate::convert::to_float(&value).ok(),
    }
}

fn clamp_duration_millis(value: f64, min: u64, max: u64) -> Duration {
    let millis = if value.is_finite() {
        value.ceil() as i128
    } else {
        i128::from(max)
    };
    Duration::from_millis(millis.clamp(i128::from(min), i128::from(max)) as u64)
}

fn curl_handle_object() -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&curl_runtime_class("CurlHandle"), "CurlHandle");
    reset_curl_handle(&object);
    object
}

fn reset_curl_handle(object: &ObjectRef) {
    object.set_property("__curl_errno", Value::Int(0));
    object.set_property("__curl_error", Value::String(PhpString::from("")));
    object.set_property("__curl_returntransfer", Value::Bool(false));
    object.set_property("__curl_http_code", Value::Int(0));
    object.set_property("__curl_effective_url", Value::String(PhpString::from("")));
    object.set_property("__curl_header_size", Value::Int(0));
    object.set_property("__curl_total_time", Value::Float(FloatValue::from_f64(0.0)));
    object.set_property(
        "__curl_last_response_headers",
        Value::String(PhpString::from("")),
    );
    object.set_property(
        "__curl_last_response_body",
        Value::String(PhpString::from("")),
    );
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

fn curl_multi_handle_arg(name: &str, value: Option<&Value>) -> Result<ObjectRef, BuiltinError> {
    match value {
        Some(Value::Object(object)) if object.class_name() == "curlmultihandle" => {
            Ok(object.clone())
        }
        Some(value) => Err(argument_type_error(name, "1", "CurlMultiHandle", value)),
        None => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            format!("builtin {name} expects CurlMultiHandle argument"),
        )),
    }
}

fn set_curl_option(handle: &ObjectRef, option: i64, value: Value) -> BuiltinResult {
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
        CURLOPT_HEADER => "__curl_header",
        CURLOPT_NOBODY => "__curl_nobody",
        CURLOPT_USERAGENT => "__curl_useragent",
        CURLOPT_REFERER => "__curl_referer",
        CURLOPT_ENCODING => "__curl_encoding",
        CURLOPT_HTTP_VERSION => "__curl_http_version",
        CURLOPT_CONNECTTIMEOUT => "__curl_connecttimeout",
        CURLOPT_CONNECTTIMEOUT_MS => "__curl_connecttimeout_ms",
        CURLOPT_MAXREDIRS => "__curl_maxredirs",
        CURLOPT_FAILONERROR => "__curl_failonerror",
        CURLOPT_HTTPHEADER => "__curl_httpheader",
        CURLOPT_HEADERFUNCTION => "__curl_headerfunction",
        CURLOPT_WRITEFUNCTION => "__curl_writefunction",
        CURLOPT_BUFFERSIZE => "__curl_buffersize",
        CURLOPT_CAINFO => "__curl_cainfo",
        CURLOPT_HTTPAUTH => "__curl_httpauth",
        CURLOPT_PROTOCOLS => "__curl_protocols",
        CURLOPT_PROXY => "__curl_proxy",
        CURLOPT_PROXYAUTH => "__curl_proxyauth",
        CURLOPT_PROXYPORT => "__curl_proxyport",
        CURLOPT_PROXYTYPE => "__curl_proxytype",
        CURLOPT_PROXYUSERPWD => "__curl_proxyuserpwd",
        CURLOPT_REDIR_PROTOCOLS => "__curl_redir_protocols",
        CURLOPT_USERPWD => "__curl_userpwd",
        CURLOPT_POST => "__curl_post",
        CURLOPT_POSTFIELDS => "__curl_postfields",
        CURLOPT_CUSTOMREQUEST => "__curl_customrequest",
        CURLOPT_SSL_VERIFYPEER => "__curl_ssl_verifypeer",
        CURLOPT_SSL_VERIFYHOST => "__curl_ssl_verifyhost",
        _ => {
            set_curl_error(handle, 48, "unsupported cURL option");
            return Ok(Value::Bool(false));
        }
    };
    handle.set_property(property, value);
    Ok(Value::Bool(true))
}

fn set_curl_error(handle: &ObjectRef, errno: i64, error: impl Into<String>) {
    handle.set_property("__curl_errno", Value::Int(errno));
    handle.set_property(
        "__curl_error",
        Value::String(PhpString::from(error.into().into_bytes())),
    );
}

struct CurlDiagnostic {
    diagnostic_id: &'static str,
    function_name: &'static str,
    operation: &'static str,
    capability_state: &'static str,
    error_code: i64,
    error_message: String,
}

impl CurlDiagnostic {
    fn new(
        diagnostic_id: &'static str,
        function_name: &'static str,
        operation: &'static str,
        capability_state: &'static str,
        error_code: i64,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            diagnostic_id,
            function_name,
            operation,
            capability_state,
            error_code,
            error_message: error_message.into(),
        }
    }
}

fn record_curl_diagnostic(
    context: &mut BuiltinContext<'_>,
    handle: &ObjectRef,
    diagnostic: CurlDiagnostic,
    span: RuntimeSourceSpan,
) {
    let (host, port) = curl_diagnostic_endpoint(handle);
    let error_message = diagnostic
        .error_message
        .chars()
        .take(512)
        .collect::<String>();
    let payload = WordPressDiagnosticContext::new("db_network")
        .with_field("diagnostic_id", diagnostic.diagnostic_id)
        .with_field("function_name", diagnostic.function_name)
        .with_field("operation", diagnostic.operation)
        .with_field("capability_state", diagnostic.capability_state)
        .with_field("dsn_present_boolean", "false")
        .with_field("host", host)
        .with_field(
            "port",
            port.map(|port| port.to_string()).unwrap_or_default(),
        )
        .with_field("database_name_if_nonsecret", "")
        .with_field("mysql_error_code", diagnostic.error_code.to_string())
        .with_field("mysql_sqlstate", "")
        .with_field("mysql_error_message", error_message.clone())
        .with_field("curl_error_code", diagnostic.error_code.to_string());
    context.record_diagnostic(
        RuntimeDiagnostic::new(
            diagnostic.diagnostic_id,
            RuntimeSeverity::Warning,
            error_message,
            span,
            Vec::new(),
            Some(crate::PhpReferenceClassification::Warning),
        )
        .with_diagnostic_payload(RuntimeDiagnosticPayload::WordPressBringup(payload)),
    );
}

fn curl_diagnostic_endpoint(handle: &ObjectRef) -> (String, Option<u16>) {
    let Some(Value::String(url)) = handle.get_property("__curl_url") else {
        return (String::new(), None);
    };
    let url = url.to_string_lossy();
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(&url);
    let authority = rest.split('/').next().unwrap_or_default();
    let (host, port) = authority
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, Some(port))))
        .unwrap_or((authority, None));
    (host.trim_matches(['[', ']']).to_owned(), port)
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

fn curl_int_setting(handle: &ObjectRef, name: &str, default: i64) -> i64 {
    match handle.get_property(name) {
        Some(Value::Int(value)) => value,
        Some(value) => crate::convert::to_int(&value).unwrap_or(default),
        None => default,
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

#[derive(Clone)]
struct CurlRequest {
    url: String,
    host: String,
    port: u16,
    path: String,
    method: String,
    headers: Vec<String>,
    body: Vec<u8>,
    connect_timeout: Duration,
    timeout: Duration,
    follow_redirects: bool,
    max_redirects: usize,
}

impl CurlRequest {
    fn redirect(&self, location: &str) -> Result<Self, (i64, String)> {
        let url = if location.starts_with("http://") || location.starts_with("https://") {
            location.to_owned()
        } else if location.starts_with('/') {
            format!("http://{}:{}{}", self.host, self.port, location)
        } else {
            let base = self
                .path
                .rsplit_once('/')
                .map_or("/", |(base, _)| if base.is_empty() { "/" } else { base });
            format!(
                "http://{}:{}/{}{}",
                self.host,
                self.port,
                base.trim_end_matches('/'),
                location
            )
        };
        let parsed = parse_http_url(&url)?;
        let mut next = self.clone();
        next.url = url;
        next.host = parsed.host;
        next.port = parsed.port;
        next.path = parsed.path;
        Ok(next)
    }
}

struct CurlResponse {
    status: u16,
    effective_url: String,
    header_size: usize,
    headers: Vec<u8>,
    body: Vec<u8>,
    location: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener};
    use std::thread;

    static NET_TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn curl_exec_is_network_disabled_by_default() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(false);
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string("http://example.com/")],
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
        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), "E_PHP_CURL_CAPABILITY_DISABLED");
        let Some(RuntimeDiagnosticPayload::WordPressBringup(payload)) = diagnostics[0].payload()
        else {
            panic!("expected db/network diagnostic payload");
        };
        assert_eq!(
            payload.fields().get("diagnostic_id").map(String::as_str),
            Some("E_PHP_CURL_CAPABILITY_DISABLED")
        );
        assert_eq!(
            payload.fields().get("capability_state").map(String::as_str),
            Some("disabled")
        );
        assert_eq!(
            payload
                .fields()
                .get("dsn_present_boolean")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn curl_exec_allows_loopback_when_network_is_disabled() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(false);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).expect("read request");
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("GET /site-health"));
            assert!(request.contains(&format!("Host: 127.0.0.1:{port}\r\n")));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                .expect("write response");
        });

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("http://127.0.0.1:{port}/site-health"))],
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
            builtin_curl_errno(&mut context, vec![handle], RuntimeSourceSpan::default())
                .expect("errno"),
            Value::Int(0)
        );
        assert!(context.take_diagnostics().is_empty());
        server.join().expect("server");
    }

    #[test]
    fn curl_exec_gets_local_http_when_net_tests_are_enabled() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(true);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).expect("read request");
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("GET /wp-json"));
            assert!(request.contains(&format!("Host: 127.0.0.1:{port}\r\n")));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                .expect("write response");
        });

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
        server.join().expect("server");
    }

    #[test]
    fn curl_exec_stops_after_complete_content_length_response() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(true);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).expect("read request");
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /keepalive"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                .expect("write response");
            thread::sleep(Duration::from_millis(500));
        });

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("http://127.0.0.1:{port}/keepalive"))],
            RuntimeSourceSpan::default(),
        )
        .expect("init");
        for (option, value) in [
            (CURLOPT_RETURNTRANSFER, Value::Bool(true)),
            (CURLOPT_TIMEOUT_MS, Value::Int(100)),
        ] {
            builtin_curl_setopt(
                &mut context,
                vec![handle.clone(), Value::Int(option), value],
                RuntimeSourceSpan::default(),
            )
            .expect("setopt");
        }

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
            builtin_curl_errno(&mut context, vec![handle], RuntimeSourceSpan::default())
                .expect("errno"),
            Value::Int(0)
        );
        server.join().expect("server");
    }

    #[test]
    fn curl_exec_uses_request_timeout_for_delayed_response_reads() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(true);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).expect("read request");
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /delayed"));
            thread::sleep(Duration::from_millis(150));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nLATE")
                .expect("write response");
        });

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("http://127.0.0.1:{port}/delayed"))],
            RuntimeSourceSpan::default(),
        )
        .expect("init");
        for (option, value) in [
            (CURLOPT_RETURNTRANSFER, Value::Bool(true)),
            (CURLOPT_CONNECTTIMEOUT_MS, Value::Int(50)),
            (CURLOPT_TIMEOUT_MS, Value::Int(1_000)),
        ] {
            builtin_curl_setopt(
                &mut context,
                vec![handle.clone(), Value::Int(option), value],
                RuntimeSourceSpan::default(),
            )
            .expect("setopt");
        }

        assert_eq!(
            builtin_curl_exec(
                &mut context,
                vec![handle.clone()],
                RuntimeSourceSpan::default()
            )
            .expect("exec"),
            Value::string("LATE")
        );
        assert_eq!(
            builtin_curl_errno(&mut context, vec![handle], RuntimeSourceSpan::default())
                .expect("errno"),
            Value::Int(0)
        );
        server.join().expect("server");
    }

    #[test]
    fn curl_exec_populates_requests_transport_callback_fields() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(true);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).expect("read request");
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /callbacks"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                .expect("write response");
        });

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("http://127.0.0.1:{port}/callbacks"))],
            RuntimeSourceSpan::default(),
        )
        .expect("init");
        let transport = ObjectRef::new_with_display_name(
            &curl_runtime_class("WpOrg\\Requests\\Transport\\Curl"),
            "WpOrg\\Requests\\Transport\\Curl",
        );
        transport.set_property("headers", Value::string(""));
        transport.set_property("response_data", Value::string(""));
        transport.set_property("response_bytes", Value::Int(0));
        transport.set_property("response_byte_limit", Value::Bool(false));

        for (option, value) in [
            (CURLOPT_RETURNTRANSFER, Value::Bool(true)),
            (
                CURLOPT_HEADERFUNCTION,
                Value::packed_array(vec![
                    Value::Object(transport.clone()),
                    Value::string("stream_headers"),
                ]),
            ),
            (
                CURLOPT_WRITEFUNCTION,
                Value::packed_array(vec![
                    Value::Object(transport.clone()),
                    Value::string("stream_body"),
                ]),
            ),
        ] {
            builtin_curl_setopt(
                &mut context,
                vec![handle.clone(), Value::Int(option), value],
                RuntimeSourceSpan::default(),
            )
            .expect("setopt");
        }

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
            transport.get_property("response_data"),
            Some(Value::string("OK"))
        );
        assert_eq!(
            transport.get_property("response_bytes"),
            Some(Value::Int(2))
        );
        let Some(Value::String(headers)) = transport.get_property("headers") else {
            panic!("headers should be populated");
        };
        assert!(headers.to_string_lossy().starts_with("HTTP/1.1 200 OK"));
        server.join().expect("server");
    }

    #[test]
    fn curl_exec_handles_headers_post_arrays_redirects_and_response_headers() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(true);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept redirect");
            let mut request = [0_u8; 2048];
            let read = stream.read(&mut request).expect("read redirect request");
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("POST /start"));
            stream
                .write_all(b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /submit\r\nContent-Length: 0\r\n\r\n")
                .expect("write redirect");
            stream.shutdown(Shutdown::Write).expect("shutdown redirect");

            let (mut stream, _) = listener.accept().expect("accept final");
            let mut request = [0_u8; 2048];
            let read = stream.read(&mut request).expect("read final request");
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("POST /submit"));
            assert!(request.contains("X-Test: yes"));
            assert!(request.contains("Content-Type: application/x-www-form-urlencoded"));
            assert!(request.ends_with("name=alpha+beta&qty=3"));
            stream
                .write_all(b"HTTP/1.1 201 Created\r\nX-Reply: ok\r\nContent-Length: 2\r\n\r\nOK")
                .expect("write final");
            stream.shutdown(Shutdown::Write).expect("shutdown final");
        });

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("http://127.0.0.1:{port}/start"))],
            RuntimeSourceSpan::default(),
        )
        .expect("init");
        for (option, value) in [
            (CURLOPT_RETURNTRANSFER, Value::Bool(true)),
            (CURLOPT_FOLLOWLOCATION, Value::Bool(true)),
            (CURLOPT_HEADER, Value::Bool(true)),
            (
                CURLOPT_HTTPHEADER,
                Value::packed_array(vec![Value::string("X-Test: yes")]),
            ),
            (CURLOPT_HEADERFUNCTION, Value::Null),
            (CURLOPT_WRITEFUNCTION, Value::Null),
            (CURLOPT_BUFFERSIZE, Value::Int(16_384)),
            (CURLOPT_PROTOCOLS, Value::Int(3)),
            (CURLOPT_REDIR_PROTOCOLS, Value::Int(3)),
        ] {
            builtin_curl_setopt(
                &mut context,
                vec![handle.clone(), Value::Int(option), value],
                RuntimeSourceSpan::default(),
            )
            .expect("setopt");
        }
        let mut fields = PhpArray::new();
        fields.insert(
            ArrayKey::String(PhpString::from("name")),
            Value::string("alpha beta"),
        );
        fields.insert(ArrayKey::String(PhpString::from("qty")), Value::Int(3));
        builtin_curl_setopt(
            &mut context,
            vec![
                handle.clone(),
                Value::Int(CURLOPT_POSTFIELDS),
                Value::Array(fields),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("postfields");

        let Value::String(response) = builtin_curl_exec(
            &mut context,
            vec![handle.clone()],
            RuntimeSourceSpan::default(),
        )
        .expect("exec") else {
            panic!(
                "expected response string, errno={:?}, error={:?}",
                builtin_curl_errno(
                    &mut context,
                    vec![handle.clone()],
                    RuntimeSourceSpan::default()
                ),
                builtin_curl_error(
                    &mut context,
                    vec![handle.clone()],
                    RuntimeSourceSpan::default()
                )
            );
        };
        let response = response.to_string_lossy();
        assert!(response.starts_with("HTTP/1.1 201 Created"));
        assert!(response.ends_with("OK"));
        assert_eq!(
            builtin_curl_getinfo(
                &mut context,
                vec![handle.clone(), Value::Int(CURLINFO_RESPONSE_CODE)],
                RuntimeSourceSpan::default(),
            )
            .expect("status"),
            Value::Int(201)
        );
        assert!(matches!(
            builtin_curl_getinfo(
                &mut context,
                vec![handle, Value::Int(CURLINFO_HEADER_SIZE)],
                RuntimeSourceSpan::default(),
            )
            .expect("header size"),
            Value::Int(size) if size > 0
        ));
        server.join().expect("server");
    }

    struct NetTestsOverride {
        previous: Option<bool>,
    }

    impl NetTestsOverride {
        fn set(enabled: bool) -> Self {
            let mut override_value = NET_TESTS_OVERRIDE
                .lock()
                .expect("network test override lock");
            let previous = *override_value;
            *override_value = Some(enabled);
            Self { previous }
        }
    }

    impl Drop for NetTestsOverride {
        fn drop(&mut self) {
            *NET_TESTS_OVERRIDE
                .lock()
                .expect("network test override lock") = self.previous;
        }
    }
}
