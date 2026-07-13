//! cURL-compatible HTTP client builtin slice.

use super::core::{
    argument_type_error, argument_value_error, expect_arity, float_arg, int_arg, string_arg,
    string_array_key,
};
use crate::builtins::context::CurlBuiltinServices;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    CurlEasyCollector, CurlMultiDone, CurlMultiRuntimeState, CurlMultiTransferState,
    RuntimeSourceSpan,
};
use crate::{
    ArrayKey, ClassEntry, ClassFlags, FloatValue, ObjectRef, PhpArray, PhpString,
    RuntimeBringupDiagnosticContext, RuntimeDiagnostic, RuntimeDiagnosticPayload, RuntimeSeverity,
    Value, normalize_class_name,
};
use curl::MultiError;
use curl::Version;
use curl::easy::{
    Auth, Easy2, HttpVersion, IpResolve, List, PostRedirections, ProxyType, SslVersion,
};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

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
        "curl_strerror",
        builtin_curl_strerror,
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
        "curl_multi_select",
        builtin_curl_multi_select,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_multi_info_read",
        builtin_curl_multi_info_read,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_multi_remove_handle",
        builtin_curl_multi_remove_handle,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_multi_close",
        builtin_curl_multi_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_share_init",
        builtin_curl_share_init,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_share_setopt",
        builtin_curl_share_setopt,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_share_errno",
        builtin_curl_share_errno,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_share_strerror",
        builtin_curl_share_strerror,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "curl_share_close",
        builtin_curl_share_close,
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

macro_rules! curl_builtin_adapter {
    ($entry:ident => $implementation:ident) => {
        pub(in crate::builtins::modules) fn $entry(
            context: &mut BuiltinContext<'_>,
            args: Vec<Value>,
            span: RuntimeSourceSpan,
        ) -> BuiltinResult {
            let mut services = context.curl_services();
            $implementation(&mut services, args, span)
        }
    };
}

curl_builtin_adapter!(builtin_curl_multi_init => curl_multi_init);
curl_builtin_adapter!(builtin_curl_multi_add_handle => curl_multi_add_handle);
curl_builtin_adapter!(builtin_curl_multi_exec => curl_multi_exec);
curl_builtin_adapter!(builtin_curl_multi_select => curl_multi_select);
curl_builtin_adapter!(builtin_curl_multi_info_read => curl_multi_info_read);
curl_builtin_adapter!(builtin_curl_multi_remove_handle => curl_multi_remove_handle);
curl_builtin_adapter!(builtin_curl_multi_close => curl_multi_close);
curl_builtin_adapter!(builtin_curl_share_init => curl_share_init);
curl_builtin_adapter!(builtin_curl_share_setopt => curl_share_setopt);
curl_builtin_adapter!(builtin_curl_share_close => curl_share_close);
curl_builtin_adapter!(builtin_curl_init => curl_init);
curl_builtin_adapter!(builtin_curl_setopt => curl_setopt);
curl_builtin_adapter!(builtin_curl_setopt_array => curl_setopt_array);
curl_builtin_adapter!(builtin_curl_exec => curl_exec);
curl_builtin_adapter!(builtin_curl_close => curl_close);
curl_builtin_adapter!(builtin_curl_reset => curl_reset);
curl_builtin_adapter!(builtin_curl_copy_handle => curl_copy_handle);

pub const PHRUST_NET_TESTS_ENV: &str = "PHRUST_NET_TESTS";
static NET_TESTS_OVERRIDE: Mutex<Option<bool>> = Mutex::new(None);

#[doc(hidden)]
pub struct CurlNetworkTestOverride {
    previous: Option<bool>,
}

#[doc(hidden)]
pub fn set_curl_network_tests_override_for_tests(enabled: bool) -> CurlNetworkTestOverride {
    let mut override_value = NET_TESTS_OVERRIDE
        .lock()
        .expect("network test override lock");
    let previous = *override_value;
    *override_value = Some(enabled);
    CurlNetworkTestOverride { previous }
}

impl Drop for CurlNetworkTestOverride {
    fn drop(&mut self) {
        *NET_TESTS_OVERRIDE
            .lock()
            .expect("network test override lock") = self.previous;
    }
}

const CURLOPT_URL: i64 = 10002;
const CURLOPT_RETURNTRANSFER: i64 = 19913;
const CURLOPT_TIMEOUT: i64 = 13;
const CURLOPT_TIMEOUT_MS: i64 = 155;
const CURLOPT_FOLLOWLOCATION: i64 = 52;
const CURLOPT_HEADER: i64 = 42;
const CURLOPT_NOBODY: i64 = 44;
const CURLOPT_USERAGENT: i64 = 10018;
const CURLOPT_REFERER: i64 = 10016;
const CURLOPT_ACCEPT_ENCODING: i64 = 10102;
const CURLOPT_ENCODING: i64 = 10102;
const CURLOPT_HTTP_VERSION: i64 = 84;
const CURLOPT_CONNECTTIMEOUT: i64 = 78;
const CURLOPT_CONNECTTIMEOUT_MS: i64 = 156;
const CURLOPT_MAXREDIRS: i64 = 68;
const CURLOPT_FAILONERROR: i64 = 45;
const CURLOPT_AUTOREFERER: i64 = 58;
const CURLOPT_COOKIE: i64 = 10022;
const CURLOPT_COOKIEFILE: i64 = 10031;
const CURLOPT_COOKIEJAR: i64 = 10082;
const CURLOPT_COOKIESESSION: i64 = 96;
const CURLOPT_DNS_CACHE_TIMEOUT: i64 = 92;
const CURLOPT_HTTPHEADER: i64 = 10023;
const CURLOPT_HTTPGET: i64 = 80;
const CURLOPT_HTTPPROXYTUNNEL: i64 = 61;
const CURLOPT_HEADERFUNCTION: i64 = 20079;
const CURLOPT_WRITEFUNCTION: i64 = 20011;
const CURLOPT_BUFFERSIZE: i64 = 98;
const CURLOPT_CAINFO: i64 = 10065;
const CURLOPT_HTTPAUTH: i64 = 107;
const CURLOPT_IPRESOLVE: i64 = 113;
const CURLOPT_NOPROXY: i64 = 10177;
const CURLOPT_PORT: i64 = 3;
const CURLOPT_PROTOCOLS: i64 = 181;
const CURLOPT_PROXY: i64 = 10004;
const CURLOPT_PROXYAUTH: i64 = 111;
const CURLOPT_PROXYPORT: i64 = 59;
const CURLOPT_PROXYTYPE: i64 = 101;
const CURLOPT_PROXYUSERNAME: i64 = 10175;
const CURLOPT_PROXYPASSWORD: i64 = 10176;
const CURLOPT_PROXYUSERPWD: i64 = 10006;
const CURLOPT_REDIR_PROTOCOLS: i64 = 182;
const CURLOPT_TCP_NODELAY: i64 = 121;
const CURLOPT_USERNAME: i64 = 10173;
const CURLOPT_PASSWORD: i64 = 10174;
const CURLOPT_USERPWD: i64 = 10005;
const CURLOPT_POST: i64 = 47;
const CURLOPT_POSTFIELDS: i64 = 10015;
const CURLOPT_CUSTOMREQUEST: i64 = 10036;
const CURLOPT_PRIVATE: i64 = 10103;
const CURLOPT_SSLCERT: i64 = 10025;
const CURLOPT_SSLKEY: i64 = 10087;
const CURLOPT_SSL_VERIFYPEER: i64 = 64;
const CURLOPT_SSL_VERIFYHOST: i64 = 81;
const CURLOPT_SSLVERSION: i64 = 32;
const CURLOPT_VERBOSE: i64 = 41;
const CURLINFO_EFFECTIVE_URL: i64 = 1048577;
const CURLINFO_RESPONSE_CODE: i64 = 2097154;
const CURLINFO_HEADER_SIZE: i64 = 2097163;
const CURLINFO_HTTP_CONNECTCODE: i64 = 2097174;
const CURLINFO_TOTAL_TIME: i64 = 3145731;
const CURLINFO_NAMELOOKUP_TIME: i64 = 3145732;
const CURLINFO_CONNECT_TIME: i64 = 3145733;
const CURLINFO_PRETRANSFER_TIME: i64 = 3145734;
const CURLINFO_CONTENT_TYPE: i64 = 1048594;
const CURLINFO_STARTTRANSFER_TIME: i64 = 3145745;
const CURLINFO_REDIRECT_TIME: i64 = 3145747;
const CURLINFO_REDIRECT_COUNT: i64 = 2097172;
const CURLINFO_REQUEST_SIZE: i64 = 2097164;
const CURLINFO_SIZE_DOWNLOAD: i64 = 3145736;
const CURLINFO_PRIVATE: i64 = 1048597;
const CURL_IPRESOLVE_WHATEVER: i64 = 0;
const CURL_IPRESOLVE_V4: i64 = 1;
const CURL_IPRESOLVE_V6: i64 = 2;
const CURL_SSLVERSION_DEFAULT: i64 = 0;
const CURL_SSLVERSION_TLSV1: i64 = 1;
const CURL_SSLVERSION_SSLV2: i64 = 2;
const CURL_SSLVERSION_SSLV3: i64 = 3;
const CURL_SSLVERSION_TLSV1_0: i64 = 4;
const CURL_SSLVERSION_TLSV1_1: i64 = 5;
const CURL_SSLVERSION_TLSV1_2: i64 = 6;
const CURL_SSLVERSION_TLSV1_3: i64 = 7;
const CURLM_OK: i64 = 0;
const CURLM_BAD_HANDLE: i64 = 1;
const CURLMSG_DONE: i64 = 1;
const CURLSHE_OK: i64 = 0;
const CURLSHE_BAD_OPTION: i64 = 1;
const CURLSHOPT_SHARE: i64 = 1;
const CURLSHOPT_UNSHARE: i64 = 2;
const CURL_VERSION_SSL: i64 = 4;
const CURL_VERSION_LIBZ: i64 = 8;
const CURL_VERSION_IPV6: i64 = 1;
const CURL_VERSION_NTLM: i64 = 16;
const CURL_VERSION_LARGEFILE: i64 = 512;
const CURL_VERSION_ASYNCHDNS: i64 = 128;
const CURL_VERSION_SPNEGO: i64 = 256;
const CURL_VERSION_IDN: i64 = 1024;
const CURL_VERSION_SSPI: i64 = 2048;
const CURL_VERSION_CONV: i64 = 4096;
const CURL_VERSION_TLSAUTH_SRP: i64 = 16384;
const CURL_VERSION_NTLM_WB: i64 = 32768;
const CURL_VERSION_HTTP2: i64 = 65536;
const CURL_VERSION_UNIX_SOCKETS: i64 = 524288;
const CURL_VERSION_HTTPS_PROXY: i64 = 2097152;
const CURL_VERSION_BROTLI: i64 = 8388608;
const CURL_VERSION_ALTSVC: i64 = 16777216;
const CURL_VERSION_HTTP3: i64 = 33554432;
const CURL_VERSION_ZSTD: i64 = 67108864;
const CURL_VERSION_UNICODE: i64 = 134217728;
const CURL_VERSION_HSTS: i64 = 268435456;
const CURL_VERSION_GSASL: i64 = 536870912;

type CurlTransportError = (i64, String);
type CurlPostBody = (Vec<u8>, Option<&'static str>);

struct CurlHandleRuntimeView {
    options: BTreeMap<i64, Value>,
    closed: bool,
}

pub(in crate::builtins::modules) fn builtin_curl_version(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_version", &args, 0)?;
    let version = Version::get();
    let mut out = PhpArray::new();
    out.insert(
        ArrayKey::String(PhpString::from("version_number")),
        Value::Int(version.version_num() as i64),
    );
    out.insert(ArrayKey::String(PhpString::from("age")), Value::Int(0));
    out.insert(
        ArrayKey::String(PhpString::from("features")),
        Value::Int(curl_version_feature_bits(&version)),
    );
    out.insert(
        ArrayKey::String(PhpString::from("ssl_version_number")),
        Value::Int(0),
    );
    out.insert(
        ArrayKey::String(PhpString::from("version")),
        Value::String(PhpString::from(version.version())),
    );
    out.insert(
        ArrayKey::String(PhpString::from("host")),
        Value::String(PhpString::from(version.host())),
    );
    out.insert(
        ArrayKey::String(PhpString::from("ssl_version")),
        Value::String(PhpString::from(version.ssl_version().unwrap_or(""))),
    );
    out.insert(
        ArrayKey::String(PhpString::from("libz_version")),
        Value::String(PhpString::from(version.libz_version().unwrap_or(""))),
    );
    out.insert(
        ArrayKey::String(PhpString::from("protocols")),
        Value::packed_array(
            version
                .protocols()
                .map(|protocol| Value::String(PhpString::from(protocol)))
                .collect(),
        ),
    );
    Ok(Value::Array(out))
}

fn curl_version_feature_bits(version: &Version) -> i64 {
    let mut bits = 0;
    if version.feature_ipv6() {
        bits |= CURL_VERSION_IPV6;
    }
    if version.feature_ssl() {
        bits |= CURL_VERSION_SSL;
    }
    if version.feature_libz() {
        bits |= CURL_VERSION_LIBZ;
    }
    if version.feature_ntlm() {
        bits |= CURL_VERSION_NTLM;
    }
    if version.feature_spnego() {
        bits |= CURL_VERSION_SPNEGO;
    }
    if version.feature_largefile() {
        bits |= CURL_VERSION_LARGEFILE;
    }
    if version.feature_idn() {
        bits |= CURL_VERSION_IDN;
    }
    if version.feature_sspi() {
        bits |= CURL_VERSION_SSPI;
    }
    if version.feature_async_dns() {
        bits |= CURL_VERSION_ASYNCHDNS;
    }
    if version.feature_conv() {
        bits |= CURL_VERSION_CONV;
    }
    if version.feature_tlsauth_srp() {
        bits |= CURL_VERSION_TLSAUTH_SRP;
    }
    if version.feature_ntlm_wb() {
        bits |= CURL_VERSION_NTLM_WB;
    }
    if version.feature_unix_domain_socket() {
        bits |= CURL_VERSION_UNIX_SOCKETS;
    }
    if version.feature_https_proxy() {
        bits |= CURL_VERSION_HTTPS_PROXY;
    }
    if version.feature_http2() {
        bits |= CURL_VERSION_HTTP2;
    }
    if version.feature_http3() {
        bits |= CURL_VERSION_HTTP3;
    }
    if version.feature_brotli() {
        bits |= CURL_VERSION_BROTLI;
    }
    if version.feature_altsvc() {
        bits |= CURL_VERSION_ALTSVC;
    }
    if version.feature_zstd() {
        bits |= CURL_VERSION_ZSTD;
    }
    if version.feature_unicode() {
        bits |= CURL_VERSION_UNICODE;
    }
    if version.feature_hsts() {
        bits |= CURL_VERSION_HSTS;
    }
    if version.feature_gsasl() {
        bits |= CURL_VERSION_GSASL;
    }
    bits
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

pub(in crate::builtins::modules) fn builtin_curl_strerror(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_strerror", &args, 1)?;
    let code = int_arg("curl_strerror", &args[0])?;
    let message = match code {
        0 => "No error",
        1 => "Unsupported protocol",
        3 => "URL using bad/illegal format or missing URL",
        7 => "Could not connect to server",
        22 => "HTTP response code said error",
        23 => "Failed writing received data to disk/application",
        28 => "Timeout was reached",
        35 => "SSL connect error",
        47 => "Number of redirects hit maximum amount",
        48 => "An unknown option was passed in to libcurl",
        55 => "Failed sending data to the peer",
        56 => "Failure when receiving data from the peer",
        61 => "Unrecognized or bad HTTP Content or Transfer-Encoding",
        _ => "Unknown error",
    };
    Ok(Value::string(message))
}

fn curl_multi_init(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_init", &args, 0)?;
    let object =
        ObjectRef::new_with_display_name(&curl_runtime_class("CurlMultiHandle"), "CurlMultiHandle");
    context.curl_state().reset_multi(object.id());
    reset_curl_multi_handle(&object);
    Ok(Value::Object(object))
}

fn curl_multi_add_handle(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_add_handle", &args, 2)?;
    let multi = curl_multi_handle_arg("curl_multi_add_handle", args.first())?;
    let handle = curl_handle_arg("curl_multi_add_handle", args.get(1))?;
    let Some(runtime) = context.curl_state().multi_mut(multi.id()) else {
        return Ok(Value::Int(CURLM_BAD_HANDLE));
    };
    if runtime.closed || multi.get_property("__curl_multi_closed") == Some(Value::Bool(true)) {
        return Ok(Value::Int(CURLM_BAD_HANDLE));
    }
    let mut handles = curl_multi_handles(&multi);
    if !handles.iter().any(|existing| existing == &handle) {
        handles.push(handle);
    }
    set_curl_multi_handles(&multi, handles);
    set_curl_multi_pending(&multi, Vec::new());
    Ok(Value::Int(CURLM_OK))
}

fn curl_multi_exec(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_exec", &args, 2)?;
    let multi = curl_multi_handle_arg("curl_multi_exec", args.first())?;
    if multi.get_property("__curl_multi_closed") == Some(Value::Bool(true)) {
        return Ok(Value::Int(CURLM_BAD_HANDLE));
    }
    let network_requests_enabled = curl_network_requests_enabled(context);
    let handles = curl_multi_handles(&multi);
    let runtime_views = handles
        .iter()
        .map(|handle| {
            (
                handle.id(),
                CurlHandleRuntimeView {
                    options: context.curl_state_ref().options_snapshot(handle.id()),
                    closed: context.curl_state_ref().is_closed(handle.id()),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = Vec::new();
    {
        let curl_state = context.curl_state();
        let Some(runtime) = curl_state.multi_mut(multi.id()) else {
            return Ok(Value::Int(CURLM_BAD_HANDLE));
        };
        if runtime.closed {
            return Ok(Value::Int(CURLM_BAD_HANDLE));
        }
        for handle in handles {
            if runtime.transfers.contains_key(&handle.id()) {
                continue;
            }
            let Some(view) = runtime_views.get(&handle.id()) else {
                continue;
            };
            match build_multi_easy_for_handle(&handle, network_requests_enabled, view) {
                Ok(easy) => match runtime.multi.add2(easy) {
                    Ok(mut easy) => {
                        if let Err(error) = easy.set_token(handle.id() as usize) {
                            let (code, message) = curl_easy_error(error);
                            set_curl_error(&handle, code, message);
                            runtime
                                .pending
                                .push_back(curl_multi_done_entry(handle, code));
                            continue;
                        }
                        runtime.transfers.insert(
                            handle.id(),
                            CurlMultiTransferState {
                                object: handle,
                                easy,
                                completed: false,
                            },
                        );
                    }
                    Err(error) => {
                        let (code, message) = curl_multi_error(error);
                        set_curl_error(&handle, code, message);
                        runtime
                            .pending
                            .push_back(curl_multi_done_entry(handle, code));
                    }
                },
                Err((code, message)) => {
                    set_curl_error(&handle, code, message.clone());
                    diagnostics.push((handle.clone(), code, message));
                    runtime
                        .pending
                        .push_back(curl_multi_done_entry(handle, code));
                }
            }
        }
        let running = match runtime.multi.perform() {
            Ok(running) => {
                drain_curl_multi_messages(runtime, &runtime_views);
                running
            }
            Err(error) => {
                let (code, message) = curl_multi_error(error);
                for transfer in runtime.transfers.values() {
                    set_curl_error(&transfer.object, code, message.clone());
                }
                0
            }
        };
        if let Some(Value::Reference(cell)) = args.get(1) {
            cell.set(Value::Int(i64::from(running)));
        }
        set_curl_multi_pending(&multi, curl_multi_pending_values(&runtime.pending));
    }
    for (handle, code, message) in diagnostics {
        record_curl_diagnostic(
            context,
            &handle,
            CurlDiagnostic::new(
                "E_PHP_CURL_REQUEST_FAILED",
                "curl_multi_exec",
                "build_request",
                "enabled",
                code,
                message,
            ),
            span.clone(),
        );
    }
    Ok(Value::Int(CURLM_OK))
}

fn curl_multi_select(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin curl_multi_select expects one or two argument(s)",
        ));
    }
    let multi = curl_multi_handle_arg("curl_multi_select", args.first())?;
    let timeout = match args.get(1) {
        Some(value) => float_arg("curl_multi_select", value)?,
        None => 1.0,
    };
    let timeout = Duration::from_secs_f64(timeout.clamp(0.0, 1.0));
    let Some(runtime) = context.curl_state().multi_mut(multi.id()) else {
        return Ok(Value::Int(-1));
    };
    if runtime.closed || multi.get_property("__curl_multi_closed") == Some(Value::Bool(true)) {
        return Ok(Value::Int(-1));
    }
    let ready = runtime
        .multi
        .wait(&mut [], timeout)
        .map(i64::from)
        .unwrap_or(-1);
    Ok(Value::Int(ready))
}

fn curl_multi_info_read(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin curl_multi_info_read expects one or two argument(s)",
        ));
    }
    let multi = curl_multi_handle_arg("curl_multi_info_read", args.first())?;
    if multi.get_property("__curl_multi_closed") == Some(Value::Bool(true)) {
        return Ok(Value::Bool(false));
    }
    let Some(runtime) = context.curl_state().multi_mut(multi.id()) else {
        return Ok(Value::Bool(false));
    };
    if runtime.pending.is_empty() {
        if let Some(Value::Reference(cell)) = args.get(1) {
            cell.set(Value::Int(0));
        }
        return Ok(Value::Bool(false));
    }
    let Some(entry) = runtime.pending.pop_front() else {
        if let Some(Value::Reference(cell)) = args.get(1) {
            cell.set(Value::Int(0));
        }
        set_curl_multi_pending(&multi, curl_multi_pending_values(&runtime.pending));
        return Ok(Value::Bool(false));
    };
    if let Some(Value::Reference(cell)) = args.get(1) {
        cell.set(Value::Int(runtime.pending.len() as i64));
    }
    set_curl_multi_pending(&multi, curl_multi_pending_values(&runtime.pending));
    Ok(curl_multi_done_value(&entry))
}

fn curl_multi_remove_handle(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_remove_handle", &args, 2)?;
    let multi = curl_multi_handle_arg("curl_multi_remove_handle", args.first())?;
    let handle = curl_handle_arg("curl_multi_remove_handle", args.get(1))?;
    let handles = curl_multi_handles(&multi)
        .into_iter()
        .filter(|existing| existing != &handle)
        .collect::<Vec<_>>();
    set_curl_multi_handles(&multi, handles);
    context.curl_state().detach_handle_from_multis(handle.id());
    Ok(Value::Int(CURLM_OK))
}

fn curl_multi_close(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_multi_close", &args, 1)?;
    let multi = curl_multi_handle_arg("curl_multi_close", args.first())?;
    context.curl_state().close_multi(multi.id());
    multi.set_property("__curl_multi_closed", Value::Bool(true));
    Ok(Value::Null)
}

fn curl_share_init(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_share_init", &args, 0)?;
    let object =
        ObjectRef::new_with_display_name(&curl_runtime_class("CurlShareHandle"), "CurlShareHandle");
    context.curl_state().reset_share(object.id());
    reset_curl_share_handle(&object);
    Ok(Value::Object(object))
}

fn curl_share_setopt(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_share_setopt", &args, 3)?;
    let share = curl_share_handle_arg("curl_share_setopt", args.first())?;
    let option = int_arg("curl_share_setopt", &args[1])?;
    match option {
        CURLSHOPT_SHARE | CURLSHOPT_UNSHARE => {
            if let Some(state) = context.curl_state().share_mut(share.id()) {
                let lock = int_arg("curl_share_setopt", &args[2])?;
                if option == CURLSHOPT_SHARE {
                    state.shared_options.insert(lock);
                } else {
                    state.shared_options.remove(&lock);
                }
            }
            share.set_property("__curl_share_errno", Value::Int(CURLSHE_OK));
            Ok(Value::Bool(true))
        }
        _ => {
            share.set_property("__curl_share_errno", Value::Int(CURLSHE_BAD_OPTION));
            Err(argument_value_error(
                "curl_share_setopt",
                "#2 ($option)",
                "is not a valid cURL share option",
            ))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_curl_share_errno(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_share_errno", &args, 1)?;
    let share = curl_share_handle_arg("curl_share_errno", args.first())?;
    Ok(curl_int_property(&share, "__curl_share_errno"))
}

pub(in crate::builtins::modules) fn builtin_curl_share_strerror(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_share_strerror", &args, 1)?;
    let code = int_arg("curl_share_strerror", &args[0])?;
    let message = match code {
        CURLSHE_OK => "No error",
        CURLSHE_BAD_OPTION => "Unknown share option",
        2 => "Share currently in use",
        3 => "Invalid share handle",
        4 => "Out of memory",
        5 => "Feature not enabled",
        _ => "Unknown error",
    };
    Ok(Value::string(message))
}

fn curl_share_close(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_share_close", &args, 1)?;
    let share = curl_share_handle_arg("curl_share_close", args.first())?;
    if let Some(state) = context.curl_state().share_mut(share.id()) {
        state.closed = true;
        state.shared_options.clear();
    }
    share.set_property("__curl_share_closed", Value::Bool(true));
    Ok(Value::Null)
}

fn curl_init(
    context: &mut CurlBuiltinServices<'_, '_>,
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
    context.curl_state().reset_handle(handle.id());
    if let Some(url) = args.first() {
        let url = string_arg("curl_init", url)?;
        context
            .curl_state()
            .set_option(handle.id(), CURLOPT_URL, Value::String(url.clone()));
        handle.set_property("__curl_url", Value::String(url));
    }
    Ok(Value::Object(handle))
}

fn curl_setopt(
    context: &mut CurlBuiltinServices<'_, '_>,
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
            context
                .curl_state()
                .set_option(handle.id(), CURLOPT_URL, Value::String(value.clone()));
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
        CURLOPT_ACCEPT_ENCODING => "__curl_encoding",
        CURLOPT_HTTP_VERSION => "__curl_http_version",
        CURLOPT_CONNECTTIMEOUT => "__curl_connecttimeout",
        CURLOPT_CONNECTTIMEOUT_MS => "__curl_connecttimeout_ms",
        CURLOPT_MAXREDIRS => "__curl_maxredirs",
        CURLOPT_FAILONERROR => "__curl_failonerror",
        CURLOPT_AUTOREFERER => "__curl_autoreferer",
        CURLOPT_COOKIE => "__curl_cookie",
        CURLOPT_COOKIEFILE => "__curl_cookiefile",
        CURLOPT_COOKIEJAR => "__curl_cookiejar",
        CURLOPT_COOKIESESSION => "__curl_cookiesession",
        CURLOPT_DNS_CACHE_TIMEOUT => "__curl_dns_cache_timeout",
        CURLOPT_HTTPHEADER => "__curl_httpheader",
        CURLOPT_HTTPGET => "__curl_httpget",
        CURLOPT_HTTPPROXYTUNNEL => "__curl_httpproxytunnel",
        CURLOPT_HEADERFUNCTION => "__curl_headerfunction",
        CURLOPT_WRITEFUNCTION => "__curl_writefunction",
        CURLOPT_BUFFERSIZE => "__curl_buffersize",
        CURLOPT_CAINFO => "__curl_cainfo",
        CURLOPT_HTTPAUTH => "__curl_httpauth",
        CURLOPT_IPRESOLVE => "__curl_ipresolve",
        CURLOPT_NOPROXY => "__curl_noproxy",
        CURLOPT_PORT => "__curl_port",
        CURLOPT_PROTOCOLS => "__curl_protocols",
        CURLOPT_PROXY => "__curl_proxy",
        CURLOPT_PROXYAUTH => "__curl_proxyauth",
        CURLOPT_PROXYPORT => "__curl_proxyport",
        CURLOPT_PROXYTYPE => "__curl_proxytype",
        CURLOPT_PROXYUSERNAME => "__curl_proxyusername",
        CURLOPT_PROXYPASSWORD => "__curl_proxypassword",
        CURLOPT_PROXYUSERPWD => "__curl_proxyuserpwd",
        CURLOPT_REDIR_PROTOCOLS => "__curl_redir_protocols",
        CURLOPT_TCP_NODELAY => "__curl_tcp_nodelay",
        CURLOPT_USERNAME => "__curl_username",
        CURLOPT_PASSWORD => "__curl_password",
        CURLOPT_USERPWD => "__curl_userpwd",
        CURLOPT_POST => "__curl_post",
        CURLOPT_POSTFIELDS => "__curl_postfields",
        CURLOPT_CUSTOMREQUEST => "__curl_customrequest",
        CURLOPT_PRIVATE => "__curl_private",
        CURLOPT_SSLCERT => "__curl_sslcert",
        CURLOPT_SSLKEY => "__curl_sslkey",
        CURLOPT_SSL_VERIFYPEER => "__curl_ssl_verifypeer",
        CURLOPT_SSL_VERIFYHOST => "__curl_ssl_verifyhost",
        CURLOPT_SSLVERSION => "__curl_sslversion",
        CURLOPT_VERBOSE => "__curl_verbose",
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
    context
        .curl_state()
        .set_option(handle.id(), option, value.clone());
    handle.set_property(property, value);
    Ok(Value::Bool(true))
}

fn curl_setopt_array(
    context: &mut CurlBuiltinServices<'_, '_>,
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
            ArrayKey::Int(option) => option,
            ArrayKey::String(option) => option.to_string_lossy().parse().unwrap_or(-1),
        };
        let ok = set_curl_option(context, &handle, option, value.clone())?;
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

fn curl_exec(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_exec", &args, 1)?;
    let handle = curl_handle_arg("curl_exec", args.first())?;
    let network_requests_enabled = curl_network_requests_enabled(context);
    let runtime = CurlHandleRuntimeView {
        options: context.curl_state_ref().options_snapshot(handle.id()),
        closed: context.curl_state_ref().is_closed(handle.id()),
    };
    if !network_requests_enabled && !curl_handle_targets_loopback(&handle) {
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
    let request = match build_request(&handle, network_requests_enabled, Some(&runtime)) {
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
    let response = match execute_http_request(&request, &handle, Some(&runtime)) {
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
    if curl_bool_option(
        &handle,
        Some(&runtime),
        CURLOPT_FAILONERROR,
        "__curl_failonerror",
    ) && response.status >= 400
    {
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
        "__curl_http_connectcode",
        Value::Int(i64::from(response.http_connectcode)),
    );
    handle.set_property(
        "__curl_total_time",
        Value::Float(FloatValue::from_f64(response.total_time)),
    );
    handle.set_property(
        "__curl_content_type",
        response
            .content_type
            .map(|value| Value::String(PhpString::from(value.into_bytes())))
            .unwrap_or(Value::Bool(false)),
    );
    handle.set_property(
        "__curl_namelookup_time",
        Value::Float(FloatValue::from_f64(response.namelookup_time)),
    );
    handle.set_property(
        "__curl_connect_time",
        Value::Float(FloatValue::from_f64(response.connect_time)),
    );
    handle.set_property(
        "__curl_pretransfer_time",
        Value::Float(FloatValue::from_f64(response.pretransfer_time)),
    );
    handle.set_property(
        "__curl_starttransfer_time",
        Value::Float(FloatValue::from_f64(response.starttransfer_time)),
    );
    handle.set_property(
        "__curl_redirect_time",
        Value::Float(FloatValue::from_f64(response.redirect_time)),
    );
    handle.set_property(
        "__curl_redirect_count",
        Value::Int(i64::from(response.redirect_count)),
    );
    handle.set_property(
        "__curl_request_size",
        Value::Int(response.request_size as i64),
    );
    handle.set_property(
        "__curl_size_download",
        Value::Float(FloatValue::from_f64(response.download_size)),
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

    let body = if curl_bool_option(&handle, Some(&runtime), CURLOPT_HEADER, "__curl_header") {
        let mut bytes = response_headers;
        bytes.extend_from_slice(&response_body);
        bytes
    } else {
        response_body
    };
    if curl_bool_option(
        &handle,
        Some(&runtime),
        CURLOPT_RETURNTRANSFER,
        "__curl_returntransfer",
    ) {
        Ok(Value::string(body))
    } else {
        context.output().write_bytes(&body);
        Ok(Value::Bool(true))
    }
}

fn curl_network_requests_enabled(context: &CurlBuiltinServices<'_, '_>) -> bool {
    if let Some(enabled) = *NET_TESTS_OVERRIDE
        .lock()
        .expect("network test override lock")
    {
        return enabled;
    }

    context.network_requests_enabled()
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
            CURLINFO_HTTP_CONNECTCODE => curl_int_property(&handle, "__curl_http_connectcode"),
            CURLINFO_TOTAL_TIME => curl_float_property(&handle, "__curl_total_time"),
            CURLINFO_CONTENT_TYPE => handle
                .get_property("__curl_content_type")
                .unwrap_or(Value::Bool(false)),
            CURLINFO_NAMELOOKUP_TIME => curl_float_property(&handle, "__curl_namelookup_time"),
            CURLINFO_CONNECT_TIME => curl_float_property(&handle, "__curl_connect_time"),
            CURLINFO_PRETRANSFER_TIME => curl_float_property(&handle, "__curl_pretransfer_time"),
            CURLINFO_STARTTRANSFER_TIME => {
                curl_float_property(&handle, "__curl_starttransfer_time")
            }
            CURLINFO_REDIRECT_TIME => curl_float_property(&handle, "__curl_redirect_time"),
            CURLINFO_REDIRECT_COUNT => curl_int_property(&handle, "__curl_redirect_count"),
            CURLINFO_REQUEST_SIZE => curl_int_property(&handle, "__curl_request_size"),
            CURLINFO_SIZE_DOWNLOAD => curl_float_property(&handle, "__curl_size_download"),
            CURLINFO_PRIVATE => handle.get_property("__curl_private").unwrap_or(Value::Null),
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
    out.insert(
        ArrayKey::String(PhpString::from("content_type")),
        handle
            .get_property("__curl_content_type")
            .unwrap_or(Value::Bool(false)),
    );
    out.insert(
        ArrayKey::String(PhpString::from("namelookup_time")),
        curl_float_property(&handle, "__curl_namelookup_time"),
    );
    out.insert(
        ArrayKey::String(PhpString::from("connect_time")),
        curl_float_property(&handle, "__curl_connect_time"),
    );
    out.insert(
        ArrayKey::String(PhpString::from("pretransfer_time")),
        curl_float_property(&handle, "__curl_pretransfer_time"),
    );
    out.insert(
        ArrayKey::String(PhpString::from("starttransfer_time")),
        curl_float_property(&handle, "__curl_starttransfer_time"),
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

fn curl_close(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_close", &args, 1)?;
    let handle = curl_handle_arg("curl_close", args.first())?;
    context.curl_state().close_handle(handle.id());
    handle.set_property("__curl_closed", Value::Bool(true));
    Ok(Value::Null)
}

fn curl_reset(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_reset", &args, 1)?;
    let handle = curl_handle_arg("curl_reset", args.first())?;
    context.curl_state().reset_handle(handle.id());
    reset_curl_handle(&handle);
    Ok(Value::Null)
}

fn curl_copy_handle(
    context: &mut CurlBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("curl_copy_handle", &args, 1)?;
    let handle = curl_handle_arg("curl_copy_handle", args.first())?;
    let copy = curl_handle_object();
    context.curl_state().copy_handle(handle.id(), copy.id());
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
        "__curl_autoreferer",
        "__curl_cookie",
        "__curl_cookiefile",
        "__curl_cookiejar",
        "__curl_cookiesession",
        "__curl_dns_cache_timeout",
        "__curl_httpheader",
        "__curl_httpget",
        "__curl_httpproxytunnel",
        "__curl_headerfunction",
        "__curl_writefunction",
        "__curl_buffersize",
        "__curl_cainfo",
        "__curl_httpauth",
        "__curl_ipresolve",
        "__curl_noproxy",
        "__curl_port",
        "__curl_protocols",
        "__curl_proxy",
        "__curl_proxyauth",
        "__curl_proxyport",
        "__curl_proxytype",
        "__curl_proxyusername",
        "__curl_proxypassword",
        "__curl_proxyuserpwd",
        "__curl_redir_protocols",
        "__curl_tcp_nodelay",
        "__curl_username",
        "__curl_password",
        "__curl_userpwd",
        "__curl_post",
        "__curl_postfields",
        "__curl_customrequest",
        "__curl_private",
        "__curl_sslcert",
        "__curl_sslkey",
        "__curl_ssl_verifypeer",
        "__curl_ssl_verifyhost",
        "__curl_sslversion",
        "__curl_verbose",
    ] {
        if let Some(value) = handle.get_property(property) {
            copy.set_property(property, value);
        }
    }
    Ok(Value::Object(copy))
}

fn build_request(
    handle: &ObjectRef,
    network_requests_enabled: bool,
    runtime: Option<&CurlHandleRuntimeView>,
) -> Result<CurlRequest, (i64, String)> {
    if runtime.map(|state| state.closed).unwrap_or(false)
        || handle.get_property("__curl_closed") == Some(Value::Bool(true))
    {
        return Err((3, "cURL handle is closed".to_owned()));
    }
    let url = match curl_runtime_option(handle, runtime, CURLOPT_URL, "__curl_url") {
        Some(Value::String(value)) if !value.is_empty() => value.to_string_lossy(),
        _ => return Err((3, "cURL URL is empty".to_owned())),
    };
    let parsed = parse_http_url(&url)?;
    if !network_requests_enabled && !curl_host_is_loopback(&parsed.host) {
        return Err((
            7,
            "cURL MVP only permits local loopback hosts when network tests are enabled".to_owned(),
        ));
    }
    let (body, content_type) = curl_post_body(handle, runtime)?;
    let http_get = curl_bool_option(handle, runtime, CURLOPT_HTTPGET, "__curl_httpget");
    let post = !http_get
        && (curl_bool_option(handle, runtime, CURLOPT_POST, "__curl_post") || !body.is_empty());
    let method = match curl_runtime_option(
        handle,
        runtime,
        CURLOPT_CUSTOMREQUEST,
        "__curl_customrequest",
    ) {
        Some(Value::String(value)) if !value.is_empty() => value.to_string_lossy(),
        _ if http_get => "GET".to_owned(),
        _ if curl_bool_option(handle, runtime, CURLOPT_NOBODY, "__curl_nobody") => {
            "HEAD".to_owned()
        }
        _ if post => "POST".to_owned(),
        _ => "GET".to_owned(),
    };
    let mut headers = curl_header_lines(handle, runtime);
    if let Some(content_type) = content_type
        && !headers
            .iter()
            .any(|header| header.to_ascii_lowercase().starts_with("content-type:"))
    {
        headers.push(format!("Content-Type: {content_type}"));
    }
    Ok(CurlRequest {
        url,
        method,
        headers,
        body,
        connect_timeout: curl_connect_timeout(handle, runtime),
        timeout: curl_timeout(handle, runtime),
        follow_redirects: curl_bool_option(
            handle,
            runtime,
            CURLOPT_FOLLOWLOCATION,
            "__curl_followlocation",
        ),
        max_redirects: curl_int_setting(handle, runtime, CURLOPT_MAXREDIRS, "__curl_maxredirs", 5)
            .clamp(0, 20) as usize,
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

fn execute_http_request(
    request: &CurlRequest,
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
) -> Result<CurlResponse, (i64, String)> {
    let mut easy = Easy2::new(CurlEasyCollector::default());
    apply_curl_easy_options(&mut easy, request, handle, runtime)?;
    easy.perform().map_err(curl_easy_error)?;

    curl_response_from_easy(&easy, &request.url)
}

fn build_multi_easy_for_handle(
    handle: &ObjectRef,
    network_requests_enabled: bool,
    runtime: &CurlHandleRuntimeView,
) -> Result<Easy2<CurlEasyCollector>, CurlTransportError> {
    let request = build_request(handle, network_requests_enabled, Some(runtime))?;
    let mut easy = Easy2::new(CurlEasyCollector::default());
    apply_curl_easy_options(&mut easy, &request, handle, Some(runtime))?;
    Ok(easy)
}

fn drain_curl_multi_messages(
    runtime: &mut CurlMultiRuntimeState,
    runtime_views: &BTreeMap<u64, CurlHandleRuntimeView>,
) {
    let mut completed = Vec::new();
    {
        let transfers = &mut runtime.transfers;
        runtime.multi.messages(|message| {
            let Ok(token) = message.token() else {
                return;
            };
            let handle_id = token as u64;
            let Some(transfer) = transfers.get_mut(&handle_id) else {
                return;
            };
            let Some(result) = message.result_for2(&transfer.easy) else {
                return;
            };
            transfer.completed = true;
            let runtime_view = runtime_views.get(&handle_id);
            let result = match result {
                Ok(()) => match curl_response_from_easy2_handle(&transfer.easy) {
                    Ok(response) => {
                        let code = if curl_bool_option(
                            &transfer.object,
                            runtime_view,
                            CURLOPT_FAILONERROR,
                            "__curl_failonerror",
                        ) && response.status >= 400
                        {
                            set_curl_error(&transfer.object, 22, "HTTP response code said error");
                            22
                        } else {
                            set_curl_error(&transfer.object, 0, "");
                            0
                        };
                        store_curl_response(&transfer.object, response);
                        code
                    }
                    Err((code, message)) => {
                        set_curl_error(&transfer.object, code, message);
                        code
                    }
                },
                Err(error) => {
                    let (code, message) = curl_easy_error(error);
                    set_curl_error(&transfer.object, code, message);
                    code
                }
            };
            completed.push((handle_id, result));
        });
    }
    for (handle_id, result) in completed {
        if let Some(transfer) = runtime.transfers.get(&handle_id) {
            runtime
                .pending
                .push_back(curl_multi_done_entry(transfer.object.clone(), result));
        }
    }
}

fn curl_response_from_easy(
    easy: &Easy2<CurlEasyCollector>,
    fallback_url: &str,
) -> Result<CurlResponse, CurlTransportError> {
    let status = easy.response_code().map_err(curl_easy_error)? as u16;
    let effective_url = easy
        .effective_url_bytes()
        .map_err(curl_easy_error)?
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_else(|| fallback_url.to_owned());
    let header_size = easy.header_size().map_err(curl_easy_error)? as usize;
    let http_connectcode = easy.http_connectcode().map_err(curl_easy_error)?;
    let total_time = easy.total_time().map_err(curl_easy_error)?.as_secs_f64();
    let namelookup_time = easy
        .namelookup_time()
        .map_err(curl_easy_error)?
        .as_secs_f64();
    let connect_time = easy.connect_time().map_err(curl_easy_error)?.as_secs_f64();
    let pretransfer_time = easy
        .pretransfer_time()
        .map_err(curl_easy_error)?
        .as_secs_f64();
    let starttransfer_time = easy
        .starttransfer_time()
        .map_err(curl_easy_error)?
        .as_secs_f64();
    let redirect_time = easy.redirect_time().map_err(curl_easy_error)?.as_secs_f64();
    let redirect_count = easy.redirect_count().map_err(curl_easy_error)?;
    let request_size = easy.request_size().map_err(curl_easy_error)?;
    let download_size = easy.download_size().map_err(curl_easy_error)?;
    let content_type = easy
        .content_type_bytes()
        .map_err(curl_easy_error)?
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
    let collector = easy.get_ref();
    Ok(CurlResponse {
        status,
        effective_url,
        header_size: header_size.max(collector.headers.len()),
        http_connectcode,
        headers: collector.headers.clone(),
        body: collector.body.clone(),
        content_type,
        total_time,
        namelookup_time,
        connect_time,
        pretransfer_time,
        starttransfer_time,
        redirect_time,
        redirect_count,
        request_size,
        download_size,
    })
}

fn curl_response_from_easy2_handle(
    easy: &curl::multi::Easy2Handle<CurlEasyCollector>,
) -> Result<CurlResponse, CurlTransportError> {
    let status = easy.response_code().map_err(curl_easy_error)? as u16;
    let effective_url = easy
        .effective_url_bytes()
        .map_err(curl_easy_error)?
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();
    let header_size = easy.header_size().map_err(curl_easy_error)? as usize;
    let http_connectcode = easy.http_connectcode().map_err(curl_easy_error)?;
    let total_time = easy.total_time().map_err(curl_easy_error)?.as_secs_f64();
    let namelookup_time = easy
        .namelookup_time()
        .map_err(curl_easy_error)?
        .as_secs_f64();
    let connect_time = easy.connect_time().map_err(curl_easy_error)?.as_secs_f64();
    let pretransfer_time = easy
        .pretransfer_time()
        .map_err(curl_easy_error)?
        .as_secs_f64();
    let starttransfer_time = easy
        .starttransfer_time()
        .map_err(curl_easy_error)?
        .as_secs_f64();
    let redirect_time = easy.redirect_time().map_err(curl_easy_error)?.as_secs_f64();
    let redirect_count = easy.redirect_count().map_err(curl_easy_error)?;
    let request_size = easy.request_size().map_err(curl_easy_error)?;
    let download_size = easy.download_size().map_err(curl_easy_error)?;
    let content_type = easy
        .content_type_bytes()
        .map_err(curl_easy_error)?
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
    let collector = easy.get_ref();
    Ok(CurlResponse {
        status,
        effective_url,
        header_size: header_size.max(collector.headers.len()),
        http_connectcode,
        headers: collector.headers.clone(),
        body: collector.body.clone(),
        content_type,
        total_time,
        namelookup_time,
        connect_time,
        pretransfer_time,
        starttransfer_time,
        redirect_time,
        redirect_count,
        request_size,
        download_size,
    })
}

fn store_curl_response(handle: &ObjectRef, response: CurlResponse) {
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
        "__curl_http_connectcode",
        Value::Int(i64::from(response.http_connectcode)),
    );
    handle.set_property(
        "__curl_total_time",
        Value::Float(FloatValue::from_f64(response.total_time)),
    );
    handle.set_property(
        "__curl_content_type",
        response
            .content_type
            .map(|value| Value::String(PhpString::from(value.into_bytes())))
            .unwrap_or(Value::Bool(false)),
    );
    handle.set_property(
        "__curl_namelookup_time",
        Value::Float(FloatValue::from_f64(response.namelookup_time)),
    );
    handle.set_property(
        "__curl_connect_time",
        Value::Float(FloatValue::from_f64(response.connect_time)),
    );
    handle.set_property(
        "__curl_pretransfer_time",
        Value::Float(FloatValue::from_f64(response.pretransfer_time)),
    );
    handle.set_property(
        "__curl_starttransfer_time",
        Value::Float(FloatValue::from_f64(response.starttransfer_time)),
    );
    handle.set_property(
        "__curl_redirect_time",
        Value::Float(FloatValue::from_f64(response.redirect_time)),
    );
    handle.set_property(
        "__curl_redirect_count",
        Value::Int(i64::from(response.redirect_count)),
    );
    handle.set_property(
        "__curl_request_size",
        Value::Int(response.request_size as i64),
    );
    handle.set_property(
        "__curl_size_download",
        Value::Float(FloatValue::from_f64(response.download_size)),
    );
    handle.set_property(
        "__curl_last_response_headers",
        Value::String(PhpString::from(response.headers)),
    );
    handle.set_property(
        "__curl_last_response_body",
        Value::String(PhpString::from(response.body)),
    );
}

fn apply_curl_easy_options(
    easy: &mut Easy2<CurlEasyCollector>,
    request: &CurlRequest,
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
) -> Result<(), CurlTransportError> {
    easy.url(&request.url).map_err(curl_easy_error)?;
    easy.connect_timeout(request.connect_timeout)
        .map_err(curl_easy_error)?;
    easy.timeout(request.timeout).map_err(curl_easy_error)?;
    easy.follow_location(request.follow_redirects)
        .map_err(curl_easy_error)?;
    easy.max_redirections(request.max_redirects as u32)
        .map_err(curl_easy_error)?;
    if request.follow_redirects {
        let mut post_redirects = PostRedirections::new();
        post_redirects.redirect_all(true);
        easy.post_redirections(&post_redirects)
            .map_err(curl_easy_error)?;
    }
    if request.method.eq_ignore_ascii_case("HEAD") {
        easy.nobody(true).map_err(curl_easy_error)?;
    } else if !request.body.is_empty() || request.method.eq_ignore_ascii_case("POST") {
        easy.post(true).map_err(curl_easy_error)?;
        easy.post_fields_copy(&request.body)
            .map_err(curl_easy_error)?;
    }
    if !matches!(request.method.as_str(), "GET" | "POST" | "HEAD") {
        easy.custom_request(&request.method)
            .map_err(curl_easy_error)?;
    }
    if !request.headers.is_empty() {
        let mut headers = List::new();
        for header in &request.headers {
            headers.append(header).map_err(curl_easy_error)?;
        }
        easy.http_headers(headers).map_err(curl_easy_error)?;
    }
    if curl_bool_option(handle, runtime, CURLOPT_HTTPGET, "__curl_httpget") {
        easy.get(true).map_err(curl_easy_error)?;
    }
    if curl_bool_option(handle, runtime, CURLOPT_AUTOREFERER, "__curl_autoreferer") {
        easy.autoreferer(true).map_err(curl_easy_error)?;
    }
    if curl_bool_option(
        handle,
        runtime,
        CURLOPT_COOKIESESSION,
        "__curl_cookiesession",
    ) {
        easy.cookie_session(true).map_err(curl_easy_error)?;
    }
    if curl_bool_option(
        handle,
        runtime,
        CURLOPT_HTTPPROXYTUNNEL,
        "__curl_httpproxytunnel",
    ) {
        easy.http_proxy_tunnel(true).map_err(curl_easy_error)?;
    }
    if curl_bool_option(handle, runtime, CURLOPT_TCP_NODELAY, "__curl_tcp_nodelay") {
        easy.tcp_nodelay(true).map_err(curl_easy_error)?;
    }
    if curl_bool_option(handle, runtime, CURLOPT_VERBOSE, "__curl_verbose") {
        easy.verbose(true).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_USERAGENT, "__curl_useragent")
    {
        easy.useragent(&value).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_REFERER, "__curl_referer")
    {
        easy.referer(&value).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_ENCODING, "__curl_encoding")
    {
        easy.accept_encoding(&value).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_COOKIE, "__curl_cookie")
    {
        easy.cookie(&value).map_err(curl_easy_error)?;
    }
    if let Some(value) = curl_optional_string_setting_allow_empty(
        handle,
        runtime,
        CURLOPT_COOKIEFILE,
        "__curl_cookiefile",
    ) {
        easy.cookie_file(value).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_COOKIEJAR, "__curl_cookiejar")
    {
        easy.cookie_jar(value).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_CAINFO, "__curl_cainfo")
    {
        easy.cainfo(value).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_SSLCERT, "__curl_sslcert")
    {
        easy.ssl_cert(value).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_SSLKEY, "__curl_sslkey")
    {
        easy.ssl_key(value).map_err(curl_easy_error)?;
    }
    if let Some(version) = curl_http_version(handle, runtime) {
        easy.http_version(version).map_err(curl_easy_error)?;
    }
    if let Some(resolve) = curl_ip_resolve(handle, runtime) {
        easy.ip_resolve(resolve).map_err(curl_easy_error)?;
    }
    if let Some(version) = curl_ssl_version(handle, runtime) {
        easy.ssl_version(version).map_err(curl_easy_error)?;
    }
    if let Some(seconds) = curl_optional_u64_setting(
        handle,
        runtime,
        CURLOPT_DNS_CACHE_TIMEOUT,
        "__curl_dns_cache_timeout",
    ) {
        easy.dns_cache_timeout(Duration::from_secs(seconds))
            .map_err(curl_easy_error)?;
    }
    if let Some(port) = curl_optional_u16_setting(handle, runtime, CURLOPT_PORT, "__curl_port") {
        easy.port(port).map_err(curl_easy_error)?;
    }
    easy.ssl_verify_peer(curl_bool_setting(
        handle,
        runtime,
        CURLOPT_SSL_VERIFYPEER,
        "__curl_ssl_verifypeer",
        true,
    ))
    .map_err(curl_easy_error)?;
    easy.ssl_verify_host(curl_bool_setting(
        handle,
        runtime,
        CURLOPT_SSL_VERIFYHOST,
        "__curl_ssl_verifyhost",
        true,
    ))
    .map_err(curl_easy_error)?;
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_PROXY, "__curl_proxy")
    {
        easy.proxy(&value).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_NOPROXY, "__curl_noproxy")
    {
        easy.noproxy(&value).map_err(curl_easy_error)?;
    }
    if let Some(port) =
        curl_optional_u16_setting(handle, runtime, CURLOPT_PROXYPORT, "__curl_proxyport")
    {
        easy.proxy_port(port).map_err(curl_easy_error)?;
    }
    if let Some(proxy_type) = curl_proxy_type(handle, runtime) {
        easy.proxy_type(proxy_type).map_err(curl_easy_error)?;
    }
    if let Some((username, password)) =
        curl_optional_user_password(handle, runtime, CURLOPT_PROXYUSERPWD, "__curl_proxyuserpwd")
    {
        easy.proxy_username(&username).map_err(curl_easy_error)?;
        easy.proxy_password(&password).map_err(curl_easy_error)?;
    }
    if let Some(value) = curl_optional_string_setting(
        handle,
        runtime,
        CURLOPT_PROXYUSERNAME,
        "__curl_proxyusername",
    ) {
        easy.proxy_username(&value).map_err(curl_easy_error)?;
    }
    if let Some(value) = curl_optional_string_setting(
        handle,
        runtime,
        CURLOPT_PROXYPASSWORD,
        "__curl_proxypassword",
    ) {
        easy.proxy_password(&value).map_err(curl_easy_error)?;
    }
    if let Some((username, password)) =
        curl_optional_user_password(handle, runtime, CURLOPT_USERPWD, "__curl_userpwd")
    {
        easy.username(&username).map_err(curl_easy_error)?;
        easy.password(&password).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_USERNAME, "__curl_username")
    {
        easy.username(&value).map_err(curl_easy_error)?;
    }
    if let Some(value) =
        curl_optional_string_setting(handle, runtime, CURLOPT_PASSWORD, "__curl_password")
    {
        easy.password(&value).map_err(curl_easy_error)?;
    }
    if let Some(auth) = curl_auth(handle, runtime, CURLOPT_HTTPAUTH, "__curl_httpauth") {
        easy.http_auth(&auth).map_err(curl_easy_error)?;
    }
    if let Some(auth) = curl_auth(handle, runtime, CURLOPT_PROXYAUTH, "__curl_proxyauth") {
        easy.proxy_auth(&auth).map_err(curl_easy_error)?;
    }
    Ok(())
}

fn curl_easy_error(error: curl::Error) -> CurlTransportError {
    (i64::from(error.code()), error.description().to_owned())
}

fn curl_multi_error(error: MultiError) -> CurlTransportError {
    (error.code() as i64, error.description().to_owned())
}

fn parse_http_url(url: &str) -> Result<ParsedUrl, (i64, String)> {
    let rest = if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else {
        return Err((
            3,
            "cURL MVP only supports http:// and https:// URLs".to_owned(),
        ));
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    let (host, _) = authority
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
        .map_or((authority, 0), |(host, port)| (host, port));
    if host.is_empty() {
        return Err((3, "cURL URL host is empty".to_owned()));
    }
    Ok(ParsedUrl {
        host: host.trim_matches(['[', ']']).to_owned(),
    })
}

fn curl_header_lines(handle: &ObjectRef, runtime: Option<&CurlHandleRuntimeView>) -> Vec<String> {
    match curl_runtime_option(handle, runtime, CURLOPT_HTTPHEADER, "__curl_httpheader") {
        Some(Value::Array(array)) => array
            .iter()
            .filter_map(|(_, value)| string_arg("curl_exec", value).ok())
            .map(|value| value.to_string_lossy())
            .collect(),
        _ => Vec::new(),
    }
}

fn curl_post_body(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
) -> Result<CurlPostBody, CurlTransportError> {
    match curl_runtime_option(handle, runtime, CURLOPT_POSTFIELDS, "__curl_postfields") {
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

fn curl_runtime_option(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<Value> {
    runtime
        .and_then(|state| state.options.get(&option).cloned())
        .or_else(|| handle.get_property(property))
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

fn curl_connect_timeout(handle: &ObjectRef, runtime: Option<&CurlHandleRuntimeView>) -> Duration {
    if let Some(ms) = curl_duration_millis_setting(
        handle,
        runtime,
        CURLOPT_CONNECTTIMEOUT_MS,
        "__curl_connecttimeout_ms",
    ) {
        return clamp_duration_millis(ms, 1, 30_000);
    }
    if let Some(seconds) = curl_duration_seconds_setting(
        handle,
        runtime,
        CURLOPT_CONNECTTIMEOUT,
        "__curl_connecttimeout",
    ) {
        return clamp_duration_millis(seconds * 1000.0, 1, 30_000);
    }
    Duration::from_secs(10)
}

fn curl_timeout(handle: &ObjectRef, runtime: Option<&CurlHandleRuntimeView>) -> Duration {
    if let Some(ms) =
        curl_duration_millis_setting(handle, runtime, CURLOPT_TIMEOUT_MS, "__curl_timeout_ms")
    {
        return clamp_duration_millis(ms, 1, 300_000);
    }
    if let Some(seconds) =
        curl_duration_seconds_setting(handle, runtime, CURLOPT_TIMEOUT, "__curl_timeout")
    {
        return clamp_duration_millis(seconds * 1000.0, 1, 300_000);
    }
    Duration::from_secs(60)
}

fn curl_duration_seconds_setting(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<f64> {
    curl_duration_numeric_setting(handle, runtime, option, property)
}

fn curl_duration_millis_setting(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<f64> {
    curl_duration_numeric_setting(handle, runtime, option, property)
}

fn curl_duration_numeric_setting(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<f64> {
    let value = curl_runtime_option(handle, runtime, option, property)?;
    match value {
        Value::Int(value) => Some(value as f64),
        Value::Float(value) => Some(value.to_f64()),
        value => crate::convert::to_float(&value).ok(),
    }
}

fn curl_bool_option(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> bool {
    curl_bool_setting(handle, runtime, option, property, false)
}

fn curl_bool_setting(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
    default: bool,
) -> bool {
    curl_runtime_option(handle, runtime, option, property)
        .and_then(|value| crate::convert::to_bool(&value).ok())
        .unwrap_or(default)
}

fn curl_optional_string_setting(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<String> {
    match curl_runtime_option(handle, runtime, option, property) {
        Some(Value::String(value)) if !value.is_empty() => Some(value.to_string_lossy()),
        Some(value) => string_arg("curl_setopt", &value)
            .ok()
            .map(|value| value.to_string_lossy())
            .filter(|value| !value.is_empty()),
        None => None,
    }
}

fn curl_optional_string_setting_allow_empty(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<String> {
    match curl_runtime_option(handle, runtime, option, property) {
        Some(Value::String(value)) => Some(value.to_string_lossy()),
        Some(value) => string_arg("curl_setopt", &value)
            .ok()
            .map(|value| value.to_string_lossy()),
        None => None,
    }
}

fn curl_optional_u16_setting(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<u16> {
    let value = curl_runtime_option(handle, runtime, option, property)?;
    let value = crate::convert::to_int(&value).ok()?;
    u16::try_from(value).ok()
}

fn curl_optional_u64_setting(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<u64> {
    let value = curl_runtime_option(handle, runtime, option, property)?;
    let value = crate::convert::to_int(&value).ok()?;
    u64::try_from(value).ok()
}

fn curl_optional_user_password(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<(String, String)> {
    let value = curl_optional_string_setting(handle, runtime, option, property)?;
    let (username, password) = value.split_once(':').unwrap_or((value.as_str(), ""));
    Some((username.to_owned(), password.to_owned()))
}

fn curl_http_version(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
) -> Option<HttpVersion> {
    let value = curl_runtime_option(handle, runtime, CURLOPT_HTTP_VERSION, "__curl_http_version")
        .and_then(|value| crate::convert::to_int(&value).ok())?;
    Some(match value {
        0 => HttpVersion::Any,
        1 => HttpVersion::V10,
        2 => HttpVersion::V11,
        3 => HttpVersion::V2,
        4 => HttpVersion::V2TLS,
        5 => HttpVersion::V2PriorKnowledge,
        30 => HttpVersion::V3,
        _ => return None,
    })
}

fn curl_ip_resolve(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
) -> Option<IpResolve> {
    let value = curl_runtime_option(handle, runtime, CURLOPT_IPRESOLVE, "__curl_ipresolve")
        .and_then(|value| crate::convert::to_int(&value).ok())?;
    Some(match value {
        CURL_IPRESOLVE_WHATEVER => IpResolve::Any,
        CURL_IPRESOLVE_V4 => IpResolve::V4,
        CURL_IPRESOLVE_V6 => IpResolve::V6,
        _ => return None,
    })
}

fn curl_ssl_version(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
) -> Option<SslVersion> {
    let value = curl_runtime_option(handle, runtime, CURLOPT_SSLVERSION, "__curl_sslversion")
        .and_then(|value| crate::convert::to_int(&value).ok())?;
    Some(match value {
        CURL_SSLVERSION_DEFAULT => SslVersion::Default,
        CURL_SSLVERSION_TLSV1 => SslVersion::Tlsv1,
        CURL_SSLVERSION_SSLV2 => SslVersion::Sslv2,
        CURL_SSLVERSION_SSLV3 => SslVersion::Sslv3,
        CURL_SSLVERSION_TLSV1_0 => SslVersion::Tlsv10,
        CURL_SSLVERSION_TLSV1_1 => SslVersion::Tlsv11,
        CURL_SSLVERSION_TLSV1_2 => SslVersion::Tlsv12,
        CURL_SSLVERSION_TLSV1_3 => SslVersion::Tlsv13,
        _ => return None,
    })
}

fn curl_auth(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
) -> Option<Auth> {
    let value = curl_runtime_option(handle, runtime, option, property)
        .and_then(|value| crate::convert::to_int(&value).ok())?;
    if value == 0 {
        return None;
    }
    let mut auth = Auth::new();
    auth.basic(value & 1 != 0);
    auth.digest(value & 2 != 0);
    auth.ntlm(value & 8 != 0);
    if value & !0b1011 != 0 {
        auth.auto(true);
    }
    Some(auth)
}

fn curl_proxy_type(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
) -> Option<ProxyType> {
    let value = curl_runtime_option(handle, runtime, CURLOPT_PROXYTYPE, "__curl_proxytype")
        .and_then(|value| crate::convert::to_int(&value).ok())?;
    Some(match value {
        0 => ProxyType::Http,
        1 => ProxyType::Http1,
        4 => ProxyType::Socks4,
        5 => ProxyType::Socks5,
        6 => ProxyType::Socks4a,
        7 => ProxyType::Socks5Hostname,
        _ => ProxyType::Http,
    })
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
    object.set_property("__curl_http_connectcode", Value::Int(0));
    object.set_property("__curl_total_time", Value::Float(FloatValue::from_f64(0.0)));
    object.set_property("__curl_content_type", Value::Bool(false));
    object.set_property(
        "__curl_namelookup_time",
        Value::Float(FloatValue::from_f64(0.0)),
    );
    object.set_property(
        "__curl_connect_time",
        Value::Float(FloatValue::from_f64(0.0)),
    );
    object.set_property(
        "__curl_pretransfer_time",
        Value::Float(FloatValue::from_f64(0.0)),
    );
    object.set_property(
        "__curl_starttransfer_time",
        Value::Float(FloatValue::from_f64(0.0)),
    );
    object.set_property(
        "__curl_redirect_time",
        Value::Float(FloatValue::from_f64(0.0)),
    );
    object.set_property("__curl_redirect_count", Value::Int(0));
    object.set_property("__curl_request_size", Value::Int(0));
    object.set_property(
        "__curl_size_download",
        Value::Float(FloatValue::from_f64(0.0)),
    );
    object.set_property(
        "__curl_last_response_headers",
        Value::String(PhpString::from("")),
    );
    object.set_property(
        "__curl_last_response_body",
        Value::String(PhpString::from("")),
    );
}

fn reset_curl_multi_handle(object: &ObjectRef) {
    object.set_property("__curl_multi_closed", Value::Bool(false));
    object.set_property("__curl_multi_executed", Value::Bool(false));
    set_curl_multi_handles(object, Vec::new());
    set_curl_multi_pending(object, Vec::new());
}

fn reset_curl_share_handle(object: &ObjectRef) {
    object.set_property("__curl_share_errno", Value::Int(CURLSHE_OK));
    object.set_property("__curl_share_closed", Value::Bool(false));
}

fn curl_multi_handles(multi: &ObjectRef) -> Vec<ObjectRef> {
    let Some(Value::Array(array)) = multi.get_property("__curl_multi_handles") else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|(_, value)| match value {
            Value::Object(object) if object.class_name() == "curlhandle" => Some(object.clone()),
            _ => None,
        })
        .collect()
}

fn set_curl_multi_handles(multi: &ObjectRef, handles: Vec<ObjectRef>) {
    multi.set_property(
        "__curl_multi_handles",
        Value::packed_array(handles.into_iter().map(Value::Object).collect()),
    );
}

fn set_curl_multi_pending(multi: &ObjectRef, pending: Vec<Value>) {
    multi.set_property("__curl_multi_pending", Value::packed_array(pending));
}

fn curl_multi_done_entry(handle: ObjectRef, result: i64) -> CurlMultiDone {
    CurlMultiDone { handle, result }
}

fn curl_multi_pending_values(pending: &std::collections::VecDeque<CurlMultiDone>) -> Vec<Value> {
    pending.iter().map(curl_multi_done_value).collect()
}

fn curl_multi_done_value(done: &CurlMultiDone) -> Value {
    let mut entry = PhpArray::new();
    entry.insert(string_array_key("msg"), Value::Int(CURLMSG_DONE));
    entry.insert(string_array_key("result"), Value::Int(done.result));
    entry.insert(
        string_array_key("handle"),
        Value::Object(done.handle.clone()),
    );
    Value::Array(entry)
}

fn curl_runtime_class(name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name).into(),
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

fn curl_share_handle_arg(name: &str, value: Option<&Value>) -> Result<ObjectRef, BuiltinError> {
    match value {
        Some(Value::Object(object)) if object.class_name() == "curlsharehandle" => {
            Ok(object.clone())
        }
        Some(value) => Err(argument_type_error(name, "1", "CurlShareHandle", value)),
        None => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            format!("builtin {name} expects CurlShareHandle argument"),
        )),
    }
}

fn set_curl_option(
    context: &mut CurlBuiltinServices<'_, '_>,
    handle: &ObjectRef,
    option: i64,
    value: Value,
) -> BuiltinResult {
    let property = match option {
        CURLOPT_URL => {
            let value = string_arg("curl_setopt", &value)?;
            context
                .curl_state()
                .set_option(handle.id(), CURLOPT_URL, Value::String(value.clone()));
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
        CURLOPT_ACCEPT_ENCODING => "__curl_encoding",
        CURLOPT_HTTP_VERSION => "__curl_http_version",
        CURLOPT_CONNECTTIMEOUT => "__curl_connecttimeout",
        CURLOPT_CONNECTTIMEOUT_MS => "__curl_connecttimeout_ms",
        CURLOPT_MAXREDIRS => "__curl_maxredirs",
        CURLOPT_FAILONERROR => "__curl_failonerror",
        CURLOPT_AUTOREFERER => "__curl_autoreferer",
        CURLOPT_COOKIE => "__curl_cookie",
        CURLOPT_COOKIEFILE => "__curl_cookiefile",
        CURLOPT_COOKIEJAR => "__curl_cookiejar",
        CURLOPT_COOKIESESSION => "__curl_cookiesession",
        CURLOPT_DNS_CACHE_TIMEOUT => "__curl_dns_cache_timeout",
        CURLOPT_HTTPHEADER => "__curl_httpheader",
        CURLOPT_HTTPGET => "__curl_httpget",
        CURLOPT_HTTPPROXYTUNNEL => "__curl_httpproxytunnel",
        CURLOPT_HEADERFUNCTION => "__curl_headerfunction",
        CURLOPT_WRITEFUNCTION => "__curl_writefunction",
        CURLOPT_BUFFERSIZE => "__curl_buffersize",
        CURLOPT_CAINFO => "__curl_cainfo",
        CURLOPT_HTTPAUTH => "__curl_httpauth",
        CURLOPT_IPRESOLVE => "__curl_ipresolve",
        CURLOPT_NOPROXY => "__curl_noproxy",
        CURLOPT_PORT => "__curl_port",
        CURLOPT_PROTOCOLS => "__curl_protocols",
        CURLOPT_PROXY => "__curl_proxy",
        CURLOPT_PROXYAUTH => "__curl_proxyauth",
        CURLOPT_PROXYPORT => "__curl_proxyport",
        CURLOPT_PROXYTYPE => "__curl_proxytype",
        CURLOPT_PROXYUSERNAME => "__curl_proxyusername",
        CURLOPT_PROXYPASSWORD => "__curl_proxypassword",
        CURLOPT_PROXYUSERPWD => "__curl_proxyuserpwd",
        CURLOPT_REDIR_PROTOCOLS => "__curl_redir_protocols",
        CURLOPT_TCP_NODELAY => "__curl_tcp_nodelay",
        CURLOPT_USERNAME => "__curl_username",
        CURLOPT_PASSWORD => "__curl_password",
        CURLOPT_USERPWD => "__curl_userpwd",
        CURLOPT_POST => "__curl_post",
        CURLOPT_POSTFIELDS => "__curl_postfields",
        CURLOPT_CUSTOMREQUEST => "__curl_customrequest",
        CURLOPT_PRIVATE => "__curl_private",
        CURLOPT_SSLCERT => "__curl_sslcert",
        CURLOPT_SSLKEY => "__curl_sslkey",
        CURLOPT_SSL_VERIFYPEER => "__curl_ssl_verifypeer",
        CURLOPT_SSL_VERIFYHOST => "__curl_ssl_verifyhost",
        CURLOPT_SSLVERSION => "__curl_sslversion",
        CURLOPT_VERBOSE => "__curl_verbose",
        _ => {
            set_curl_error(handle, 48, "unsupported cURL option");
            return Ok(Value::Bool(false));
        }
    };
    context
        .curl_state()
        .set_option(handle.id(), option, value.clone());
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
    context: &mut CurlBuiltinServices<'_, '_>,
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
    let payload = RuntimeBringupDiagnosticContext::new("db_network")
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
        .with_diagnostic_payload(RuntimeDiagnosticPayload::Bringup(payload)),
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

fn curl_int_property(handle: &ObjectRef, name: &str) -> Value {
    match handle.get_property(name) {
        Some(Value::Int(value)) => Value::Int(value),
        _ => Value::Int(0),
    }
}

fn curl_int_setting(
    handle: &ObjectRef,
    runtime: Option<&CurlHandleRuntimeView>,
    option: i64,
    property: &str,
    default: i64,
) -> i64 {
    match curl_runtime_option(handle, runtime, option, property) {
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
}

#[derive(Clone)]
struct CurlRequest {
    url: String,
    method: String,
    headers: Vec<String>,
    body: Vec<u8>,
    connect_timeout: Duration,
    timeout: Duration,
    follow_redirects: bool,
    max_redirects: usize,
}

struct CurlResponse {
    status: u16,
    effective_url: String,
    header_size: usize,
    http_connectcode: u32,
    headers: Vec<u8>,
    body: Vec<u8>,
    content_type: Option<String>,
    total_time: f64,
    namelookup_time: f64,
    connect_time: f64,
    pretransfer_time: f64,
    starttransfer_time: f64,
    redirect_time: f64,
    redirect_count: u32,
    request_size: u64,
    download_size: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{BuiltinRequestState, OutputBuffer, ReferenceCell};
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener};
    use std::thread;

    static NET_TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn curl_state_persists_in_one_request_owner_across_builtin_contexts() {
        let mut request_state = BuiltinRequestState::new();
        let handle = {
            let mut output = OutputBuffer::default();
            let mut context =
                BuiltinContext::new_with_request_state(&mut output, &mut request_state);
            builtin_curl_init(
                &mut context,
                vec![Value::string("http://example.com/")],
                RuntimeSourceSpan::default(),
            )
            .expect("init")
        };

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new_with_request_state(&mut output, &mut request_state);
        assert_eq!(
            builtin_curl_errno(&mut context, vec![handle], RuntimeSourceSpan::default())
                .expect("errno"),
            Value::Int(0)
        );
    }

    #[test]
    fn curl_version_advertises_ssl_transport_capability() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let libcurl = Version::get();
        let Value::Array(version) =
            builtin_curl_version(&mut context, vec![], RuntimeSourceSpan::default())
                .expect("curl_version")
        else {
            panic!("curl_version should return an array");
        };

        assert_eq!(
            version.get(&ArrayKey::String(PhpString::from("version_number"))),
            Some(&Value::Int(libcurl.version_num() as i64))
        );
        assert_eq!(
            version.get(&ArrayKey::String(PhpString::from("version"))),
            Some(&Value::String(PhpString::from(libcurl.version())))
        );
        assert_ne!(
            version.get(&ArrayKey::String(PhpString::from("version"))),
            Some(&Value::String(PhpString::from("phrust-curl-mvp")))
        );
        assert_eq!(
            version.get(&ArrayKey::String(PhpString::from("host"))),
            Some(&Value::String(PhpString::from(libcurl.host())))
        );
        assert_ne!(
            version.get(&ArrayKey::String(PhpString::from("host"))),
            Some(&Value::String(PhpString::from("phrust")))
        );
        assert_eq!(
            version.get(&ArrayKey::String(PhpString::from("features"))),
            Some(&Value::Int(curl_version_feature_bits(&libcurl)))
        );
        let Some(Value::Array(protocols)) =
            version.get(&ArrayKey::String(PhpString::from("protocols")))
        else {
            panic!("protocols should be an array");
        };
        assert!(
            protocols
                .iter()
                .any(|(_, value)| value == &Value::String(PhpString::from("https")))
        );
    }

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
        let Some(RuntimeDiagnosticPayload::Bringup(payload)) = diagnostics[0].payload() else {
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
    fn curl_setopt_unsupported_option_fails_with_diagnostic() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string("http://127.0.0.1/")],
            RuntimeSourceSpan::default(),
        )
        .expect("init");

        assert_eq!(
            builtin_curl_setopt(
                &mut context,
                vec![handle.clone(), Value::Int(99_999_999), Value::Bool(true)],
                RuntimeSourceSpan::default(),
            )
            .expect("setopt"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_curl_errno(
                &mut context,
                vec![handle.clone()],
                RuntimeSourceSpan::default()
            )
            .expect("errno"),
            Value::Int(48)
        );
        assert_eq!(
            builtin_curl_error(&mut context, vec![handle], RuntimeSourceSpan::default())
                .expect("error"),
            Value::string("unsupported cURL option")
        );
        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), "E_PHP_CURL_OPTION_UNSUPPORTED");
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
            vec![Value::string(format!(
                "http://127.0.0.1:{port}/site-health"
            ))],
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
    fn curl_loopback_gate_accepts_https_loopback_urls() {
        let handle = curl_handle_object();
        handle.set_property(
            "__curl_url",
            Value::String(PhpString::from("https://127.0.0.1/site-health")),
        );

        assert!(curl_handle_targets_loopback(&handle));
        build_request(&handle, false, None).expect("loopback https request");
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
            assert!(request.starts_with("GET /api/status"));
            assert!(request.contains(&format!("Host: 127.0.0.1:{port}\r\n")));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                .expect("write response");
        });

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("http://127.0.0.1:{port}/api/status"))],
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
    fn curl_multi_info_read_reports_failed_handle_errno() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(false);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let _ = stream.read(&mut [0_u8; 64]);
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
            let _ = stream.shutdown(Shutdown::Both);
        });

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let multi = builtin_curl_multi_init(&mut context, vec![], RuntimeSourceSpan::default())
            .expect("multi init");
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("https://127.0.0.1:{port}/"))],
            RuntimeSourceSpan::default(),
        )
        .expect("init");
        assert_eq!(
            builtin_curl_multi_add_handle(
                &mut context,
                vec![multi.clone(), handle.clone()],
                RuntimeSourceSpan::default(),
            )
            .expect("add"),
            Value::Int(CURLM_OK)
        );

        let active = ReferenceCell::new(Value::Null);
        for _ in 0..20 {
            assert_eq!(
                builtin_curl_multi_exec(
                    &mut context,
                    vec![multi.clone(), Value::Reference(active.clone())],
                    RuntimeSourceSpan::default(),
                )
                .expect("exec"),
                Value::Int(CURLM_OK)
            );
            if active.get() == Value::Int(0) {
                break;
            }
            let _ = builtin_curl_multi_select(
                &mut context,
                vec![multi.clone(), Value::Float(FloatValue::from_f64(0.05))],
                RuntimeSourceSpan::default(),
            )
            .expect("select");
        }
        assert_eq!(active.get(), Value::Int(0));

        let queue = ReferenceCell::new(Value::Null);
        let Value::Array(done) = builtin_curl_multi_info_read(
            &mut context,
            vec![multi.clone(), Value::Reference(queue.clone())],
            RuntimeSourceSpan::default(),
        )
        .expect("info read") else {
            panic!("curl_multi_info_read should return an array");
        };
        assert_eq!(queue.get(), Value::Int(0));
        assert_eq!(
            done.get(&string_array_key("msg")),
            Some(&Value::Int(CURLMSG_DONE))
        );
        let Some(Value::Int(result)) = done.get(&string_array_key("result")) else {
            panic!("done result should be an integer");
        };
        assert_ne!(*result, 0);
        assert_eq!(done.get(&string_array_key("handle")), Some(&handle));
        assert_eq!(
            builtin_curl_errno(&mut context, vec![handle], RuntimeSourceSpan::default())
                .expect("errno"),
            Value::Int(*result)
        );
        assert_eq!(
            builtin_curl_multi_info_read(
                &mut context,
                vec![multi, Value::Reference(queue)],
                RuntimeSourceSpan::default(),
            )
            .expect("second info read"),
            Value::Bool(false)
        );
        server.join().expect("server");
    }

    #[test]
    fn curl_multi_remove_handle_removes_added_handle() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let multi = builtin_curl_multi_init(&mut context, vec![], RuntimeSourceSpan::default())
            .expect("multi init");
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string("http://127.0.0.1/")],
            RuntimeSourceSpan::default(),
        )
        .expect("init");
        builtin_curl_multi_add_handle(
            &mut context,
            vec![multi.clone(), handle.clone()],
            RuntimeSourceSpan::default(),
        )
        .expect("add");
        assert_eq!(
            curl_multi_handles(match &multi {
                Value::Object(object) => object,
                _ => panic!("multi should be an object"),
            })
            .len(),
            1
        );

        assert_eq!(
            builtin_curl_multi_remove_handle(
                &mut context,
                vec![multi.clone(), handle],
                RuntimeSourceSpan::default(),
            )
            .expect("remove"),
            Value::Int(CURLM_OK)
        );
        assert!(
            curl_multi_handles(match &multi {
                Value::Object(object) => object,
                _ => panic!("multi should be an object"),
            })
            .is_empty()
        );
    }

    #[test]
    fn curl_build_request_allows_external_hosts_when_network_is_enabled() {
        let handle = curl_handle_object();
        handle.set_property(
            "__curl_url",
            Value::String(PhpString::from("https://updates.example.test/v1/check")),
        );

        assert!(build_request(&handle, false, None).is_err());
        let request = build_request(&handle, true, None).expect("external request allowed");
        assert_eq!(request.url, "https://updates.example.test/v1/check");
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
    fn curl_exec_decodes_complete_chunked_response() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(true);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).expect("read request");
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /chunked"));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n",
                )
                .expect("write response");
            thread::sleep(Duration::from_millis(500));
        });

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("http://127.0.0.1:{port}/chunked"))],
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
            Value::string("Wikipedia")
        );
        assert_eq!(
            curl_handle_arg("curl_exec", Some(&handle))
                .expect("curl handle")
                .get_property("__curl_last_response_body"),
            Some(Value::string("Wikipedia"))
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
    fn curl_exec_maps_libcurl_timeout_errors() {
        let _guard = NET_TEST_ENV_LOCK.lock().expect("env lock");
        let _override = NetTestsOverride::set(false);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).expect("read request");
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /timeout"));
            thread::sleep(Duration::from_millis(300));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nLATE");
        });

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_curl_init(
            &mut context,
            vec![Value::string(format!("http://127.0.0.1:{port}/timeout"))],
            RuntimeSourceSpan::default(),
        )
        .expect("init");
        for (option, value) in [
            (CURLOPT_RETURNTRANSFER, Value::Bool(true)),
            (CURLOPT_CONNECTTIMEOUT_MS, Value::Int(25)),
            (CURLOPT_TIMEOUT_MS, Value::Int(25)),
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
            Value::Bool(false)
        );
        assert_eq!(
            builtin_curl_errno(
                &mut context,
                vec![handle.clone()],
                RuntimeSourceSpan::default()
            )
            .expect("errno"),
            Value::Int(28)
        );
        let Value::String(error) =
            builtin_curl_error(&mut context, vec![handle], RuntimeSourceSpan::default())
                .expect("error")
        else {
            panic!("curl_error should return a string");
        };
        assert!(!error.is_empty());
        server.join().expect("server");
    }

    #[test]
    fn curl_exec_stores_callback_response_payloads_without_mutating_targets() {
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
        let Value::Object(handle_object) = &handle else {
            panic!("curl_init should return an object handle");
        };
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
            Some(Value::string(""))
        );
        assert_eq!(
            transport.get_property("response_bytes"),
            Some(Value::Int(0))
        );
        assert_eq!(transport.get_property("headers"), Some(Value::string("")));
        let Some(Value::String(headers)) =
            handle_object.get_property("__curl_last_response_headers")
        else {
            panic!("response headers should be stored on the cURL handle");
        };
        assert!(headers.to_string_lossy().starts_with("HTTP/1.1 200 OK"));
        assert_eq!(
            handle_object.get_property("__curl_last_response_body"),
            Some(Value::string("OK"))
        );
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
