//! Core builtin implementations and cross-module helpers.

use super::super::context::{
    JSON_ERROR_DEPTH, JSON_ERROR_INF_OR_NAN, JSON_ERROR_RECURSION, JSON_ERROR_SYNTAX,
    JSON_ERROR_UNSUPPORTED_TYPE, JSON_ERROR_UTF8, JSON_FORCE_OBJECT, JSON_HEX_AMP, JSON_HEX_APOS,
    JSON_HEX_QUOT, JSON_HEX_TAG, JSON_INVALID_UTF8_IGNORE, JSON_INVALID_UTF8_SUBSTITUTE,
    JSON_NUMERIC_CHECK, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_PRESERVE_ZERO_FRACTION,
    JSON_PRETTY_PRINT, JSON_THROW_ON_ERROR, JSON_UNESCAPED_LINE_TERMINATORS,
    JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE, json_error_message,
};
use super::super::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use super::debug_output::DebugFormatter;
pub(in crate::builtins::modules) use super::debug_output::php_float_debug_string;
mod encoding;
mod http;
mod password;
mod serialization;

use crate::convert::float_to_php_string;
use crate::numeric_string::{NumericStringKind, NumericStringValue, classify_php_string};
use crate::{
    ArrayKey, ClassEntry, ClassFlags, NumericValue, ObjectRef, OutputBuffer, PhpArray, PhpString,
    ResourceKind, StreamWrapperRegistry, UnserializeOptions, Value, compare, equal, identical,
    normalize_class_name, pcre, serialize as serialize_value, to_bool, to_float, to_int, to_number,
    to_string, unserialize as unserialize_value,
};
pub(in crate::builtins::modules) use encoding::{
    HTML_ESCAPE_DEFAULT_FLAGS, build_query_pairs, format_array_values, hash_digest_bytes,
    hex_decode, hex_encode, hex_nibble, hmac_digest_bytes, html_decode, html_escape_with_options,
    url_decode, url_encode,
};
use http::{
    builtin_header, builtin_header_remove, builtin_headers_list, builtin_headers_sent,
    builtin_http_response_code, builtin_memory_get_peak_usage, builtin_memory_get_usage,
    builtin_setcookie, builtin_setrawcookie,
};
use password::{builtin_password_hash, builtin_password_needs_rehash, builtin_password_verify};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use serialization::{
    builtin_serialize, builtin_setlocale, builtin_unserialize, builtin_var_export,
};
use std::fs::{self, Metadata};
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::builtins::modules) const RANGE_MAX_ELEMENTS: usize = 1_000_000;
pub(in crate::builtins::modules) const SORT_REGULAR: i64 = 0;
pub(in crate::builtins::modules) const SORT_NUMERIC: i64 = 1;
pub(in crate::builtins::modules) const SORT_STRING: i64 = 2;
pub(in crate::builtins::modules) const SORT_LOCALE_STRING: i64 = 5;
pub(in crate::builtins::modules) const SORT_NATURAL: i64 = 6;
pub(in crate::builtins::modules) const SORT_FLAG_CASE: i64 = 8;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("assert", builtin_assert, BuiltinCompatibility::Php),
    BuiltinEntry::new("boolval", builtin_boolval, BuiltinCompatibility::Php),
    BuiltinEntry::new("uniqid", builtin_uniqid, BuiltinCompatibility::Php),
    BuiltinEntry::new("usleep", builtin_usleep, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "set_time_limit",
        builtin_set_time_limit,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ignore_user_abort",
        builtin_config_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "error_reporting",
        builtin_error_handling_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "error_log",
        builtin_error_handling_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "exec",
        builtin_process_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("floatval", builtin_floatval, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "flush",
        builtin_output_buffering_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "get_cfg_var",
        builtin_config_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "get_current_user",
        builtin_environment_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "get_debug_type",
        builtin_get_debug_type,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "get_resource_id",
        builtin_get_resource_id,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "get_resource_type",
        builtin_get_resource_type,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "get_resources",
        builtin_get_resources,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "getenv",
        builtin_environment_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "gethostbyname",
        builtin_gethostbyname,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("header", builtin_header, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "header_remove",
        builtin_header_remove,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "headers_list",
        builtin_headers_list,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "headers_sent",
        builtin_headers_sent,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "http_response_code",
        builtin_http_response_code,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("setcookie", builtin_setcookie, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "setrawcookie",
        builtin_setrawcookie,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "gc_collect_cycles",
        builtin_gc_collect_cycles,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gc_enabled", builtin_gc_enabled, BuiltinCompatibility::Php),
    BuiltinEntry::new("gettype", builtin_gettype, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ini_get",
        builtin_config_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ini_get_all",
        builtin_config_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ini_set",
        builtin_config_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("intval", builtin_intval, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_array", builtin_is_array, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_bool", builtin_is_bool, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "is_countable",
        builtin_is_countable,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("is_double", builtin_is_float, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_float", builtin_is_float, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_int", builtin_is_int, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_integer", builtin_is_int, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "is_iterable",
        builtin_is_iterable,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("is_long", builtin_is_int, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_null", builtin_is_null, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_numeric", builtin_is_numeric, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_object", builtin_is_object, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "is_resource",
        builtin_is_resource,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("is_scalar", builtin_is_scalar, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_string", builtin_is_string, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "memory_get_peak_usage",
        builtin_memory_get_peak_usage,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "memory_get_usage",
        builtin_memory_get_usage,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("mail", builtin_mail, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ob_end_clean",
        builtin_output_buffering_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ob_end_flush",
        builtin_output_buffering_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ob_get_clean",
        builtin_output_buffering_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ob_get_flush",
        builtin_output_buffering_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ob_get_contents",
        builtin_output_buffering_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ob_get_length",
        builtin_output_buffering_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ob_get_level",
        builtin_output_buffering_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ob_start",
        builtin_output_buffering_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "passthru",
        builtin_process_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pclose",
        builtin_process_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "php_sapi_name",
        builtin_environment_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "php_uname",
        builtin_environment_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "popen",
        builtin_process_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("print", builtin_print, BuiltinCompatibility::Php),
    BuiltinEntry::new("print_r", builtin_print_r, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "proc_close",
        builtin_process_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "proc_get_status",
        builtin_process_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "proc_open",
        builtin_process_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "password_hash",
        builtin_password_hash,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "password_needs_rehash",
        builtin_password_needs_rehash,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "password_verify",
        builtin_password_verify,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "putenv",
        builtin_environment_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "random_bytes",
        builtin_random_bytes,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("random_int", builtin_random_int, BuiltinCompatibility::Php),
    BuiltinEntry::new("rand", builtin_rand, BuiltinCompatibility::Php),
    BuiltinEntry::new("mt_rand", builtin_mt_rand, BuiltinCompatibility::Php),
    BuiltinEntry::new("getrandmax", builtin_getrandmax, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "mt_getrandmax",
        builtin_mt_getrandmax,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "restore_error_handler",
        builtin_error_handling_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "restore_exception_handler",
        builtin_error_handling_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("serialize", builtin_serialize, BuiltinCompatibility::Php),
    BuiltinEntry::new("setlocale", builtin_setlocale, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "set_error_handler",
        builtin_error_handling_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "set_exception_handler",
        builtin_error_handling_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "shell_exec",
        builtin_process_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "system",
        builtin_process_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "token_get_all",
        builtin_token_get_all,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("token_name", builtin_token_name, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "trigger_error",
        builtin_error_handling_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "unserialize",
        builtin_unserialize,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "user_error",
        builtin_error_handling_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "debug_zval_dump",
        builtin_debug_zval_dump,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("var_dump", builtin_var_dump, BuiltinCompatibility::Php),
    BuiltinEntry::new("var_export", builtin_var_export, BuiltinCompatibility::Php),
];

pub(in crate::builtins::modules) fn expect_arity(
    name: &str,
    args: &[Value],
    expected: usize,
) -> Result<(), BuiltinError> {
    if args.len() == expected {
        return Ok(());
    }
    Err(arity_error(
        name,
        &format!("exactly {expected} argument(s)"),
    ))
}

pub(in crate::builtins::modules) fn arity_error(name: &str, expected: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_ARITY",
        format!("builtin {name} expects {expected}"),
    )
}

pub(in crate::builtins::modules) fn builtin_config_requires_vm(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_CONFIG_CONTEXT_REQUIRED",
        "configuration builtins require VM request-local INI state",
    ))
}

pub(in crate::builtins::modules) fn builtin_error_handling_requires_vm(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_ERROR_CONTEXT_REQUIRED",
        "error handling builtins require VM handler stacks and request-local INI state",
    ))
}

pub(in crate::builtins::modules) fn builtin_output_buffering_requires_vm(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_OUTPUT_BUFFER_CONTEXT_REQUIRED",
        "output buffering builtins require VM output buffer stack state",
    ))
}

pub(in crate::builtins::modules) fn builtin_environment_requires_vm(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_ENVIRONMENT_CONTEXT_REQUIRED",
        "environment builtins require VM request context state",
    ))
}

pub(in crate::builtins::modules) fn builtin_process_requires_vm(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_PROCESS_CONTEXT_REQUIRED",
        "process builtins require VM process capability policy",
    ))
}

pub(in crate::builtins::modules) fn builtin_random_bytes(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("random_bytes", &args, 1)?;
    let length = int_arg("random_bytes", &args[0])?;
    if length < 1 {
        return Err(value_error("random_bytes", "length must be greater than 0"));
    }
    let mut bytes = vec![0; length as usize];
    getrandom::fill(&mut bytes).map_err(|error| {
        BuiltinError::new(
            "E_PHP_RUNTIME_RANDOM_FAILURE",
            format!("random_bytes(): failed to read random bytes: {error}"),
        )
    })?;
    Ok(Value::string(bytes))
}

pub(in crate::builtins::modules) fn builtin_random_int(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("random_int", &args, 2)?;
    let min = int_arg("random_int", &args[0])?;
    let max = int_arg("random_int", &args[1])?;
    if max < min {
        return Err(value_error(
            "random_int",
            "max must be greater than or equal to min",
        ));
    }
    random_int_inclusive("random_int", min, max).map(Value::Int)
}

pub(in crate::builtins::modules) fn builtin_rand(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    random_range_builtin("rand", args)
}

pub(in crate::builtins::modules) fn builtin_mt_rand(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    random_range_builtin("mt_rand", args)
}

fn random_range_builtin(name: &str, args: Vec<Value>) -> BuiltinResult {
    if !args.is_empty() && args.len() != 2 {
        return Err(arity_error(name, "zero or two argument(s)"));
    }
    let (min, max) = if args.is_empty() {
        (0, i64::from(PHP_RAND_MAX))
    } else {
        (int_arg(name, &args[0])?, int_arg(name, &args[1])?)
    };
    if max < min {
        return Err(value_error(
            name,
            "max must be greater than or equal to min",
        ));
    }
    random_int_inclusive(name, min, max).map(Value::Int)
}

pub(in crate::builtins::modules) fn builtin_getrandmax(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("getrandmax", &args, 0)?;
    Ok(Value::Int(i64::from(PHP_RAND_MAX)))
}

pub(in crate::builtins::modules) fn builtin_mt_getrandmax(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mt_getrandmax", &args, 0)?;
    Ok(Value::Int(i64::from(PHP_RAND_MAX)))
}

const PHP_RAND_MAX: i32 = i32::MAX;

fn random_int_inclusive(name: &str, min: i64, max: i64) -> Result<i64, BuiltinError> {
    let range = (i128::from(max) - i128::from(min) + 1) as u128;
    let zone = u128::MAX - (u128::MAX % range);
    loop {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(|error| {
            BuiltinError::new(
                "E_PHP_RUNTIME_RANDOM_FAILURE",
                format!("{name}(): failed to read random bytes: {error}"),
            )
        })?;
        let sample = u128::from_le_bytes(bytes);
        if sample < zone {
            let offset = (sample % range) as i128;
            return Ok((i128::from(min) + offset) as i64);
        }
    }
}

pub(in crate::builtins::modules) fn builtin_gc_collect_cycles(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("gc_collect_cycles", &args, 0)?;
    Ok(Value::Int(0))
}

pub(in crate::builtins::modules) fn builtin_gc_enabled(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("gc_enabled", &args, 0)?;
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_usleep(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("usleep", &args, 1)?;
    let micros = int_arg("usleep", &args[0])?;
    if micros < 0 {
        return Err(value_error(
            "usleep",
            "Argument #1 ($microseconds) must be greater than or equal to 0",
        ));
    }
    std::thread::sleep(std::time::Duration::from_micros(micros as u64));
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_set_time_limit(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("set_time_limit", &args, 1)?;
    let seconds = int_arg("set_time_limit", &args[0])?;
    if seconds < 0 {
        return Err(value_error(
            "set_time_limit",
            "Argument #1 ($seconds) must be greater than or equal to 0",
        ));
    }
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_gethostbyname(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("gethostbyname", &args, 1)?;
    let hostname = string_arg("gethostbyname", &args[0])?;
    let hostname_text = hostname.to_string_lossy();
    if hostname_text.len() > 255 {
        context.php_warning(
            "E_PHP_RUNTIME_DNS_WARNING",
            "gethostbyname(): Host name cannot be longer than 255 characters",
            span,
        );
        return Ok(Value::String(hostname));
    }

    let resolved = (hostname_text.as_ref(), 0)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| {
            addrs.find_map(|addr| match addr.ip() {
                IpAddr::V4(ip) => Some(ip.to_string()),
                IpAddr::V6(_) => None,
            })
        });

    match resolved {
        Some(ip) => Ok(Value::string(ip)),
        None => {
            context.php_warning(
                "E_PHP_RUNTIME_DNS_WARNING",
                format!("gethostbyname(): Host name to ip failed {hostname_text}"),
                span,
            );
            Ok(Value::String(hostname))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_mail(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=5).contains(&args.len()) {
        return Err(arity_error("mail", "3 to 5 argument(s)"));
    }
    string_arg("mail", &args[0])?;
    string_arg("mail", &args[1])?;
    string_arg("mail", &args[2])?;
    if let Some(headers) = args.get(3) {
        match deref_value(headers) {
            Value::Array(_) => {}
            _ => {
                string_arg("mail", headers)?;
            }
        }
    }
    if let Some(params) = args.get(4) {
        string_arg("mail", params)?;
    }
    Ok(Value::Bool(true))
}

/// Monotonic per-process counter mixed into `uniqid(..., true)` so that two
/// back-to-back calls always differ even within the same microsecond.
static UNIQID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(in crate::builtins::modules) fn builtin_uniqid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error("uniqid", "zero to two argument(s)"));
    }
    let mut out = match args.first() {
        Some(value) => string_arg("uniqid", value)?.into_bytes(),
        None => Vec::new(),
    };
    let more_entropy = match args.get(1) {
        Some(value) => to_bool(value).map_err(|message| conversion_error("uniqid", message))?,
        None => false,
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| value_error("uniqid", "system time is before UNIX epoch"))?;
    // PHP: "%s%08x%05x" of seconds (low 32 bits) and microseconds.
    let sec = now.as_secs() & 0xFFFF_FFFF;
    let usec = now.subsec_micros();
    out.extend_from_slice(format!("{sec:08x}{usec:05x}").as_bytes());
    if more_entropy {
        // PHP appends "%.8F" of a small random float; we derive a value in
        // [0, 10) from the sub-microsecond clock and a per-call counter so it
        // is well-formed (always 10 chars) and unique between calls.
        let counter = UNIQID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mixed = u64::from(now.subsec_nanos())
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(counter);
        let entropy = (mixed % 1_000_000_000) as f64 / 100_000_000.0;
        out.extend_from_slice(format!("{entropy:.8}").as_bytes());
    }
    Ok(Value::string(out))
}

pub(in crate::builtins::modules) fn builtin_print(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("print", &args, 1)?;
    let value = args.into_iter().next().expect("checked arity");
    let string = to_string(&value).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("builtin print could not convert value: {message}"),
        )
    })?;
    context.output().write_php_string(&string);
    Ok(Value::Int(1))
}

pub(in crate::builtins::modules) fn builtin_token_get_all(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("token_get_all", "1 or 2 argument(s)"));
    }
    let source = to_string(&args[0])
        .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TOKENIZER_TYPE", message))?
        .to_string_lossy();
    let flags = args
        .get(1)
        .map_or(Ok(0), to_int)
        .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TOKENIZER_TYPE", message))?;
    crate::tokenizer::tokenize(&source, flags).map(crate::tokenizer::token_get_all_value)
}

pub(in crate::builtins::modules) fn builtin_token_name(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("token_name", &args, 1)?;
    let id = to_int(&args[0])
        .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TOKENIZER_TYPE", message))?;
    Ok(Value::string(
        crate::tokenizer::token_name_for_id(id)
            .unwrap_or("UNKNOWN")
            .as_bytes()
            .to_vec(),
    ))
}

pub(in crate::builtins::modules) fn builtin_gettype(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("gettype", &args, 1)?;
    Ok(Value::string(php_gettype(
        &args.into_iter().next().expect("checked arity"),
    )))
}

pub(in crate::builtins::modules) fn builtin_get_debug_type(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("get_debug_type", &args, 1)?;
    Ok(Value::string(php_debug_type(
        &args.into_iter().next().expect("checked arity"),
    )))
}

pub(in crate::builtins::modules) fn builtin_get_resource_id(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("get_resource_id", &args, 1)?;
    match deref_value(args.first().expect("checked arity")) {
        Value::Resource(resource) => Ok(Value::Int(resource.id().get() as i64)),
        _ => Ok(Value::Bool(false)),
    }
}

pub(in crate::builtins::modules) fn builtin_get_resource_type(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("get_resource_type", &args, 1)?;
    match deref_value(args.first().expect("checked arity")) {
        Value::Resource(resource) => Ok(Value::string(resource.resource_type().into_bytes())),
        _ => Ok(Value::Bool(false)),
    }
}

pub(in crate::builtins::modules) fn builtin_get_resources(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("get_resources", "zero or one argument(s)"));
    }

    let requested_type = match args.first().map(deref_value) {
        None | Some(Value::Null) => None,
        Some(Value::Array(_) | Value::Resource(_)) => {
            return Err(argument_type_error(
                "get_resources",
                "#1 ($type)",
                "?string",
                args.first().expect("checked presence"),
            ));
        }
        Some(value) => Some(string_arg("get_resources", &value)?.to_string_lossy()),
    };

    let Some(resources) = context.resources() else {
        return Ok(Value::Array(PhpArray::new()));
    };
    let resources = resources.resources();

    if let Some(resource_type) = requested_type.as_deref() {
        let has_matching_resource = resources
            .iter()
            .any(|resource| resource.resource_type() == resource_type);
        let can_be_empty = matches!(resource_type, "stream" | "stream-context" | "Unknown");
        if !has_matching_resource && !can_be_empty {
            return Err(argument_value_error(
                "get_resources",
                "#1 ($type)",
                "must be a valid resource type",
            ));
        }
    }

    let mut array = PhpArray::new();
    for resource in resources {
        if requested_type
            .as_deref()
            .is_none_or(|resource_type| resource.resource_type() == resource_type)
        {
            array.insert(
                ArrayKey::Int(resource.id().get() as i64),
                Value::Resource(resource),
            );
        }
    }

    Ok(Value::Array(array))
}

pub(in crate::builtins::modules) fn builtin_is_int(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_int", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Int(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_string(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_string", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::String(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_bool(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_bool", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Bool(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_null(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_null", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Null
    )))
}

pub(in crate::builtins::modules) fn builtin_is_array(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_array", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Array(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_float(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_float", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Float(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_object(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_object", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) | Value::Callable(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_resource(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_resource", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Resource(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_scalar(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_scalar", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_countable(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_countable", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Array(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_iterable(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_iterable", &args, 1)?;
    Ok(Value::Bool(matches!(
        deref_value(args.first().expect("checked arity")),
        Value::Array(_) | Value::Generator(_)
    )))
}

pub(in crate::builtins::modules) fn builtin_is_numeric(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_numeric", &args, 1)?;
    let is_numeric = match deref_value(args.first().expect("checked arity")) {
        Value::Int(_) | Value::Float(_) => true,
        Value::String(value) => matches!(
            classify_php_string(&value).kind,
            NumericStringKind::IntString | NumericStringKind::FloatString
        ),
        _ => false,
    };
    Ok(Value::Bool(is_numeric))
}

pub(in crate::builtins::modules) fn builtin_boolval(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("boolval", &args, 1)?;
    let value = args.into_iter().next().expect("checked arity");
    to_bool(&value)
        .map(Value::Bool)
        .map_err(|message| conversion_error("boolval", message))
}

pub(in crate::builtins::modules) fn builtin_assert(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("assert", "one or two argument(s)"));
    }
    let assertion = to_bool(&args[0]).map_err(|message| conversion_error("assert", message))?;
    if assertion {
        Ok(Value::Bool(true))
    } else {
        Err(BuiltinError::new(
            "E_PHP_RUNTIME_ASSERTION_ERROR",
            "Uncaught AssertionError: assert(false)",
        ))
    }
}

pub(in crate::builtins::modules) fn builtin_intval(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("intval", "one or two argument(s)"));
    }
    let base = args
        .get(1)
        .map(|value| int_arg("intval", value))
        .transpose()?
        .unwrap_or(10);
    let value = args.first().expect("checked arity");
    let Value::String(text) = deref_value(value) else {
        return to_int(value)
            .map(Value::Int)
            .map_err(|message| conversion_error("intval", message));
    };
    if base == 10 {
        return to_int(value)
            .map(Value::Int)
            .map_err(|message| conversion_error("intval", message));
    }
    Ok(Value::Int(parse_intval_string_base(text.as_bytes(), base)))
}

fn parse_intval_string_base(bytes: &[u8], base: i64) -> i64 {
    let mut cursor = 0;
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }

    let mut negative = false;
    if let Some(sign) = bytes.get(cursor)
        && (*sign == b'-' || *sign == b'+')
    {
        negative = *sign == b'-';
        cursor += 1;
    }

    let mut parse_base = base;
    if parse_base == 0 {
        parse_base = 10;
        if bytes.get(cursor) == Some(&b'0') {
            match bytes.get(cursor + 1).copied() {
                Some(b'x' | b'X') => {
                    parse_base = 16;
                    cursor += 2;
                }
                Some(b'b' | b'B') => {
                    parse_base = 2;
                    cursor += 2;
                }
                _ => {
                    parse_base = 8;
                    cursor += 1;
                }
            }
        }
    } else if (parse_base == 2 || parse_base == 16)
        && bytes.get(cursor) == Some(&b'0')
        && matches!(
            (parse_base, bytes.get(cursor + 1).copied()),
            (2, Some(b'b' | b'B')) | (16, Some(b'x' | b'X'))
        )
    {
        cursor += 2;
    }

    if !(2..=36).contains(&parse_base) {
        return 0;
    }

    let mut value = 0_i128;
    while let Some(byte) = bytes.get(cursor).copied() {
        let Some(digit) = ascii_digit_value(byte) else {
            break;
        };
        if i64::from(digit) >= parse_base {
            break;
        }
        value = value
            .saturating_mul(i128::from(parse_base))
            .saturating_add(i128::from(digit));
        cursor += 1;
    }

    let signed = if negative { -value } else { value };
    signed.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

fn ascii_digit_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'z' => Some(u32::from(byte - b'a') + 10),
        b'A'..=b'Z' => Some(u32::from(byte - b'A') + 10),
        _ => None,
    }
}

pub(in crate::builtins::modules) fn builtin_floatval(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("floatval", &args, 1)?;
    let value = args.into_iter().next().expect("checked arity");
    to_float(&value)
        .map(Value::float)
        .map_err(|message| conversion_error("floatval", message))
}

pub(in crate::builtins::modules) fn builtin_var_dump(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let serialize_precision = context
        .ini_get("serialize_precision")
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(-1);
    let mut formatter = DebugFormatter::with_serialize_precision(serialize_precision);
    for value in &args {
        formatter.write_var_dump_value(context.output(), value, 0);
    }
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_debug_zval_dump(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let serialize_precision = context
        .ini_get("serialize_precision")
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(-1);
    let mut formatter = DebugFormatter::with_serialize_precision(serialize_precision);
    for value in &args {
        formatter.write_debug_zval_dump_value(context.output(), value, 0);
    }
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_print_r(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin print_r expects one or two argument(s)",
        ));
    }
    let return_output = args
        .get(1)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("print_r", message))?
        .unwrap_or(false);
    let mut output = OutputBuffer::new();
    DebugFormatter::default().write_print_r_value(&mut output, &args[0], 0);
    if return_output {
        Ok(Value::string(output.into_bytes()))
    } else {
        context.output().write_bytes(output.as_bytes());
        Ok(Value::Bool(true))
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::builtins::modules) struct PackFormatSpec {
    pub(in crate::builtins::modules) code: u8,
    pub(in crate::builtins::modules) count: Option<usize>,
    pub(in crate::builtins::modules) label: Option<Vec<u8>>,
}

pub(in crate::builtins::modules) fn parse_pack_format(
    format: &[u8],
    allow_labels: bool,
) -> Result<Vec<PackFormatSpec>, BuiltinError> {
    let mut specs = Vec::new();
    let mut index = 0;
    while index < format.len() {
        if format[index].is_ascii_whitespace() || format[index] == b'/' {
            index += 1;
            continue;
        }

        let code = format[index];
        index += 1;
        let count_start = index;
        while index < format.len() && format[index].is_ascii_digit() {
            index += 1;
        }
        let count = if count_start == index {
            None
        } else {
            Some(parse_ascii_usize(
                if allow_labels { "unpack" } else { "pack" },
                &format[count_start..index],
                "count",
            )?)
        };

        let label = if allow_labels {
            let label_start = index;
            while index < format.len() && format[index] != b'/' {
                index += 1;
            }
            (label_start != index).then(|| format[label_start..index].to_vec())
        } else {
            None
        };

        specs.push(PackFormatSpec { code, count, label });
    }
    Ok(specs)
}

pub(in crate::builtins::modules) fn invalid_pack_format(_name: &str, code: u8) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        format!("Invalid format type {}", code as char),
    )
}

pub(in crate::builtins::modules) fn unpack_offset_error() -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        "unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)",
    )
}

pub(in crate::builtins::modules) fn pack_u32_bytes(code: u8, value: i64) -> [u8; 4] {
    match code {
        b'l' => (value as i32).to_le_bytes(),
        b'I' | b'V' => (value as u32).to_le_bytes(),
        _ => unreachable!("checked pack format"),
    }
}

pub(in crate::builtins::modules) fn unpack_u32_value(code: u8, bytes: &[u8]) -> i64 {
    let bytes: [u8; 4] = bytes.try_into().expect("checked unpack width");
    match code {
        b'l' => i64::from(i32::from_le_bytes(bytes)),
        b'I' | b'V' => i64::from(u32::from_le_bytes(bytes)),
        _ => unreachable!("checked unpack format"),
    }
}

pub(in crate::builtins::modules) fn unpack_result_key(
    spec: &PackFormatSpec,
    index: usize,
    next_numeric_key: &mut i64,
) -> ArrayKey {
    match &spec.label {
        Some(label) if !label.is_empty() && spec.count.unwrap_or(1) == 1 => {
            ArrayKey::String(PhpString::from_bytes(label.clone()))
        }
        Some(label) if !label.is_empty() => {
            let mut key = label.clone();
            key.extend_from_slice((index + 1).to_string().as_bytes());
            ArrayKey::String(PhpString::from_bytes(key))
        }
        _ => {
            let key = *next_numeric_key;
            *next_numeric_key += 1;
            ArrayKey::Int(key)
        }
    }
}

pub(in crate::builtins::modules) fn type_error(
    name: &str,
    expected: &str,
    actual: &Value,
) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_TYPE",
        format!(
            "builtin {name} expects {expected}, got {}",
            runtime_type_name(actual)
        ),
    )
}

pub(in crate::builtins::modules) fn value_error(name: &str, message: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        format!("builtin {name}: {message}"),
    )
}

pub(in crate::builtins::modules) fn argument_value_error(
    name: &str,
    argument: &str,
    message: &str,
) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        format!("{name}(): Argument {argument} {message}"),
    )
}

pub(in crate::builtins::modules) fn argument_type_error(
    name: &str,
    argument: &str,
    expected: &str,
    actual: &Value,
) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_TYPE",
        format!(
            "{name}(): Argument {argument} must be of type {expected}, {} given",
            php_argument_type_name(actual)
        ),
    )
}

pub(in crate::builtins::modules) fn conversion_error(name: &str, message: String) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_TYPE",
        format!("builtin {name} could not convert value: {message}"),
    )
}

pub(in crate::builtins::modules) fn string_arg(
    name: &str,
    value: &Value,
) -> Result<crate::PhpString, BuiltinError> {
    to_string(value).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("builtin {name} expects string-compatible value: {message}"),
        )
    })
}

pub(in crate::builtins::modules) fn string_needle_arg(
    name: &str,
    argument: &str,
    value: &Value,
) -> Result<crate::PhpString, BuiltinError> {
    match deref_value(value) {
        Value::Array(_) | Value::Resource(_) => {
            Err(argument_type_error(name, argument, "string", value))
        }
        _ => string_arg(name, value)
            .map_err(|_| argument_type_error(name, argument, "string", value)),
    }
}

pub(in crate::builtins::modules) fn strtr_string_arg(
    context: &mut BuiltinContext<'_>,
    value: &Value,
    argument: &str,
    nullable_signature_type: &str,
    span: RuntimeSourceSpan,
) -> Result<crate::PhpString, BuiltinError> {
    match deref_value(value) {
        Value::Null => {
            context.php_deprecation(
                "E_PHP_RUNTIME_STRTR_NULL_STRING_ARG",
                format!(
                    "strtr(): Passing null to parameter {argument} of type {nullable_signature_type} is deprecated"
                ),
                span,
            );
            Ok(crate::PhpString::from_bytes(Vec::new()))
        }
        Value::Array(_) | Value::Resource(_) => {
            Err(strtr_argument_type_error(argument, "string", value))
        }
        _ => string_arg("strtr", value)
            .map_err(|_| strtr_argument_type_error(argument, "string", value)),
    }
}

pub(in crate::builtins::modules) fn nullable_string_arg(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    argument: &str,
    nullable_signature_type: &str,
    span: RuntimeSourceSpan,
) -> Result<crate::PhpString, BuiltinError> {
    match deref_value(value) {
        Value::Null => {
            context.php_deprecation(
                format!("E_PHP_RUNTIME_{}_NULL_STRING_ARG", name.to_ascii_uppercase()),
                format!(
                    "{name}(): Passing null to parameter {argument} of type {nullable_signature_type} is deprecated"
                ),
                span,
            );
            Ok(crate::PhpString::from_bytes(Vec::new()))
        }
        Value::Array(_) | Value::Resource(_) => Err(argument_type_error(
            name,
            argument,
            nullable_signature_type,
            value,
        )),
        _ => string_arg(name, value)
            .map_err(|_| argument_type_error(name, argument, nullable_signature_type, value)),
    }
}

pub(in crate::builtins::modules) fn strtr_argument_type_error(
    argument: &str,
    expected: &str,
    actual: &Value,
) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_TYPE",
        format!(
            "strtr(): Argument {argument} must be of type {expected}, {} given",
            php_argument_type_name(actual)
        ),
    )
}

pub(in crate::builtins::modules) fn php_argument_type_name(value: &Value) -> String {
    match deref_value(value) {
        Value::Null | Value::Uninitialized => "null".to_owned(),
        Value::Bool(true) => "true".to_owned(),
        Value::Bool(false) => "false".to_owned(),
        Value::Int(_) => "int".to_owned(),
        Value::Float(_) => "float".to_owned(),
        Value::String(_) => "string".to_owned(),
        Value::Array(_) => "array".to_owned(),
        Value::Object(object) => object.display_name(),
        Value::Resource(_) => "resource".to_owned(),
        Value::Fiber(_) | Value::Generator(_) => "object".to_owned(),
        Value::Callable(_) => "callable".to_owned(),
        Value::Reference(_) => unreachable!("deref_value removes references"),
    }
}

pub(in crate::builtins::modules) fn string_cast_value(
    context: &mut BuiltinContext<'_>,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<crate::PhpString, String> {
    match value {
        Value::Array(_) => {
            context.php_warning(
                "E_PHP_RUNTIME_ARRAY_TO_STRING_WARNING",
                "Array to string conversion",
                span,
            );
            Ok(crate::PhpString::from_test_str("Array"))
        }
        Value::Reference(cell) => string_cast_value(context, &cell.get(), span),
        other => to_string(other),
    }
}

pub(in crate::builtins::modules) fn int_arg(
    name: &str,
    value: &Value,
) -> Result<i64, BuiltinError> {
    to_int(value).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("builtin {name} expects int-compatible value: {message}"),
        )
    })
}

pub(in crate::builtins::modules) fn printf_int_arg(
    name: &str,
    value: &Value,
    context: &mut BuiltinContext<'_>,
    span: RuntimeSourceSpan,
) -> Result<i64, BuiltinError> {
    match deref_value(value) {
        Value::Float(value) => {
            let value = value.to_f64();
            if value.is_finite() && !(i64::MIN as f64..=i64::MAX as f64).contains(&value) {
                context.php_warning(
                    "E_PHP_RUNTIME_PRINTF_FLOAT_TO_INT_RANGE",
                    format!(
                        "The float {} is not representable as an int, cast occurred",
                        php_float_warning_literal(value)
                    ),
                    span,
                );
                return Ok(wrapping_float_to_i64(value));
            }
            Ok(value as i64)
        }
        value => int_arg(name, &value),
    }
}

pub(in crate::builtins::modules) fn wrapping_float_to_i64(value: f64) -> i64 {
    let modulus = 18_446_744_073_709_551_616.0_f64;
    let remainder = value.abs().rem_euclid(modulus);
    let unsigned = remainder as u64;
    let signed = unsigned as i64;
    if value.is_sign_negative() {
        signed.wrapping_neg()
    } else {
        signed
    }
}

pub(in crate::builtins::modules) fn php_float_warning_literal(value: f64) -> String {
    let formatted = format!("{value:.1E}");
    let Some((mantissa, exponent)) = formatted.split_once('E') else {
        return formatted;
    };
    let exponent = exponent.parse::<i32>().unwrap_or(0);
    format!("{mantissa}E{exponent:+}")
}

pub(in crate::builtins::modules) fn float_arg(
    name: &str,
    value: &Value,
) -> Result<f64, BuiltinError> {
    to_float(value).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("builtin {name} expects float-compatible value: {message}"),
        )
    })
}

pub(in crate::builtins::modules) fn string_array_key(value: &str) -> ArrayKey {
    ArrayKey::String(crate::PhpString::from_test_str(value))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::builtins::modules) struct ParsedUrl {
    pub(in crate::builtins::modules) scheme: Option<Vec<u8>>,
    pub(in crate::builtins::modules) host: Option<Vec<u8>>,
    pub(in crate::builtins::modules) port: Option<i64>,
    pub(in crate::builtins::modules) user: Option<Vec<u8>>,
    pub(in crate::builtins::modules) pass: Option<Vec<u8>>,
    pub(in crate::builtins::modules) path: Option<Vec<u8>>,
    pub(in crate::builtins::modules) query: Option<Vec<u8>>,
    pub(in crate::builtins::modules) fragment: Option<Vec<u8>>,
}

pub(in crate::builtins::modules) fn parse_url_component(
    parsed: &ParsedUrl,
    component: i64,
) -> BuiltinResult {
    let value = match component {
        0 => parsed.scheme.clone().map(Value::string),
        1 => parsed.host.clone().map(Value::string),
        2 => parsed.port.map(Value::Int),
        3 => parsed.user.clone().map(Value::string),
        4 => parsed.pass.clone().map(Value::string),
        5 => parsed.path.clone().map(Value::string),
        6 => parsed.query.clone().map(Value::string),
        7 => parsed.fragment.clone().map(Value::string),
        other => {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_VALUE",
                format!("parse_url(): Argument #2 must be a valid URL component, {other} given"),
            ));
        }
    };
    Ok(value.unwrap_or(Value::Null))
}

pub(in crate::builtins::modules) fn insert_url_component(
    array: &mut PhpArray,
    key: &str,
    value: Option<Vec<u8>>,
) {
    if let Some(value) = value {
        array.insert(string_array_key(key), Value::string(value));
    }
}

pub(in crate::builtins::modules) fn parse_php_url(bytes: &[u8]) -> Option<ParsedUrl> {
    let mut parsed = ParsedUrl::default();
    let mut s = 0usize;
    let len = bytes.len();

    if let Some(colon) = find_byte(bytes, s, b':') {
        if colon != s {
            if !bytes[s..colon].iter().all(|byte| {
                byte.is_ascii_alphabetic()
                    || byte.is_ascii_digit()
                    || matches!(*byte, b'+' | b'.' | b'-')
            }) {
                if colon + 1 < len && colon < find_first_of(bytes, s, b"?#") {
                    return parse_php_url_port(bytes, s, colon, parsed);
                }
                if starts_with_at(bytes, s, b"//") {
                    s += 2;
                    return parse_php_url_host(bytes, s, parsed);
                }
                return Some(parse_php_url_path(bytes, s, parsed));
            }

            parsed.scheme = Some(url_component(bytes, s, colon));
            if colon + 1 == len {
                return Some(parsed);
            }

            if bytes[colon + 1] != b'/' {
                let mut p = colon + 1;
                while p < len && bytes[p].is_ascii_digit() {
                    p += 1;
                }
                if (p == len || bytes[p] == b'/') && p - colon < 7 {
                    parsed.scheme = None;
                    return parse_php_url_port(bytes, s, colon, parsed);
                }
                return Some(parse_php_url_path(bytes, colon + 1, parsed));
            }

            if colon + 2 < len && bytes[colon + 2] == b'/' {
                s = colon + 3;
                if parsed
                    .scheme
                    .as_deref()
                    .is_some_and(|scheme| scheme.eq_ignore_ascii_case(b"file"))
                    && colon + 3 < len
                    && bytes[colon + 3] == b'/'
                {
                    if colon + 5 < len && bytes[colon + 5] == b':' {
                        s = colon + 4;
                    }
                    return Some(parse_php_url_path(bytes, s, parsed));
                }
            } else {
                return Some(parse_php_url_path(bytes, colon + 1, parsed));
            }
        } else {
            return parse_php_url_port(bytes, s, colon, parsed);
        }
    } else if starts_with_at(bytes, s, b"//") {
        s += 2;
    } else {
        return Some(parse_php_url_path(bytes, s, parsed));
    }

    parse_php_url_host(bytes, s, parsed)
}

pub(in crate::builtins::modules) fn parse_php_url_port(
    bytes: &[u8],
    mut s: usize,
    colon: usize,
    mut parsed: ParsedUrl,
) -> Option<ParsedUrl> {
    let len = bytes.len();
    let p = colon + 1;
    let mut pp = p;
    while pp < len && pp - p < 6 && bytes[pp].is_ascii_digit() {
        pp += 1;
    }

    if pp > p && pp - p < 6 && (pp == len || bytes[pp] == b'/') {
        let port = parse_url_port(&bytes[p..pp])?;
        parsed.port = Some(port);
        if starts_with_at(bytes, s, b"//") {
            s += 2;
        }
    } else if p == pp && pp == len {
        return None;
    } else if starts_with_at(bytes, s, b"//") {
        s += 2;
    } else {
        return Some(parse_php_url_path(bytes, s, parsed));
    }

    parse_php_url_host(bytes, s, parsed)
}

pub(in crate::builtins::modules) fn parse_php_url_host(
    bytes: &[u8],
    mut s: usize,
    mut parsed: ParsedUrl,
) -> Option<ParsedUrl> {
    let len = bytes.len();
    let e = find_first_of(bytes, s, b"/?#");

    if let Some(at) = find_last_byte(&bytes[s..e], b'@').map(|offset| s + offset) {
        if let Some(pass_sep) = find_byte(bytes, s, b':').filter(|index| *index < at) {
            parsed.user = Some(url_component(bytes, s, pass_sep));
            parsed.pass = Some(url_component(bytes, pass_sep + 1, at));
        } else {
            parsed.user = Some(url_component(bytes, s, at));
        }
        s = at + 1;
    }

    let port_separator = if s < e && bytes[s] == b'[' && bytes[e - 1] == b']' {
        None
    } else {
        find_last_byte(&bytes[s..e], b':').map(|offset| s + offset)
    };
    let host_end = if let Some(port_separator) = port_separator {
        if parsed.port.is_none() {
            let port_start = port_separator + 1;
            if e - port_start > 5 {
                return None;
            }
            if port_start < e {
                parsed.port = Some(parse_url_port(&bytes[port_start..e])?);
            }
        }
        port_separator
    } else {
        e
    };

    if host_end <= s {
        return None;
    }

    parsed.host = Some(url_component(bytes, s, host_end));
    if e == len {
        Some(parsed)
    } else {
        Some(parse_php_url_path(bytes, e, parsed))
    }
}

pub(in crate::builtins::modules) fn parse_php_url_path(
    bytes: &[u8],
    s: usize,
    mut parsed: ParsedUrl,
) -> ParsedUrl {
    let len = bytes.len();
    let mut e = len;
    if let Some(fragment_start) = find_byte(bytes, s, b'#') {
        parsed.fragment = Some(url_component(bytes, fragment_start + 1, e));
        e = fragment_start;
    }
    if let Some(query_start) = find_byte_before(bytes, s, e, b'?') {
        parsed.query = Some(url_component(bytes, query_start + 1, e));
        e = query_start;
    }
    if s < e || s == len {
        parsed.path = Some(url_component(bytes, s, e));
    }
    parsed
}

pub(in crate::builtins::modules) fn parse_url_port(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || !php_source::byte_kernel::all_ascii_digits(bytes) {
        return None;
    }
    let value = std::str::from_utf8(bytes).ok()?.parse::<i64>().ok()?;
    (0..=65535).contains(&value).then_some(value)
}

pub(in crate::builtins::modules) fn url_component(
    bytes: &[u8],
    start: usize,
    end: usize,
) -> Vec<u8> {
    bytes[start..end]
        .iter()
        .map(|byte| if byte.is_ascii_control() { b'_' } else { *byte })
        .collect()
}

pub(in crate::builtins::modules) fn find_byte(
    bytes: &[u8],
    start: usize,
    needle: u8,
) -> Option<usize> {
    php_source::byte_kernel::find_byte(&bytes[start..], needle).map(|offset| start + offset)
}

pub(in crate::builtins::modules) fn find_byte_before(
    bytes: &[u8],
    start: usize,
    end: usize,
    needle: u8,
) -> Option<usize> {
    php_source::byte_kernel::find_byte(&bytes[start..end], needle).map(|offset| start + offset)
}

pub(in crate::builtins::modules) fn find_first_of(
    bytes: &[u8],
    start: usize,
    needles: &[u8],
) -> usize {
    match needles {
        [] => bytes.len(),
        [one] => find_byte(bytes, start, *one).unwrap_or(bytes.len()),
        [one, two] => php_source::byte_kernel::find_any2(&bytes[start..], *one, *two)
            .map_or(bytes.len(), |offset| start + offset),
        [one, two, three] => {
            php_source::byte_kernel::find_any3(&bytes[start..], *one, *two, *three)
                .map_or(bytes.len(), |offset| start + offset)
        }
        _ => bytes[start..]
            .iter()
            .position(|byte| needles.contains(byte))
            .map_or(bytes.len(), |offset| start + offset),
    }
}

pub(in crate::builtins::modules) fn find_last_byte(bytes: &[u8], needle: u8) -> Option<usize> {
    php_source::byte_kernel::rfind_byte(bytes, needle)
}

pub(in crate::builtins::modules) fn starts_with_at(
    bytes: &[u8],
    start: usize,
    needle: &[u8],
) -> bool {
    bytes
        .get(start..start.saturating_add(needle.len()))
        .is_some_and(|candidate| candidate == needle)
}

pub(in crate::builtins::modules) fn php_path_separators() -> &'static [char] {
    if cfg!(windows) { &['/', '\\'] } else { &['/'] }
}

pub(in crate::builtins::modules) fn trim_trailing_path_separators(path: &str) -> &str {
    let trimmed = path.trim_end_matches(php_path_separators());
    if trimmed.is_empty() && path.starts_with(php_path_separators()) {
        &path[..1]
    } else {
        trimmed
    }
}

pub(in crate::builtins::modules) fn php_basename(path: &str) -> String {
    let trimmed = trim_trailing_path_separators(path);
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .rsplit(php_path_separators())
        .next()
        .unwrap_or(trimmed)
        .to_owned()
}

pub(in crate::builtins::modules) fn php_dirname_once(path: &str) -> String {
    let trimmed = trim_trailing_path_separators(path);
    if trimmed.is_empty() {
        return String::new();
    }
    let Some(index) = trimmed.rfind(php_path_separators()) else {
        return ".".to_owned();
    };
    if index == 0 {
        return trimmed[..1].to_owned();
    }
    let parent = trimmed[..index].trim_end_matches(php_path_separators());
    if parent.is_empty() {
        ".".to_owned()
    } else {
        parent.to_owned()
    }
}

pub(in crate::builtins::modules) fn split_extension(basename: &str) -> (String, Option<String>) {
    let Some(index) = basename.rfind('.') else {
        return (basename.to_owned(), None);
    };
    (
        basename[..index].to_owned(),
        Some(basename[index + 1..].to_owned()),
    )
}

pub(in crate::builtins::modules) fn resolve_runtime_path(
    context: &BuiltinContext<'_>,
    path: &str,
) -> PathBuf {
    let raw = Path::new(path);
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        context.cwd().join(raw)
    };
    normalize_runtime_path(&joined)
}

pub(in crate::builtins::modules) fn normalize_runtime_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

pub(in crate::builtins::modules) fn metadata_for_arg(
    context: &BuiltinContext<'_>,
    name: &str,
    value: &Value,
    follow_links: bool,
) -> Result<Option<Metadata>, BuiltinError> {
    let path = string_arg(name, value)?.to_string_lossy();
    let resolved = resolve_runtime_path(context, &path);
    if !context.filesystem_capabilities().allows_path(&resolved) {
        return Ok(None);
    }
    let metadata = if follow_links {
        fs::metadata(&resolved)
    } else {
        fs::symlink_metadata(&resolved)
    };
    Ok(metadata.ok())
}

pub(in crate::builtins::modules) fn resource_arg(value: &Value) -> Option<crate::ResourceRef> {
    match deref_value(value) {
        Value::Resource(resource) => Some(resource),
        _ => None,
    }
}

pub(in crate::builtins::modules) fn read_file_value(
    context: &mut BuiltinContext<'_>,
    function: &str,
    path: &str,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if path.starts_with("php://") {
        let cwd = context.cwd().to_path_buf();
        let filesystem = context.filesystem_capabilities().clone();
        let php_input = context.php_input().to_vec();
        let Some(resources) = context.resources() else {
            context.php_warning(
                "E_PHP_RUNTIME_STREAM_RESOURCE_TABLE",
                format!("{function}({path}): Failed to open stream: resources unavailable"),
                span,
            );
            return Ok(Value::Bool(false));
        };
        let resource = match StreamWrapperRegistry::new().open(
            resources,
            path,
            "rb",
            &cwd,
            &filesystem,
            &php_input,
        ) {
            Ok(resource) => resource,
            Err(error) => {
                context.php_warning(
                    error.diagnostic_id(),
                    format!(
                        "{function}({path}): Failed to open stream: {}",
                        error.message()
                    ),
                    span,
                );
                return Ok(Value::Bool(false));
            }
        };
        return match resource.read_to_end() {
            Ok(bytes) => Ok(Value::string(bytes)),
            Err(error) => {
                context.php_warning(
                    error.diagnostic_id(),
                    format!(
                        "{function}({path}): Failed to open stream: {}",
                        error.message()
                    ),
                    span,
                );
                Ok(Value::Bool(false))
            }
        };
    }
    if crate::phar::is_phar_uri(path) {
        return match crate::phar::read_uri(path, context.cwd(), context.filesystem_capabilities()) {
            Ok(bytes) => Ok(Value::string(bytes)),
            Err(error) => {
                context.php_warning(
                    error.diagnostic_id(),
                    format!(
                        "{function}({path}): Failed to open stream: {}",
                        error.message()
                    ),
                    span,
                );
                Ok(Value::Bool(false))
            }
        };
    }
    let resolved = resolve_runtime_path(context, path);
    if !context.filesystem_capabilities().allows_path(&resolved) {
        context.php_warning(
            "E_PHP_FILESYSTEM_CAPABILITY_DENIED",
            format!("{function}({path}): Failed to open stream: Operation not permitted"),
            span,
        );
        return Ok(Value::Bool(false));
    }
    match fs::read(&resolved) {
        Ok(bytes) => Ok(Value::string(bytes)),
        Err(error) => {
            context.php_warning(
                "E_PHP_RUNTIME_STREAM_OPEN",
                format!(
                    "{function}({path}): Failed to open stream: {}",
                    php_io_error_message(&error)
                ),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn php_io_error_message(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "No such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "Permission denied".to_string(),
        std::io::ErrorKind::AlreadyExists => "File exists".to_string(),
        _ => error.to_string(),
    }
}

pub(in crate::builtins::modules) fn directory_entries_with_dots(
    path: &Path,
) -> Option<Vec<String>> {
    let mut entries = vec![".".to_string(), "..".to_string()];
    let read_dir = fs::read_dir(path).ok()?;
    let mut names = read_dir
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    names.sort();
    entries.extend(names);
    Some(entries)
}

pub(in crate::builtins::modules) fn glob_directory_and_pattern(
    context: &BuiltinContext<'_>,
    pattern: &str,
) -> (PathBuf, String) {
    let wildcard_index = pattern.find(['*', '?']).unwrap_or(pattern.len());
    let parent_end = pattern[..wildcard_index]
        .rfind(php_path_separators())
        .map_or(0, |index| index + 1);
    let (directory, file_pattern) = pattern.split_at(parent_end);
    let directory = if directory.is_empty() {
        context.cwd().to_path_buf()
    } else {
        resolve_runtime_path(context, directory)
    };
    (directory, file_pattern.to_string())
}

pub(in crate::builtins::modules) fn glob_pattern_matches(pattern: &str, name: &str) -> bool {
    fn matches_bytes(pattern: &[u8], name: &[u8]) -> bool {
        match pattern.split_first() {
            None => name.is_empty(),
            Some((&b'*', rest)) => {
                matches_bytes(rest, name)
                    || (!name.is_empty() && matches_bytes(pattern, &name[1..]))
            }
            Some((&b'?', rest)) => !name.is_empty() && matches_bytes(rest, &name[1..]),
            Some((&expected, rest)) => {
                name.first().copied() == Some(expected) && matches_bytes(rest, &name[1..])
            }
        }
    }
    matches_bytes(pattern.as_bytes(), name.as_bytes())
}

pub(in crate::builtins::modules) fn is_remote_stream_uri(uri: &str) -> bool {
    matches!(
        uri.split_once("://").map(|(scheme, _)| scheme),
        Some("http" | "https" | "ftp" | "ftps")
    )
}

pub(in crate::builtins::modules) fn php_value_to_json_checked(
    value: &Value,
    flags: i64,
    max_depth: usize,
) -> Result<(JsonValue, Option<i64>), i64> {
    let mut state = JsonEncodeState::new(flags, max_depth);
    let json = php_value_to_json_inner(value, flags, &mut state)?;
    Ok((json, state.first_error))
}

struct JsonEncodeState {
    partial: bool,
    first_error: Option<i64>,
    active_arrays: Vec<usize>,
    active_objects: Vec<u64>,
    active_references: Vec<usize>,
    depth: usize,
    max_depth: usize,
}

impl JsonEncodeState {
    const fn new(flags: i64, max_depth: usize) -> Self {
        Self {
            partial: flags & JSON_PARTIAL_OUTPUT_ON_ERROR != 0,
            first_error: None,
            active_arrays: Vec::new(),
            active_objects: Vec::new(),
            active_references: Vec::new(),
            depth: 0,
            max_depth,
        }
    }

    fn error_json(&mut self, code: i64) -> Result<JsonValue, i64> {
        if self.partial {
            self.first_error.get_or_insert(code);
            Ok(match code {
                JSON_ERROR_INF_OR_NAN => JsonValue::Number(JsonNumber::from(0)),
                _ => JsonValue::Null,
            })
        } else {
            Err(code)
        }
    }

    fn enter_nested(&mut self) -> Result<bool, i64> {
        self.depth = self.depth.saturating_add(1);
        if self.depth > self.max_depth {
            self.depth = self.depth.saturating_sub(1);
            if self.partial {
                self.first_error.get_or_insert(JSON_ERROR_DEPTH);
                Ok(false)
            } else {
                Err(JSON_ERROR_DEPTH)
            }
        } else {
            Ok(true)
        }
    }

    fn leave_nested(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

fn php_value_to_json_inner(
    value: &Value,
    flags: i64,
    state: &mut JsonEncodeState,
) -> Result<JsonValue, i64> {
    match value {
        Value::Reference(cell) => {
            let id = cell.gc_debug_id();
            if state.active_references.contains(&id) {
                return state.error_json(JSON_ERROR_RECURSION);
            }
            state.active_references.push(id);
            let referenced = cell.get();
            let json = php_value_to_json_inner(&referenced, flags, state);
            state.active_references.pop();
            json
        }
        _ => php_deref_value_to_json_inner(deref_value(value), flags, state),
    }
}

fn php_deref_value_to_json_inner(
    value: Value,
    flags: i64,
    state: &mut JsonEncodeState,
) -> Result<JsonValue, i64> {
    match value {
        Value::Null | Value::Uninitialized => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(value)),
        Value::Int(value) => Ok(JsonValue::Number(JsonNumber::from(value))),
        Value::Float(value) => {
            let value = value.to_f64();
            if !value.is_finite() {
                return state.error_json(JSON_ERROR_INF_OR_NAN);
            }
            if value.is_finite()
                && value.fract() == 0.0
                && flags & JSON_PRESERVE_ZERO_FRACTION == 0
                && value >= i64::MIN as f64
                && value <= i64::MAX as f64
            {
                Ok(JsonValue::Number(JsonNumber::from(value as i64)))
            } else {
                JsonNumber::from_f64(value)
                    .map(JsonValue::Number)
                    .ok_or(JSON_ERROR_INF_OR_NAN)
            }
        }
        Value::String(value) => {
            if flags & JSON_NUMERIC_CHECK != 0 {
                match classify_php_string(&value) {
                    classified
                        if matches!(classified.kind, NumericStringKind::IntString)
                            && matches!(classified.value, Some(NumericStringValue::Int(_))) =>
                    {
                        if let Some(NumericStringValue::Int(value)) = classified.value {
                            return Ok(JsonValue::Number(JsonNumber::from(value)));
                        }
                    }
                    classified
                        if matches!(classified.kind, NumericStringKind::FloatString)
                            && matches!(classified.value, Some(NumericStringValue::Float(_))) =>
                    {
                        if let Some(NumericStringValue::Float(value)) = classified.value
                            && value.is_finite()
                        {
                            return JsonNumber::from_f64(value)
                                .map(JsonValue::Number)
                                .ok_or(JSON_ERROR_SYNTAX);
                        }
                    }
                    _ => {}
                }
            }
            match json_string_from_php_bytes(value.as_bytes(), flags) {
                Ok(text) => Ok(JsonValue::String(text)),
                Err(code) => state.error_json(code),
            }
        }
        Value::Array(array) => {
            if !state.enter_nested()? {
                return Ok(JsonValue::Null);
            }
            let id = array.gc_debug_id();
            if state.active_arrays.contains(&id) {
                state.leave_nested();
                return state.error_json(JSON_ERROR_RECURSION);
            }
            state.active_arrays.push(id);
            if flags & JSON_FORCE_OBJECT == 0
                && let Some(elements) = array.packed_elements()
            {
                let json = elements
                    .into_iter()
                    .map(|value| php_value_to_json_inner(value, flags, state))
                    .collect::<Result<Vec<_>, _>>()
                    .map(JsonValue::Array);
                state.active_arrays.pop();
                state.leave_nested();
                json
            } else {
                let mut object = JsonMap::new();
                for (key, value) in array.iter() {
                    let key = match key {
                        ArrayKey::Int(value) => value.to_string(),
                        ArrayKey::String(value) => {
                            json_key_from_php_bytes(value.as_bytes(), flags, state)?
                        }
                    };
                    object.insert(key, php_value_to_json_inner(value, flags, state)?);
                }
                state.active_arrays.pop();
                state.leave_nested();
                Ok(JsonValue::Object(object))
            }
        }
        Value::Object(object) => {
            if !state.enter_nested()? {
                return Ok(JsonValue::Null);
            }
            let id = object.id();
            if state.active_objects.contains(&id) {
                state.leave_nested();
                return state.error_json(JSON_ERROR_RECURSION);
            }
            state.active_objects.push(id);
            if let Some(json) = spl_fixed_array_to_json(&object, flags, state) {
                state.active_objects.pop();
                state.leave_nested();
                return json;
            }
            let mut json = JsonMap::new();
            for (name, value) in object.properties_snapshot() {
                let label = object.property_debug_label(&name);
                if label.contains(":private") || label.contains(":protected") {
                    continue;
                }
                json.insert(name, php_value_to_json_inner(&value, flags, state)?);
            }
            state.active_objects.pop();
            state.leave_nested();
            Ok(JsonValue::Object(json))
        }
        Value::Resource(_)
        | Value::Fiber(_)
        | Value::Generator(_)
        | Value::Callable(_)
        | Value::Reference(_) => state.error_json(JSON_ERROR_UNSUPPORTED_TYPE),
    }
}

fn json_string_from_php_bytes(bytes: &[u8], flags: i64) -> Result<String, i64> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_string()),
        Err(_) if flags & JSON_INVALID_UTF8_IGNORE != 0 => Ok(utf8_ignore_invalid(bytes)),
        Err(_) if flags & JSON_INVALID_UTF8_SUBSTITUTE != 0 => Ok(utf8_substitute_invalid(bytes)),
        Err(_) => Err(JSON_ERROR_UTF8),
    }
}

fn json_key_from_php_bytes(
    bytes: &[u8],
    flags: i64,
    state: &mut JsonEncodeState,
) -> Result<String, i64> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_string()),
        Err(_) if flags & JSON_INVALID_UTF8_IGNORE != 0 => Ok(utf8_ignore_invalid(bytes)),
        Err(_) if flags & JSON_INVALID_UTF8_SUBSTITUTE != 0 => Ok(utf8_substitute_invalid(bytes)),
        Err(_) if state.partial => {
            state.first_error.get_or_insert(JSON_ERROR_UTF8);
            Ok(String::new())
        }
        Err(_) => Err(JSON_ERROR_UTF8),
    }
}

pub(in crate::builtins::modules) fn utf8_ignore_invalid(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                out.push_str(valid);
                break;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0
                    && let Ok(valid_prefix) = std::str::from_utf8(&rest[..valid_up_to])
                {
                    out.push_str(valid_prefix);
                }
                let skip = error.error_len().unwrap_or(1);
                rest = &rest[valid_up_to.saturating_add(skip)..];
            }
        }
    }
    out
}

fn utf8_substitute_invalid(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                out.push_str(valid);
                break;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0
                    && let Ok(valid_prefix) = std::str::from_utf8(&rest[..valid_up_to])
                {
                    out.push_str(valid_prefix);
                }
                out.push('\u{fffd}');
                let invalid = &rest[valid_up_to..];
                let first = invalid.first().copied().unwrap_or_default();
                let mut skip = error.error_len().unwrap_or(1);
                if first >= 0xc0 {
                    while skip < invalid.len() && (invalid[skip] & 0xc0) == 0x80 {
                        skip += 1;
                    }
                }
                rest = &invalid[skip..];
            }
        }
    }
    out
}

fn spl_fixed_array_to_json(
    object: &ObjectRef,
    flags: i64,
    state: &mut JsonEncodeState,
) -> Option<Result<JsonValue, i64>> {
    if !object.class_name().eq_ignore_ascii_case("splfixedarray") {
        return None;
    }
    let Some(Value::Array(entries)) = object.get_property("__entries") else {
        return Some(Ok(JsonValue::Array(Vec::new())));
    };

    let mut indexed_entries = Vec::new();
    for (_, entry) in entries.iter() {
        let Value::Array(pair) = deref_value(entry) else {
            continue;
        };
        let index = match pair.get(&ArrayKey::Int(0)).map(deref_value) {
            Some(Value::Int(index)) if index >= 0 => index as usize,
            _ => continue,
        };
        let value = pair.get(&ArrayKey::Int(1)).cloned().unwrap_or(Value::Null);
        indexed_entries.push((index, value));
    }

    let size = indexed_entries
        .iter()
        .map(|(index, _)| *index)
        .max()
        .map_or(0, |index| index.saturating_add(1));
    let mut elements = vec![JsonValue::Null; size];
    for (index, value) in indexed_entries {
        elements[index] = match php_value_to_json_inner(&value, flags, state) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
    }
    Some(Ok(JsonValue::Array(elements)))
}

pub(in crate::builtins::modules) fn json_to_php_value(
    value: JsonValue,
    associative: bool,
) -> Value {
    match value {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(value) => Value::Bool(value),
        JsonValue::Number(value) => value
            .as_i64()
            .map(Value::Int)
            .or_else(|| value.as_f64().map(Value::float))
            .or_else(|| value.to_string().parse::<f64>().ok().map(Value::float))
            .unwrap_or(Value::Null),
        JsonValue::String(value) => Value::string(value),
        JsonValue::Array(values) => Value::packed_array(
            values
                .into_iter()
                .map(|value| json_to_php_value(value, associative))
                .collect(),
        ),
        JsonValue::Object(values) if associative => {
            let mut array = crate::PhpArray::new();
            for (key, value) in values {
                array.insert(
                    ArrayKey::String(PhpString::from_test_str(&key)),
                    json_to_php_value(value, associative),
                );
            }
            Value::Array(array)
        }
        JsonValue::Object(values) => {
            let object = ObjectRef::new_with_display_name(&json_std_class(), "stdClass");
            for (key, value) in values {
                object.set_property(key, json_to_php_value(value, associative));
            }
            Value::Object(object)
        }
    }
}

pub(in crate::builtins::modules) fn normalize_json_encoded(
    mut encoded: String,
    flags: i64,
) -> String {
    if flags & JSON_PRETTY_PRINT != 0 {
        encoded = json_pretty_indent_for_php(&encoded);
    }

    if flags & JSON_UNESCAPED_SLASHES == 0 {
        encoded = encoded.replace('/', "\\/");
    }

    if flags & JSON_UNESCAPED_UNICODE == 0 {
        encoded = escape_json_non_ascii(&encoded);
    } else if flags & JSON_UNESCAPED_LINE_TERMINATORS == 0 {
        encoded = escape_json_line_terminators(&encoded);
    }

    if flags & JSON_HEX_TAG != 0 {
        encoded = encoded.replace('<', "\\u003C").replace('>', "\\u003E");
    }
    if flags & JSON_HEX_AMP != 0 {
        encoded = encoded.replace('&', "\\u0026");
    }
    if flags & JSON_HEX_APOS != 0 {
        encoded = encoded.replace('\'', "\\u0027");
    }
    if flags & JSON_HEX_QUOT != 0 {
        encoded = encoded.replace("\\\"", "\\u0022");
    }

    // serde_json preserves the decimal marker for finite PHP floats, so this
    // flag is an explicit no-op after value conversion above.
    let _ = flags & JSON_PRESERVE_ZERO_FRACTION;
    encoded
}

fn escape_json_non_ascii(encoded: &str) -> String {
    let mut normalized = String::with_capacity(encoded.len());
    for ch in encoded.chars() {
        if ch.is_ascii() {
            normalized.push(ch);
            continue;
        }
        if matches!(ch, '\u{2028}' | '\u{2029}') {
            normalized.push_str(match ch {
                '\u{2028}' => "\\u2028",
                '\u{2029}' => "\\u2029",
                _ => unreachable!(),
            });
            continue;
        }
        let code = ch as u32;
        if code <= 0xFFFF {
            normalized.push_str(&format!("\\u{code:04x}"));
        } else {
            let code = code - 0x1_0000;
            let high = 0xD800 + ((code >> 10) & 0x3FF);
            let low = 0xDC00 + (code & 0x3FF);
            normalized.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
        }
    }
    normalized
}

fn escape_json_line_terminators(encoded: &str) -> String {
    encoded
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn json_pretty_indent_for_php(encoded: &str) -> String {
    let mut normalized = String::with_capacity(encoded.len());
    for (index, line) in encoded.split('\n').enumerate() {
        if index > 0 {
            normalized.push('\n');
        }
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        for _ in 0..indent * 2 {
            normalized.push(' ');
        }
        normalized.push_str(&line[indent..]);
    }
    normalized
}

pub(in crate::builtins::modules) fn compile_preg_pattern(
    context: &mut BuiltinContext<'_>,
    pattern: PhpString,
) -> Option<std::sync::Arc<pcre::CompiledPattern>> {
    match context.pcre_cache().compile(&pattern) {
        Ok(compiled) => Some(compiled),
        Err(error) => {
            context.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
            None
        }
    }
}

pub(in crate::builtins::modules) fn preg_failure(
    context: &mut BuiltinContext<'_>,
    error: pcre::PcreFailure,
) -> BuiltinResult {
    context.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
    Ok(Value::Bool(false))
}

pub(in crate::builtins::modules) fn assign_reference_arg(argument: Option<&Value>, value: Value) {
    if let Some(Value::Reference(reference)) = argument {
        reference.set(value);
    }
}

pub(in crate::builtins::modules) fn pattern_order_matches(
    matches: Vec<Value>,
    capture_count: usize,
) -> Value {
    let mut grouped: Vec<PhpArray> = std::iter::repeat_with(PhpArray::new)
        .take(capture_count)
        .collect();
    for match_value in matches {
        let Value::Array(captures) = match_value else {
            continue;
        };
        for (key, value) in captures.iter() {
            let ArrayKey::Int(index) = key else {
                continue;
            };
            let index = index as usize;
            while grouped.len() <= index {
                grouped.push(PhpArray::new());
            }
            grouped[index].append(value.clone());
        }
    }
    Value::packed_array(grouped.into_iter().map(Value::Array).collect())
}

pub(in crate::builtins::modules) fn preg_replace_subject_with_specs(
    specs: &[(std::sync::Arc<pcre::CompiledPattern>, Vec<u8>)],
    subject: &Value,
    limit: i64,
    count: &mut i64,
) -> Result<Value, pcre::PcreFailure> {
    match deref_value(subject) {
        Value::Array(array) => {
            let mut output = PhpArray::new();
            for (key, value) in array.iter() {
                let text = to_string(value).map_err(|message| {
                    pcre::PcreFailure::new(pcre::PREG_INTERNAL_ERROR, message)
                })?;
                let replaced = preg_replace_bytes_with_specs(specs, text.as_bytes(), limit, count)?;
                output.insert(key.clone(), Value::string(replaced));
            }
            Ok(Value::Array(output))
        }
        value => {
            let text = to_string(&value)
                .map_err(|message| pcre::PcreFailure::new(pcre::PREG_INTERNAL_ERROR, message))?;
            preg_replace_bytes_with_specs(specs, text.as_bytes(), limit, count).map(Value::string)
        }
    }
}

pub(in crate::builtins::modules) fn preg_replace_filter_subject_with_specs(
    specs: &[(std::sync::Arc<pcre::CompiledPattern>, Vec<u8>)],
    subject: &Value,
    limit: i64,
    count: &mut i64,
) -> Result<Value, pcre::PcreFailure> {
    match deref_value(subject) {
        Value::Array(array) => {
            let mut output = PhpArray::new();
            for (key, value) in array.iter() {
                let text = to_string(value).map_err(|message| {
                    pcre::PcreFailure::new(pcre::PREG_INTERNAL_ERROR, message)
                })?;
                let before = *count;
                let replaced = preg_replace_bytes_with_specs(specs, text.as_bytes(), limit, count)?;
                if *count > before {
                    output.insert(key.clone(), Value::string(replaced));
                }
            }
            Ok(Value::Array(output))
        }
        value => {
            let text = to_string(&value)
                .map_err(|message| pcre::PcreFailure::new(pcre::PREG_INTERNAL_ERROR, message))?;
            let before = *count;
            let replaced = preg_replace_bytes_with_specs(specs, text.as_bytes(), limit, count)?;
            Ok(if *count > before {
                Value::string(replaced)
            } else {
                Value::Null
            })
        }
    }
}

fn preg_replace_bytes_with_specs(
    specs: &[(std::sync::Arc<pcre::CompiledPattern>, Vec<u8>)],
    subject: &[u8],
    limit: i64,
    count: &mut i64,
) -> Result<Vec<u8>, pcre::PcreFailure> {
    let mut output = subject.to_vec();
    for (compiled, replacement) in specs {
        output = preg_replace_bytes(compiled, replacement, &output, limit, count)?;
    }
    Ok(output)
}

pub(in crate::builtins::modules) fn preg_replace_bytes(
    compiled: &pcre::CompiledPattern,
    replacement: &[u8],
    subject: &[u8],
    limit: i64,
    count: &mut i64,
) -> Result<Vec<u8>, pcre::PcreFailure> {
    let mut output = Vec::new();
    let mut last_end = 0usize;
    let mut local_count = 0i64;
    for captures in compiled.captures_iter(subject) {
        let captures = captures.map_err(pcre::PcreFailure::from)?;
        let Some(full) = captures.get(0) else {
            continue;
        };
        if limit >= 0 && local_count >= limit {
            break;
        }
        output.extend_from_slice(&subject[last_end..full.start()]);
        output.extend_from_slice(&expand_preg_replacement(replacement, &captures));
        last_end = full.end();
        local_count += 1;
        *count += 1;
    }
    output.extend_from_slice(&subject[last_end..]);
    Ok(output)
}

pub(in crate::builtins::modules) fn preg_replace_callback_subject(
    context: &mut BuiltinContext<'_>,
    compiled: &pcre::CompiledPattern,
    callback: BuiltinEntry,
    subject: &Value,
    limit: i64,
    flags: i64,
    count: &mut i64,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    match deref_value(subject) {
        Value::Array(array) => {
            let mut output = PhpArray::new();
            for (key, value) in array.iter() {
                let text = to_string(value)
                    .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TYPE_ERROR", message))?;
                let replaced = preg_replace_callback_bytes(
                    context,
                    compiled,
                    callback,
                    text.as_bytes(),
                    limit,
                    flags,
                    count,
                    span.clone(),
                )?;
                output.insert(key.clone(), Value::string(replaced));
            }
            Ok(Value::Array(output))
        }
        value => {
            let text = to_string(&value)
                .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TYPE_ERROR", message))?;
            preg_replace_callback_bytes(
                context,
                compiled,
                callback,
                text.as_bytes(),
                limit,
                flags,
                count,
                span,
            )
            .map(Value::string)
        }
    }
}

pub(in crate::builtins::modules) fn preg_replace_callback_bytes(
    context: &mut BuiltinContext<'_>,
    compiled: &pcre::CompiledPattern,
    callback: BuiltinEntry,
    subject: &[u8],
    limit: i64,
    flags: i64,
    count: &mut i64,
    span: RuntimeSourceSpan,
) -> Result<Vec<u8>, BuiltinError> {
    let mut output = Vec::new();
    let mut last_end = 0usize;
    let mut local_count = 0i64;
    for captures in compiled.captures_iter(subject) {
        let captures = captures.map_err(|error| {
            let error = pcre::PcreFailure::from(error);
            BuiltinError::new("E_PHP_RUNTIME_PCRE_ERROR", error.message().to_string())
        })?;
        let Some(full) = captures.get(0) else {
            continue;
        };
        if limit >= 0 && local_count >= limit {
            break;
        }
        output.extend_from_slice(&subject[last_end..full.start()]);
        let callback_result = (callback.function())(
            context,
            vec![pcre::captures_to_array_with_names(
                &captures,
                compiled.capture_names(),
                flags,
                0,
            )],
            span.clone(),
        )?;
        let callback_text = to_string(&callback_result)
            .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TYPE_ERROR", message))?;
        output.extend_from_slice(callback_text.as_bytes());
        last_end = full.end();
        local_count += 1;
        *count += 1;
    }
    output.extend_from_slice(&subject[last_end..]);
    Ok(output)
}

pub(in crate::builtins::modules) fn expand_preg_replacement(
    replacement: &[u8],
    captures: &pcre2::bytes::Captures<'_>,
) -> Vec<u8> {
    let mut output = Vec::new();
    let mut index = 0usize;
    while index < replacement.len() {
        let byte = replacement[index];
        if (byte == b'$' || byte == b'\\') && index + 1 < replacement.len() {
            if byte == b'$'
                && let Some((capture_index, consumed)) =
                    parse_braced_preg_replacement_capture(replacement, index + 1)
            {
                append_preg_replacement_capture(&mut output, captures, capture_index);
                index += consumed + 1;
                continue;
            }
            if let Some((capture_index, consumed)) =
                parse_unbraced_preg_replacement_capture(replacement, index + 1)
            {
                append_preg_replacement_capture(&mut output, captures, capture_index);
                index += consumed + 1;
                continue;
            }
        }
        output.push(byte);
        index += 1;
    }
    output
}

fn parse_unbraced_preg_replacement_capture(
    replacement: &[u8],
    start: usize,
) -> Option<(usize, usize)> {
    let first = *replacement.get(start)?;
    if !first.is_ascii_digit() {
        return None;
    }
    let mut capture_index = (first - b'0') as usize;
    let mut consumed = 1usize;
    if let Some(second) = replacement.get(start + 1).copied()
        && second.is_ascii_digit()
    {
        capture_index = capture_index * 10 + (second - b'0') as usize;
        consumed = 2;
    }
    Some((capture_index, consumed))
}

fn parse_braced_preg_replacement_capture(
    replacement: &[u8],
    start: usize,
) -> Option<(usize, usize)> {
    if replacement.get(start).copied()? != b'{' {
        return None;
    }
    let first = replacement.get(start + 1).copied()?;
    if !first.is_ascii_digit() {
        return None;
    }
    let mut capture_index = (first - b'0') as usize;
    let mut digit_count = 1usize;
    if let Some(second) = replacement.get(start + 2).copied()
        && second.is_ascii_digit()
    {
        capture_index = capture_index * 10 + (second - b'0') as usize;
        digit_count = 2;
    }
    if replacement.get(start + 1 + digit_count).copied() != Some(b'}') {
        return None;
    }
    Some((capture_index, digit_count + 2))
}

fn append_preg_replacement_capture(
    output: &mut Vec<u8>,
    captures: &pcre2::bytes::Captures<'_>,
    capture_index: usize,
) {
    if let Some(capture) = captures.get(capture_index) {
        output.extend_from_slice(capture.as_bytes());
    }
}

pub(in crate::builtins::modules) fn append_split_piece(
    array: &mut PhpArray,
    bytes: &[u8],
    offset: usize,
    flags: i64,
) {
    if flags & pcre::PREG_SPLIT_NO_EMPTY != 0 && bytes.is_empty() {
        return;
    }
    let value = if flags & pcre::PREG_SPLIT_OFFSET_CAPTURE != 0 {
        Value::packed_array(vec![
            Value::string(bytes.to_vec()),
            Value::Int(offset as i64),
        ])
    } else {
        Value::string(bytes.to_vec())
    };
    array.append(value);
}

pub(in crate::builtins::modules) fn json_failure(
    context: &mut BuiltinContext<'_>,
    flags: i64,
    code: i64,
) -> BuiltinResult {
    if flags & JSON_THROW_ON_ERROR != 0 {
        Err(
            BuiltinError::new("E_PHP_RUNTIME_JSON_EXCEPTION", json_error_message(code))
                .with_json_error_code(code),
        )
    } else {
        context.set_json_last_error(code);
        Ok(Value::Bool(false))
    }
}

pub(in crate::builtins::modules) fn json_std_class() -> ClassEntry {
    ClassEntry {
        name: normalize_class_name("stdClass"),
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

pub(in crate::builtins::modules) fn metadata_mtime(metadata: &Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(unix)]
pub(in crate::builtins::modules) fn metadata_mode(metadata: &Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;

    metadata.mode()
}

#[cfg(not(unix))]
pub(in crate::builtins::modules) fn metadata_mode(metadata: &Metadata) -> u32 {
    let file_type = if metadata.is_dir() {
        0o040000
    } else if metadata.is_file() {
        0o100000
    } else {
        0
    };
    let permissions = if metadata.permissions().readonly() {
        0o444
    } else if metadata.is_dir() {
        0o777
    } else {
        0o666
    };
    file_type | permissions
}

#[cfg(unix)]
pub(in crate::builtins::modules) fn metadata_owner(metadata: &Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;

    metadata.uid()
}

#[cfg(not(unix))]
pub(in crate::builtins::modules) fn metadata_owner(_metadata: &Metadata) -> u32 {
    0
}

#[cfg(unix)]
pub(in crate::builtins::modules) fn metadata_group(metadata: &Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;

    metadata.gid()
}

#[cfg(not(unix))]
pub(in crate::builtins::modules) fn metadata_group(_metadata: &Metadata) -> u32 {
    0
}

#[cfg(unix)]
pub(in crate::builtins::modules) fn set_permissions_mode(
    path: &Path,
    mode: u32,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(mode & 0o7777);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
pub(in crate::builtins::modules) fn set_permissions_mode(
    path: &Path,
    mode: u32,
) -> std::io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(mode & 0o222 == 0);
    fs::set_permissions(path, permissions)
}

pub(in crate::builtins::modules) fn file_type_name(metadata: &Metadata) -> &'static str {
    let file_type = metadata.file_type();
    if file_type.is_file() {
        "file"
    } else if file_type.is_dir() {
        "dir"
    } else if file_type.is_symlink() {
        "link"
    } else {
        "unknown"
    }
}

pub(in crate::builtins::modules) fn stat_array(metadata: Metadata) -> Value {
    let size = metadata.len() as i64;
    let mtime = metadata_mtime(&metadata);
    let mode = metadata_mode(&metadata) as i64;
    let mut array = crate::PhpArray::new();
    array.insert(ArrayKey::Int(2), Value::Int(mode));
    array.insert(ArrayKey::Int(7), Value::Int(size));
    array.insert(ArrayKey::Int(9), Value::Int(mtime));
    array.insert(string_array_key("mode"), Value::Int(mode));
    array.insert(string_array_key("size"), Value::Int(size));
    array.insert(string_array_key("mtime"), Value::Int(mtime));
    array.insert(
        string_array_key("type"),
        Value::string(file_type_name(&metadata)),
    );
    Value::Array(array)
}

pub(in crate::builtins::modules) fn numeric_f64_arg(
    name: &str,
    value: &Value,
) -> Result<f64, BuiltinError> {
    to_number(value)
        .map(|number| number.as_f64())
        .map_err(|message| conversion_error(name, message))
}

pub(in crate::builtins::modules) fn min_max_builtin(
    name: &str,
    args: Vec<Value>,
    pick_max: bool,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error(name, "at least one argument"));
    }
    let values = if args.len() == 1 {
        match &args[0] {
            Value::Array(array) => array
                .iter()
                .map(|(_, value)| value.clone())
                .collect::<Vec<_>>(),
            _ => return Err(argument_type_error(name, "#1 ($value)", "array", &args[0])),
        }
    } else {
        args
    };
    if values.is_empty() {
        return Err(argument_value_error(
            name,
            "#1 ($value)",
            "must contain at least one element",
        ));
    }
    let mut selected = values[0].clone();
    for value in values.into_iter().skip(1) {
        let ordering =
            compare(&value, &selected).map_err(|message| conversion_error(name, message))?;
        if (pick_max && ordering.is_gt()) || (!pick_max && ordering.is_lt()) {
            selected = value;
        }
    }
    Ok(selected)
}

pub(in crate::builtins::modules) fn group_decimal_integer(
    integer: &str,
    separator: &str,
) -> String {
    if separator.is_empty() || integer.len() <= 3 {
        return integer.to_owned();
    }
    let mut grouped = String::with_capacity(integer.len() + separator.len() * (integer.len() / 3));
    let first_group = integer.len() % 3;
    if first_group != 0 {
        grouped.push_str(&integer[..first_group]);
    }
    for chunk_start in (first_group..integer.len()).step_by(3) {
        if !grouped.is_empty() {
            grouped.push_str(separator);
        }
        grouped.push_str(&integer[chunk_start..chunk_start + 3]);
    }
    grouped
}

pub(in crate::builtins::modules) fn normalize_offset(len: usize, offset: i64) -> usize {
    if offset >= 0 {
        (offset as usize).min(len)
    } else {
        len.saturating_sub(offset.unsigned_abs() as usize)
    }
}

pub(in crate::builtins::modules) fn checked_search_offset(
    name: &str,
    len: usize,
    offset: i64,
) -> Result<usize, BuiltinError> {
    let abs = offset.unsigned_abs() as usize;
    if offset > len as i64 || (offset < 0 && abs > len) {
        return Err(value_error(name, "offset is out of range"));
    }
    Ok(normalize_offset(len, offset))
}

pub(in crate::builtins::modules) fn byte_substring_length(
    name: &str,
    total: usize,
    start: usize,
    length: Option<i64>,
) -> Result<usize, BuiltinError> {
    match length {
        None => Ok(total.saturating_sub(start)),
        Some(length) if length >= 0 => Ok((length as usize).min(total.saturating_sub(start))),
        Some(length) => {
            let trim = length.unsigned_abs() as usize;
            if trim > total.saturating_sub(start) {
                return Err(value_error(name, "length is out of range"));
            }
            Ok(total.saturating_sub(start).saturating_sub(trim))
        }
    }
}

pub(in crate::builtins::modules) fn string_search_slice(
    context: &mut BuiltinContext<'_>,
    name: &str,
    args: Vec<Value>,
    case_insensitive: bool,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error(name, "two or three argument(s)"));
    }
    let haystack = nullable_string_arg(
        context,
        name,
        &args[0],
        "#1 ($haystack)",
        "string",
        span.clone(),
    )?;
    let needle = nullable_string_arg(
        context,
        name,
        &args[1],
        "#2 ($needle)",
        "string",
        span.clone(),
    )?;
    let before_needle = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error(name, message))?
        .unwrap_or(false);
    if needle.is_empty() {
        return Ok(if before_needle {
            Value::string(Vec::new())
        } else {
            Value::string(haystack.as_bytes().to_vec())
        });
    }
    Ok(
        find_bytes_from(haystack.as_bytes(), needle.as_bytes(), 0, case_insensitive).map_or(
            Value::Bool(false),
            |index| {
                if before_needle {
                    Value::string(haystack.as_bytes()[..index].to_vec())
                } else {
                    Value::string(haystack.as_bytes()[index..].to_vec())
                }
            },
        ),
    )
}

pub(in crate::builtins::modules) fn string_span(
    name: &str,
    args: Vec<Value>,
    accepted: bool,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error(name, "two to four argument(s)"));
    }
    let input = string_arg(name, &args[0])?;
    let mask = string_arg(name, &args[1])?;
    let offset = args
        .get(2)
        .map(|value| position_offset_arg(name, value))
        .transpose()?
        .unwrap_or(0);
    let start = string_span_offset(input.len(), offset);
    let length = args.get(3).map(|value| int_arg(name, value)).transpose()?;
    let scan_len = string_span_length(input.len(), start, length);
    let scan = &input.as_bytes()[start..start + scan_len];
    let count = scan
        .iter()
        .take_while(|byte| mask.as_bytes().contains(byte) == accepted)
        .count();
    Ok(Value::Int(count as i64))
}

pub(in crate::builtins::modules) fn string_span_offset(len: usize, offset: i64) -> usize {
    if offset >= 0 {
        (offset as usize).min(len)
    } else {
        len.saturating_sub(offset.unsigned_abs() as usize)
    }
}

pub(in crate::builtins::modules) fn string_span_length(
    total: usize,
    start: usize,
    length: Option<i64>,
) -> usize {
    let remaining = total.saturating_sub(start);
    match length {
        None => remaining,
        Some(length) if length >= 0 => (length as usize).min(remaining),
        Some(length) => remaining.saturating_sub(length.unsigned_abs() as usize),
    }
}

pub(in crate::builtins::modules) fn string_position(
    context: &mut BuiltinContext<'_>,
    name: &str,
    args: Vec<Value>,
    case_insensitive: bool,
    reverse: bool,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            format!("builtin {name} expects two or three argument(s)"),
        ));
    }
    let haystack = nullable_string_arg(
        context,
        name,
        &args[0],
        "#1 ($haystack)",
        "string",
        span.clone(),
    )?;
    let needle = nullable_string_arg(
        context,
        name,
        &args[1],
        "#2 ($needle)",
        "string",
        span.clone(),
    )?;
    let offset = args
        .get(2)
        .map(|value| position_offset_arg(name, value))
        .transpose()?
        .unwrap_or(0);
    let start = checked_search_offset(name, haystack.len(), offset)
        .map_err(|_| position_offset_error(name))?;
    let result = if reverse {
        rfind_bytes(
            haystack.as_bytes(),
            needle.as_bytes(),
            start,
            offset >= 0,
            case_insensitive,
        )
    } else {
        find_bytes_from(
            haystack.as_bytes(),
            needle.as_bytes(),
            start,
            case_insensitive,
        )
    };
    Ok(result.map_or(Value::Bool(false), |index| Value::Int(index as i64)))
}

pub(in crate::builtins::modules) fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes_from(haystack, needle, 0, false)
}

pub(in crate::builtins::modules) fn find_bytes_from(
    haystack: &[u8],
    needle: &[u8],
    start: usize,
    case_insensitive: bool,
) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(haystack.len()));
    }
    if start > haystack.len() || needle.len() > haystack.len().saturating_sub(start) {
        return None;
    }
    if !case_insensitive {
        return php_source::byte_kernel::find_bytes_from(haystack, needle, start);
    }
    php_source::byte_kernel::find_bytes_ascii_case_insensitive_from(haystack, needle, start)
}

pub(in crate::builtins::modules) fn rfind_bytes(
    haystack: &[u8],
    needle: &[u8],
    start: usize,
    start_is_lower_bound: bool,
    case_insensitive: bool,
) -> Option<usize> {
    if needle.is_empty() {
        return Some(if start_is_lower_bound {
            haystack.len()
        } else {
            start.min(haystack.len())
        });
    }
    if needle.len() > haystack.len() {
        return None;
    }
    let max_start = haystack.len().saturating_sub(needle.len());
    if start_is_lower_bound && start > max_start {
        return None;
    }
    let (lower, upper) = if start_is_lower_bound {
        (start, max_start)
    } else {
        (0, start.min(max_start))
    };
    let end = upper + needle.len();
    let index = if case_insensitive {
        php_source::byte_kernel::rfind_bytes_ascii_case_insensitive_before(haystack, needle, end)
    } else {
        php_source::byte_kernel::rfind_bytes_before(haystack, needle, end)
    }?;
    (index >= lower).then_some(index)
}

pub(in crate::builtins::modules) fn position_offset_error(name: &str) -> BuiltinError {
    argument_value_error(
        name,
        "#3 ($offset)",
        "must be contained in argument #1 ($haystack)",
    )
}

pub(in crate::builtins::modules) fn position_offset_arg(
    name: &str,
    value: &Value,
) -> Result<i64, BuiltinError> {
    if let Value::String(value) = deref_value(value) {
        let classified = classify_php_string(&value);
        return match (classified.kind, classified.value) {
            (NumericStringKind::IntString | NumericStringKind::FloatString, Some(value)) => {
                Ok(value.to_i64())
            }
            _ => Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_TYPE",
                format!("{name}(): Argument #3 ($offset) must be of type int, string given"),
            )),
        };
    }
    if let Value::Float(value) = value {
        let value = value.to_f64();
        if !value.is_finite() || value >= i64::MAX as f64 || value < i64::MIN as f64 {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_TYPE",
                format!("{name}(): Argument #3 ($offset) must be of type int, float given"),
            ));
        }
    }
    int_arg(name, value)
}

pub(in crate::builtins::modules) fn compare_strings(
    name: &str,
    args: &[Value],
    case_insensitive: bool,
    limit: Option<usize>,
) -> BuiltinResult {
    let left = string_arg(name, &args[0])?;
    let right = string_arg(name, &args[1])?;
    let mut left = left.as_bytes().to_vec();
    let mut right = right.as_bytes().to_vec();
    if let Some(limit) = limit {
        left.truncate(limit);
        right.truncate(limit);
    }
    if case_insensitive {
        php_source::byte_kernel::ascii_lowercase_in_place(&mut left);
        php_source::byte_kernel::ascii_lowercase_in_place(&mut right);
    }
    Ok(Value::Int(binary_string_compare(&left, &right)))
}

pub(in crate::builtins::modules) fn binary_string_compare(left: &[u8], right: &[u8]) -> i64 {
    let limit = left.len().min(right.len());
    for index in 0..limit {
        let diff = i64::from(left[index]) - i64::from(right[index]);
        if diff != 0 {
            return diff;
        }
    }
    match left.len().cmp(&right.len()) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

pub(in crate::builtins::modules) fn substr_replace_one(
    name: &str,
    subject: &Value,
    replacement: &PhpString,
    offset: i64,
    length: Option<i64>,
) -> BuiltinResult {
    let subject = string_arg(name, subject)?;
    let start = normalize_offset(subject.len(), offset);
    let replace_len = byte_substring_length(name, subject.len(), start, length)?;
    let end = start + replace_len;
    let mut output = Vec::with_capacity(
        subject
            .len()
            .saturating_sub(replace_len)
            .saturating_add(replacement.len()),
    );
    output.extend_from_slice(&subject.as_bytes()[..start]);
    output.extend_from_slice(replacement.as_bytes());
    output.extend_from_slice(&subject.as_bytes()[end..]);
    Ok(Value::string(output))
}

pub(in crate::builtins::modules) fn substr_replace_indexed_string_arg(
    value: &Value,
    index: usize,
) -> Result<PhpString, BuiltinError> {
    match deref_value(value) {
        // PHP walks the replacement array's values in iteration order, so a gap
        // left by unset() is skipped rather than yielding an empty replacement.
        Value::Array(array) => array.iter().nth(index).map_or_else(
            || Ok(PhpString::from_bytes(Vec::new())),
            |(_, value)| string_arg("substr_replace", value),
        ),
        other => string_arg("substr_replace", &other),
    }
}

pub(in crate::builtins::modules) fn substr_replace_indexed_int_arg(
    value: &Value,
    index: usize,
) -> Result<Option<i64>, BuiltinError> {
    match deref_value(value) {
        Value::Array(array) => array.iter().nth(index).map_or(Ok(None), |(_, value)| {
            int_arg("substr_replace", value).map(Some)
        }),
        other => int_arg("substr_replace", &other).map(Some),
    }
}

pub(in crate::builtins::modules) fn stripslashes_bytes(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'\\'
            && let Some(next) = input.get(index + 1).copied()
        {
            output.push(if next == b'0' { b'\0' } else { next });
            index += 2;
        } else {
            output.push(input[index]);
            index += 1;
        }
    }
    output
}

pub(in crate::builtins::modules) fn stripcslashes_bytes(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] != b'\\' {
            output.push(input[index]);
            index += 1;
            continue;
        }
        index += 1;
        let Some(next) = input.get(index).copied() else {
            output.push(b'\\');
            break;
        };
        match next {
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'v' => output.push(0x0b),
            b'f' => output.push(0x0c),
            b'a' => output.push(0x07),
            b'b' => output.push(0x08),
            b'\\' | b'\'' | b'"' => output.push(next),
            b'x' | b'X' => {
                let (decoded, consumed) = decode_c_hex_escape(&input[index + 1..]);
                if consumed == 0 {
                    output.push(next);
                } else {
                    output.push(decoded);
                    index += consumed;
                }
            }
            b'0'..=b'7' => {
                let (decoded, consumed) = decode_c_octal_escape(&input[index..]);
                output.push(decoded);
                index += consumed.saturating_sub(1);
            }
            byte => output.push(byte),
        }
        index += 1;
    }
    output
}

pub(in crate::builtins::modules) fn allowed_strip_tags_arg(
    value: &Value,
) -> Result<Vec<u8>, BuiltinError> {
    match deref_value(value) {
        Value::Null | Value::Uninitialized => Ok(Vec::new()),
        Value::Array(array) => {
            let mut allowed = Vec::new();
            for (_, value) in array.iter() {
                allowed.push(b'<');
                allowed.extend_from_slice(&strip_tags_allowed_string(value)?);
                allowed.push(b'>');
            }
            Ok(lower_ascii_bytes(&allowed))
        }
        Value::Resource(_) => Err(argument_type_error(
            "strip_tags",
            "#2 ($allowed_tags)",
            "array|string|null",
            value,
        )),
        _ => Ok(lower_ascii_bytes(&strip_tags_allowed_string(value)?)),
    }
}

pub(in crate::builtins::modules) fn strip_tags_allowed_string(
    value: &Value,
) -> Result<Vec<u8>, BuiltinError> {
    match string_arg("strip_tags", value) {
        Ok(value) => Ok(value.into_bytes()),
        Err(error) if matches!(deref_value(value), Value::Object(_)) => {
            let _ = error;
            Ok(Vec::new())
        }
        Err(error) => Err(error),
    }
}

pub(in crate::builtins::modules) fn lower_ascii_bytes(input: &[u8]) -> Vec<u8> {
    php_source::byte_kernel::ascii_lowercase_copy(input)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StripTagsState {
    Output,
    HtmlTag,
    PhpTag,
    Declaration,
    Comment,
}

pub(in crate::builtins::modules) fn strip_tags_bytes(
    input: &[u8],
    allowed: Option<&[u8]>,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0usize;
    let mut state = StripTagsState::Output;
    let mut depth = 0usize;
    let mut bracket_depth = 0isize;
    let mut quote = None::<u8>;
    let mut last_significant = 0u8;
    let mut is_xml = false;
    let mut tag_buffer = Vec::new();

    while index < input.len() {
        let byte = input[index];
        match state {
            StripTagsState::Output => match byte {
                0 => {}
                b'<' => {
                    if quote.is_some() {
                        index += 1;
                        continue;
                    }
                    if input
                        .get(index + 1)
                        .is_some_and(|next| next.is_ascii_whitespace())
                    {
                        output.push(byte);
                    } else {
                        last_significant = b'<';
                        state = StripTagsState::HtmlTag;
                        tag_buffer.clear();
                        if allowed.is_some() {
                            tag_buffer.push(b'<');
                        }
                    }
                }
                b'>' => {
                    if depth > 0 {
                        depth -= 1;
                    } else if quote.is_none() {
                        output.push(byte);
                    }
                }
                _ => output.push(byte),
            },
            StripTagsState::HtmlTag => match byte {
                0 => {}
                b'<' => {
                    if quote.is_some() {
                        index += 1;
                        continue;
                    }
                    if input
                        .get(index + 1)
                        .is_some_and(|next| next.is_ascii_whitespace())
                    {
                        push_strip_tag_byte(&mut tag_buffer, allowed, byte);
                    } else {
                        depth += 1;
                    }
                }
                b'>' => {
                    if depth > 0 {
                        depth -= 1;
                    } else if quote.is_none() {
                        last_significant = b'>';
                        if is_xml && index > 0 && input[index - 1] == b'-' {
                            index += 1;
                            continue;
                        }
                        state = StripTagsState::Output;
                        is_xml = false;
                        push_strip_tag_byte(&mut tag_buffer, allowed, b'>');
                        if let Some(allowed) = allowed
                            && strip_tag_is_allowed(&tag_buffer, allowed)
                        {
                            output.extend_from_slice(&tag_buffer);
                        }
                        tag_buffer.clear();
                    }
                }
                b'"' | b'\'' => {
                    quote = match quote {
                        Some(current) if current == byte => None,
                        None if index > 0 => Some(byte),
                        current => current,
                    };
                    push_strip_tag_byte(&mut tag_buffer, allowed, byte);
                }
                b'!' if index > 0 && input[index - 1] == b'<' => {
                    state = StripTagsState::Declaration;
                    last_significant = byte;
                }
                b'?' if index > 0 && input[index - 1] == b'<' => {
                    bracket_depth = 0;
                    state = StripTagsState::PhpTag;
                }
                _ => push_strip_tag_byte(&mut tag_buffer, allowed, byte),
            },
            StripTagsState::PhpTag => match byte {
                b'(' if !matches!(last_significant, b'"' | b'\'') => {
                    last_significant = b'(';
                    bracket_depth += 1;
                }
                b')' if !matches!(last_significant, b'"' | b'\'') => {
                    last_significant = b')';
                    bracket_depth -= 1;
                }
                b'>' => {
                    if depth > 0 {
                        depth -= 1;
                    } else if quote.is_none()
                        && bracket_depth == 0
                        && last_significant != b'"'
                        && index > 0
                        && input[index - 1] == b'?'
                    {
                        state = StripTagsState::Output;
                        tag_buffer.clear();
                    }
                }
                b'"' | b'\'' if index > 0 && input[index - 1] != b'\\' => {
                    if last_significant == byte {
                        last_significant = 0;
                    } else if last_significant != b'\\' {
                        last_significant = byte;
                    }
                    quote = match quote {
                        Some(current) if current == byte => None,
                        None => Some(byte),
                        current => current,
                    };
                }
                b'l' | b'L'
                    if index >= 4
                        && matches!(input[index - 1], b'm' | b'M')
                        && matches!(input[index - 2], b'x' | b'X')
                        && input[index - 3] == b'?'
                        && input[index - 4] == b'<' =>
                {
                    state = StripTagsState::HtmlTag;
                    is_xml = true;
                }
                _ => {}
            },
            StripTagsState::Declaration => match byte {
                b'>' => {
                    if depth > 0 {
                        depth -= 1;
                    } else if quote.is_none() {
                        state = StripTagsState::Output;
                        tag_buffer.clear();
                    }
                }
                b'"' | b'\'' if index > 0 && input[index - 1] != b'\\' => {
                    quote = match quote {
                        Some(current) if current == byte => None,
                        None => Some(byte),
                        current => current,
                    };
                }
                b'-' if index >= 2 && input[index - 1] == b'-' && input[index - 2] == b'!' => {
                    state = StripTagsState::Comment;
                }
                b'e' | b'E'
                    if index >= 6
                        && matches!(input[index - 1], b'p' | b'P')
                        && matches!(input[index - 2], b'y' | b'Y')
                        && matches!(input[index - 3], b't' | b'T')
                        && matches!(input[index - 4], b'c' | b'C')
                        && matches!(input[index - 5], b'o' | b'O')
                        && matches!(input[index - 6], b'd' | b'D') =>
                {
                    state = StripTagsState::HtmlTag;
                }
                _ => {}
            },
            StripTagsState::Comment => {
                if byte == b'>'
                    && quote.is_none()
                    && index >= 2
                    && input[index - 1] == b'-'
                    && input[index - 2] == b'-'
                {
                    state = StripTagsState::Output;
                    tag_buffer.clear();
                }
            }
        }
        index += 1;
    }
    output
}

pub(in crate::builtins::modules) fn push_strip_tag_byte(
    buffer: &mut Vec<u8>,
    allowed: Option<&[u8]>,
    byte: u8,
) {
    if allowed.is_some() {
        buffer.push(byte);
    }
}

pub(in crate::builtins::modules) fn strip_tag_is_allowed(tag: &[u8], allowed: &[u8]) -> bool {
    let normalized = normalize_strip_tag(tag);
    !normalized.is_empty() && find_bytes_from(allowed, &normalized, 0, false).is_some()
}

pub(in crate::builtins::modules) fn normalize_strip_tag(tag: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(tag.len().min(32));
    let mut state = 0u8;
    let mut index = 0usize;
    while index < tag.len() {
        let byte = tag[index].to_ascii_lowercase();
        match byte {
            b'<' => normalized.push(byte),
            b'>' => break,
            byte if byte.is_ascii_whitespace() => {
                if state == 1 {
                    break;
                }
            }
            b'/' if (index > 0 && tag[index - 1] == b'<') || tag.get(index + 1) == Some(&b'>') => {}
            _ => {
                if state == 0 {
                    state = 1;
                }
                normalized.push(byte);
            }
        }
        index += 1;
    }
    if normalized.is_empty() {
        return normalized;
    }
    normalized.push(b'>');
    normalized
}

pub(in crate::builtins::modules) fn decode_c_hex_escape(input: &[u8]) -> (u8, usize) {
    let mut value = 0u8;
    let mut consumed = 0usize;
    for byte in input.iter().copied().take(2) {
        let Some(nibble) = hex_nibble(byte) else {
            break;
        };
        value = (value << 4) | nibble;
        consumed += 1;
    }
    (value, consumed)
}

pub(in crate::builtins::modules) fn decode_c_octal_escape(input: &[u8]) -> (u8, usize) {
    let mut value = 0u16;
    let mut consumed = 0usize;
    for byte in input.iter().copied().take(3) {
        if !(b'0'..=b'7').contains(&byte) {
            break;
        }
        value = (value << 3) | u16::from(byte - b'0');
        consumed += 1;
    }
    (value as u8, consumed)
}

pub(in crate::builtins::modules) fn natural_compare_builtin(
    name: &str,
    args: &[Value],
    case_insensitive: bool,
) -> BuiltinResult {
    let left = string_arg(name, &args[0])?;
    let right = string_arg(name, &args[1])?;
    Ok(Value::Int(ordering_to_i64(natural_compare_bytes(
        left.as_bytes(),
        right.as_bytes(),
        case_insensitive,
    ))))
}

pub(in crate::builtins::modules) fn ordering_to_i64(ordering: std::cmp::Ordering) -> i64 {
    match ordering {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

pub(in crate::builtins::modules) fn natural_compare_bytes(
    left: &[u8],
    right: &[u8],
    case_insensitive: bool,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let mut left_index = 0usize;
    let mut right_index = 0usize;

    if left.is_empty() || right.is_empty() {
        return left.len().cmp(&right.len());
    }

    while left_index + 1 < left.len()
        && left[left_index] == b'0'
        && left[left_index + 1].is_ascii_digit()
    {
        left_index += 1;
    }
    while right_index + 1 < right.len()
        && right[right_index] == b'0'
        && right[right_index + 1].is_ascii_digit()
    {
        right_index += 1;
    }

    loop {
        while left_index < left.len() && left[left_index].is_ascii_whitespace() {
            left_index += 1;
        }
        while right_index < right.len() && right[right_index].is_ascii_whitespace() {
            right_index += 1;
        }

        match (left_index >= left.len(), right_index >= right.len()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (false, false) => {}
        }

        let left_byte = left[left_index];
        let right_byte = right[right_index];
        if left_byte.is_ascii_digit() && right_byte.is_ascii_digit() {
            let order = if left_byte == b'0' || right_byte == b'0' {
                natural_compare_left(left, &mut left_index, right, &mut right_index)
            } else {
                natural_compare_right(left, &mut left_index, right, &mut right_index)
            };
            if order != Ordering::Equal {
                return order;
            }
            match (left_index >= left.len(), right_index >= right.len()) {
                (true, true) => return Ordering::Equal,
                (true, false) => return Ordering::Less,
                (false, true) => return Ordering::Greater,
                (false, false) => continue,
            }
        }

        let left_cmp = if case_insensitive {
            left_byte.to_ascii_uppercase()
        } else {
            left_byte
        };
        let right_cmp = if case_insensitive {
            right_byte.to_ascii_uppercase()
        } else {
            right_byte
        };
        let order = left_cmp.cmp(&right_cmp);
        if order != Ordering::Equal {
            return order;
        }

        left_index += 1;
        right_index += 1;
        match (left_index >= left.len(), right_index >= right.len()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (false, false) => {}
        }
    }
}

pub(in crate::builtins::modules) fn natural_compare_left(
    left: &[u8],
    left_index: &mut usize,
    right: &[u8],
    right_index: &mut usize,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    loop {
        let left_digit = left
            .get(*left_index)
            .copied()
            .is_some_and(|byte| byte.is_ascii_digit());
        let right_digit = right
            .get(*right_index)
            .copied()
            .is_some_and(|byte| byte.is_ascii_digit());
        match (left_digit, right_digit) {
            (false, false) => return Ordering::Equal,
            (false, true) => return Ordering::Less,
            (true, false) => return Ordering::Greater,
            (true, true) => {
                let order = left[*left_index].cmp(&right[*right_index]);
                if order != Ordering::Equal {
                    return order;
                }
                *left_index += 1;
                *right_index += 1;
            }
        }
    }
}

pub(in crate::builtins::modules) fn natural_compare_right(
    left: &[u8],
    left_index: &mut usize,
    right: &[u8],
    right_index: &mut usize,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let mut bias = Ordering::Equal;
    loop {
        let left_digit = left
            .get(*left_index)
            .copied()
            .is_some_and(|byte| byte.is_ascii_digit());
        let right_digit = right
            .get(*right_index)
            .copied()
            .is_some_and(|byte| byte.is_ascii_digit());
        match (left_digit, right_digit) {
            (false, false) => return bias,
            (false, true) => return Ordering::Less,
            (true, false) => return Ordering::Greater,
            (true, true) => {
                if bias == Ordering::Equal {
                    bias = left[*left_index].cmp(&right[*right_index]);
                }
                *left_index += 1;
                *right_index += 1;
            }
        }
    }
}

pub(in crate::builtins::modules) fn wordwrap_bytes(
    input: &[u8],
    width: usize,
    break_string: &[u8],
    cut: bool,
) -> Vec<u8> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut output = Vec::new();
    for (line_index, line) in input.split(|byte| *byte == b'\n').enumerate() {
        if line_index > 0 {
            output.push(b'\n');
        }
        wordwrap_line(line, width, break_string, cut, &mut output);
    }
    output
}

pub(in crate::builtins::modules) fn wordwrap_zero_width_bytes(
    input: &[u8],
    break_string: &[u8],
) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    for byte in input {
        if byte.is_ascii_whitespace() {
            output.extend_from_slice(break_string);
        } else {
            output.push(*byte);
        }
    }
    output
}

pub(in crate::builtins::modules) fn wordwrap_check_memory_limit(
    context: &mut BuiltinContext<'_>,
    input: &[u8],
    width: usize,
    break_string: &[u8],
    span: &RuntimeSourceSpan,
) -> Result<(), BuiltinError> {
    let Some(limit) = context
        .ini_get("memory_limit")
        .and_then(parse_php_memory_limit_bytes)
    else {
        return Ok(());
    };
    let Some(estimated) = wordwrap_worst_case_output_len(input.len(), width, break_string.len())
    else {
        return wordwrap_memory_limit_error(context, limit, usize::MAX, span);
    };
    if estimated <= limit {
        return Ok(());
    }
    wordwrap_memory_limit_error(context, limit, estimated.saturating_sub(input.len()), span)
}

pub(in crate::builtins::modules) fn wordwrap_worst_case_output_len(
    input_len: usize,
    width: usize,
    break_len: usize,
) -> Option<usize> {
    if input_len == 0 || width == 0 || break_len == 0 {
        return Some(input_len);
    }
    let breaks = input_len.saturating_sub(1) / width;
    input_len.checked_add(breaks.checked_mul(break_len)?)
}

pub(in crate::builtins::modules) fn wordwrap_memory_limit_error(
    context: &mut BuiltinContext<'_>,
    limit: usize,
    allocation: usize,
    span: &RuntimeSourceSpan,
) -> Result<(), BuiltinError> {
    let file = span.file.as_deref().unwrap_or("<unknown>");
    let line = span.start;
    let message = format!(
        "Allowed memory size of {limit} bytes exhausted (tried to allocate {allocation} bytes)"
    );
    context.output().write_test_str(&format!(
        "\nFatal error: {message} in {file} on line {line}\n"
    ));
    Err(BuiltinError::new("E_PHP_RUNTIME_MEMORY_LIMIT", message))
}

pub(in crate::builtins::modules) fn parse_php_memory_limit_bytes(value: &str) -> Option<usize> {
    let value = value.trim();
    if value.is_empty() || value == "-1" {
        return None;
    }
    let (number, multiplier) = match value.as_bytes().last().copied() {
        Some(b'g' | b'G') => (&value[..value.len() - 1], 1024usize * 1024 * 1024),
        Some(b'm' | b'M') => (&value[..value.len() - 1], 1024usize * 1024),
        Some(b'k' | b'K') => (&value[..value.len() - 1], 1024usize),
        _ => (value, 1usize),
    };
    let bytes = number.trim().parse::<usize>().ok()?;
    bytes.checked_mul(multiplier)
}

pub(in crate::builtins::modules) fn wordwrap_negative_cut_bytes(
    input: &[u8],
    break_string: &[u8],
) -> Vec<u8> {
    let mut output = Vec::new();
    for byte in input {
        if *byte == b'\n' {
            output.push(b'\n');
        } else {
            output.extend_from_slice(break_string);
            if !byte.is_ascii_whitespace() {
                output.push(*byte);
            }
        }
    }
    output
}

pub(in crate::builtins::modules) fn wordwrap_line(
    line: &[u8],
    width: usize,
    break_string: &[u8],
    cut: bool,
    output: &mut Vec<u8>,
) {
    let mut start = 0usize;
    while line.len().saturating_sub(start) > width {
        let search_end = start + (width + 1).min(line.len() - start);
        let search = &line[start..search_end];
        if let Some(space) = php_source::byte_kernel::rfind_ascii_whitespace(search) {
            if space > 0 {
                output.extend_from_slice(&line[start..start + space]);
                output.extend_from_slice(break_string);
                start += space + 1;
            } else if cut && !break_string_is_whitespace(break_string) {
                output.extend_from_slice(&line[start..start + width]);
                output.extend_from_slice(break_string);
                start += width;
            } else {
                if !cut {
                    output.push(line[start]);
                }
                start += 1;
            }
        } else if cut {
            output.extend_from_slice(&line[start..start + width]);
            if line[start..start + width].ends_with(break_string) {
                start += width;
            } else if line[start + width..].starts_with(break_string) {
                output.extend_from_slice(break_string);
                start += width + break_string.len();
            } else {
                output.extend_from_slice(break_string);
                start += width;
            }
            if line.get(start).is_some_and(u8::is_ascii_whitespace) {
                start += 1;
            }
        } else if let Some(space) =
            php_source::byte_kernel::find_ascii_whitespace(&line[start + width..])
        {
            output.extend_from_slice(&line[start..start + width + space]);
            output.extend_from_slice(break_string);
            start += width + space + 1;
        } else {
            break;
        }
    }
    output.extend_from_slice(&line[start..]);
}

pub(in crate::builtins::modules) fn break_string_is_whitespace(break_string: &[u8]) -> bool {
    php_source::byte_kernel::all_ascii_whitespace(break_string)
}

pub(in crate::builtins::modules) fn uuencode_bytes(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for chunk in input.chunks(45) {
        output.push(uuencode_sixbit(chunk.len() as u8));
        for triple in chunk.chunks(3) {
            let a = triple.first().copied().unwrap_or(0);
            let b = triple.get(1).copied().unwrap_or(0);
            let c = triple.get(2).copied().unwrap_or(0);
            output.push(uuencode_sixbit(a >> 2));
            output.push(uuencode_sixbit(((a << 4) | (b >> 4)) & 0x3f));
            output.push(uuencode_sixbit(((b << 2) | (c >> 6)) & 0x3f));
            output.push(uuencode_sixbit(c & 0x3f));
        }
        output.push(b'\n');
    }
    output.extend_from_slice(b"`\n");
    output
}

pub(in crate::builtins::modules) fn uuencode_sixbit(value: u8) -> u8 {
    let encoded = (value & 0x3f) + 0x20;
    if encoded == 0x20 { b'`' } else { encoded }
}

pub(in crate::builtins::modules) fn uudecode_bytes(input: &[u8]) -> Option<Vec<u8>> {
    if input.is_empty() {
        return None;
    }
    let mut output = Vec::new();
    for raw_line in input.split(|byte| *byte == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        let length = uudecode_sixbit(*line.first()?) as usize;
        if length == 0 {
            return Some(output);
        }
        let encoded_len = length.div_ceil(3) * 4;
        if line.len().saturating_sub(1) < encoded_len {
            return None;
        }
        let mut decoded = Vec::with_capacity(length);
        for group in line[1..].chunks(4) {
            if group.is_empty() {
                continue;
            }
            let a = uudecode_sixbit(group.first().copied().unwrap_or(b'`'));
            let b = uudecode_sixbit(group.get(1).copied().unwrap_or(b'`'));
            let c = uudecode_sixbit(group.get(2).copied().unwrap_or(b'`'));
            let d = uudecode_sixbit(group.get(3).copied().unwrap_or(b'`'));
            decoded.push((a << 2) | (b >> 4));
            decoded.push((b << 4) | (c >> 2));
            decoded.push((c << 6) | d);
            if decoded.len() >= length {
                break;
            }
        }
        decoded.truncate(length);
        if decoded.len() != length {
            return None;
        }
        output.extend(decoded);
    }
    Some(output)
}

pub(in crate::builtins::modules) fn uudecode_sixbit(value: u8) -> u8 {
    if value == b'`' {
        0
    } else {
        value.wrapping_sub(0x20) & 0x3f
    }
}

pub(in crate::builtins::modules) fn trim_builtin(
    context: &mut BuiltinContext<'_>,
    name: &str,
    args: Vec<Value>,
    left: bool,
    right: bool,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            format!("builtin {name} expects one or two argument(s)"),
        ));
    }
    let string = string_arg(name, &args[0])?;
    let mask = args
        .get(1)
        .map(|value| string_arg(name, value))
        .transpose()?;
    let default_mask = mask.is_none();
    let mask = mask.as_ref().map_or_else(default_trim_mask, |mask| {
        trim_mask_from_charlist(context, name, mask.as_bytes(), span)
    });
    let bytes = string.as_bytes();
    let (default_start, default_end) = if default_mask {
        php_source::byte_kernel::trim_default_bounds(bytes)
    } else {
        (0, bytes.len())
    };
    let start = if left && default_mask {
        default_start
    } else if left {
        bytes
            .iter()
            .position(|byte| !mask[usize::from(*byte)])
            .unwrap_or(bytes.len())
    } else {
        0
    };
    let end = if right && default_mask {
        default_end
    } else if right {
        bytes
            .iter()
            .rposition(|byte| !mask[usize::from(*byte)])
            .map_or(start, |index| index + 1)
    } else {
        bytes.len()
    };
    Ok(Value::string(bytes[start..end].to_vec()))
}

pub(in crate::builtins::modules) fn default_trim_mask() -> [bool; 256] {
    let mut mask = [false; 256];
    for byte in b" \t\n\r\0\x0b" {
        mask[usize::from(*byte)] = true;
    }
    mask
}

pub(in crate::builtins::modules) fn trim_mask_from_charlist(
    context: &mut BuiltinContext<'_>,
    name: &str,
    charlist: &[u8],
    span: RuntimeSourceSpan,
) -> [bool; 256] {
    let mut mask = [false; 256];
    let mut index = 0usize;
    let mut previous_range = false;
    while index < charlist.len() {
        if charlist.get(index..index + 2) == Some(b"..") {
            trim_range_warning(
                context,
                name,
                if index == 0 {
                    "Invalid '..'-range, no character to the left of '..'"
                } else if index + 2 >= charlist.len() {
                    "Invalid '..'-range, no character to the right of '..'"
                } else {
                    "Invalid '..'-range"
                },
                span.clone(),
            );
            index += 2;
            previous_range = false;
            continue;
        }

        let byte = charlist[index];
        if charlist.get(index + 1..index + 3) == Some(b"..") {
            if index + 3 >= charlist.len() {
                trim_range_warning(
                    context,
                    name,
                    "Invalid '..'-range, no character to the right of '..'",
                    span.clone(),
                );
                mask[usize::from(byte)] = true;
                index += 3;
                previous_range = false;
                continue;
            }
            let end = charlist[index + 3];
            if previous_range {
                trim_range_warning(context, name, "Invalid '..'-range", span.clone());
                mask[usize::from(byte)] = true;
                index += 1;
                previous_range = false;
                continue;
            }
            if byte > end {
                trim_range_warning(
                    context,
                    name,
                    "Invalid '..'-range, '..'-range needs to be incrementing",
                    span.clone(),
                );
                mask[usize::from(byte)] = true;
                mask[usize::from(end)] = true;
                index += 4;
                previous_range = false;
                continue;
            }
            for included in byte..=end {
                mask[usize::from(included)] = true;
            }
            index += 4;
            previous_range = true;
        } else {
            mask[usize::from(byte)] = true;
            index += 1;
            previous_range = false;
        }
    }
    mask
}

pub(in crate::builtins::modules) fn trim_range_warning(
    context: &mut BuiltinContext<'_>,
    name: &str,
    message: &str,
    span: RuntimeSourceSpan,
) {
    context.php_warning(
        "E_PHP_RUNTIME_TRIM_CHARLIST_RANGE",
        format!("{name}(): {message}"),
        span,
    );
}

pub(in crate::builtins::modules) fn split_bytes(bytes: &[u8], separator: &[u8]) -> Vec<Vec<u8>> {
    split_bytes_limited(bytes, separator, usize::MAX)
}

pub(in crate::builtins::modules) fn split_bytes_limited(
    bytes: &[u8],
    separator: &[u8],
    limit: usize,
) -> Vec<Vec<u8>> {
    if limit == 0 {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut start = 0;
    while parts.len() + 1 < limit {
        let Some(index) = find_bytes_from(bytes, separator, start, false) else {
            break;
        };
        parts.push(bytes[start..index].to_vec());
        start = index + separator.len();
    }
    parts.push(bytes[start..].to_vec());
    parts
}

pub(in crate::builtins::modules) fn array_key_arg(
    name: &str,
    value: &Value,
) -> Result<ArrayKey, BuiltinError> {
    ArrayKey::from_value(&deref_value(value))
        .ok_or_else(|| type_error(name, "int|string key-compatible value", value))
}

pub(in crate::builtins::modules) fn array_value_arg(
    name: &str,
    value: &Value,
) -> Result<crate::PhpArray, BuiltinError> {
    let Value::Array(array) = deref_value(value) else {
        return Err(type_error(name, "array", value));
    };
    Ok(array)
}

pub(in crate::builtins::modules) fn array_list_arg(
    name: &str,
    values: &[Value],
) -> Result<Vec<crate::PhpArray>, BuiltinError> {
    values
        .iter()
        .map(|value| array_value_arg(name, value))
        .collect()
}

pub(in crate::builtins::modules) fn array_reference_cell(
    name: &str,
    value: &Value,
) -> Result<crate::ReferenceCell, BuiltinError> {
    let Value::Reference(cell) = value else {
        return Err(type_error(name, "array reference", value));
    };
    Ok(cell.clone())
}

pub(in crate::builtins::modules) fn array_from_reference_cell(
    name: &str,
    cell: &crate::ReferenceCell,
) -> Result<crate::PhpArray, BuiltinError> {
    let value = cell.get();
    let Value::Array(array) = value else {
        return Err(type_error(name, "array", &value));
    };
    Ok(array)
}

pub(in crate::builtins::modules) fn array_key_to_value(key: &ArrayKey) -> Value {
    match key {
        ArrayKey::Int(value) => Value::Int(*value),
        ArrayKey::String(value) => Value::String(value.clone()),
    }
}

pub(in crate::builtins::modules) fn random_bounded_usize(
    name: &str,
    upper: usize,
) -> Result<usize, BuiltinError> {
    debug_assert!(upper > 0);
    let range = upper as u128;
    let zone = u128::MAX - (u128::MAX % range);
    loop {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(|error| {
            BuiltinError::new(
                "E_PHP_RUNTIME_RANDOM_FAILURE",
                format!("{name}(): failed to read random bytes: {error}"),
            )
        })?;
        let sample = u128::from_le_bytes(bytes);
        if sample < zone {
            return Ok((sample % range) as usize);
        }
    }
}

pub(in crate::builtins::modules) fn same_filesystem_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub(in crate::builtins::modules) fn array_value_matches(
    name: &str,
    left: &Value,
    right: &Value,
    strict: bool,
) -> Result<bool, BuiltinError> {
    if strict {
        Ok(identical(left, right))
    } else {
        equal(left, right).map_err(|message| conversion_error(name, message))
    }
}

pub(in crate::builtins::modules) fn array_diff_by_value(
    first: &crate::PhpArray,
    others: &[crate::PhpArray],
) -> Result<crate::PhpArray, BuiltinError> {
    let mut output = crate::PhpArray::new();
    for (key, value) in first.iter() {
        let needle = array_compare_value_key("array_diff", value)?;
        if others.iter().all(|other| {
            !other.iter().any(|(_, candidate)| {
                array_compare_value_key("array_diff", candidate)
                    .is_ok_and(|candidate| candidate == needle)
            })
        }) {
            output.insert(key.clone(), value.clone());
        }
    }
    Ok(output)
}

pub(in crate::builtins::modules) fn array_diff_by_key_and_value(
    first: &crate::PhpArray,
    others: &[crate::PhpArray],
) -> Result<crate::PhpArray, BuiltinError> {
    let mut output = crate::PhpArray::new();
    for (key, value) in first.iter() {
        let needle = array_compare_value_key("array_diff_assoc", value)?;
        if others.iter().all(|other| {
            !other.get(&key).is_some_and(|candidate| {
                array_compare_value_key("array_diff_assoc", candidate)
                    .is_ok_and(|candidate| candidate == needle)
            })
        }) {
            output.insert(key.clone(), value.clone());
        }
    }
    Ok(output)
}

pub(in crate::builtins::modules) fn array_intersect_by_value(
    first: &crate::PhpArray,
    others: &[crate::PhpArray],
) -> Result<crate::PhpArray, BuiltinError> {
    let mut output = crate::PhpArray::new();
    for (key, value) in first.iter() {
        let needle = array_compare_value_key("array_intersect", value)?;
        if others.iter().all(|other| {
            other.iter().any(|(_, candidate)| {
                array_compare_value_key("array_intersect", candidate)
                    .is_ok_and(|candidate| candidate == needle)
            })
        }) {
            output.insert(key.clone(), value.clone());
        }
    }
    Ok(output)
}

pub(in crate::builtins::modules) fn array_intersect_by_key_and_value(
    first: &crate::PhpArray,
    others: &[crate::PhpArray],
) -> Result<crate::PhpArray, BuiltinError> {
    let mut output = crate::PhpArray::new();
    for (key, value) in first.iter() {
        let needle = array_compare_value_key("array_intersect_assoc", value)?;
        if others.iter().all(|other| {
            other.get(&key).is_some_and(|candidate| {
                array_compare_value_key("array_intersect_assoc", candidate)
                    .is_ok_and(|candidate| candidate == needle)
            })
        }) {
            output.insert(key.clone(), value.clone());
        }
    }
    Ok(output)
}

pub(in crate::builtins::modules) fn array_compare_value_key(
    name: &str,
    value: &Value,
) -> Result<Vec<u8>, BuiltinError> {
    Ok(to_string(&deref_value(value))
        .map_err(|message| conversion_error(name, message))?
        .as_bytes()
        .to_vec())
}

pub(in crate::builtins::modules) fn array_callback_intersect_empty_shortcut(
    name: &str,
    args: Vec<Value>,
    callback_count: usize,
) -> BuiltinResult {
    if args.len() < callback_count + 2 {
        return Err(arity_error(
            name,
            if callback_count == 1 {
                "at least three argument(s)"
            } else {
                "at least four argument(s)"
            },
        ));
    }
    let first = array_value_arg(name, &args[0])?;
    let array_arg_end = args.len() - callback_count;
    let others = array_list_arg(name, &args[1..array_arg_end])?;
    if first.is_empty() || others.iter().any(crate::PhpArray::is_empty) {
        return Ok(Value::Array(crate::PhpArray::new()));
    }
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_CALLABLE_CONTEXT_REQUIRED",
        format!("{name}() requires VM callable dispatch for non-empty array comparisons"),
    ))
}

#[derive(Clone, Debug)]
pub(in crate::builtins::modules) enum ArrayUniqueKey {
    Regular(Value),
    Numeric(f64),
    String(Vec<u8>),
}

pub(in crate::builtins::modules) fn array_unique_key(
    value: &Value,
    flags: i64,
) -> Result<ArrayUniqueKey, BuiltinError> {
    let normalized_flags = flags & !SORT_FLAG_CASE;
    let case_insensitive = (flags & SORT_FLAG_CASE) != 0;
    match normalized_flags {
        SORT_REGULAR => Ok(ArrayUniqueKey::Regular(deref_value(value))),
        SORT_NUMERIC => {
            let numeric = to_number(&deref_value(value))
                .map_err(|message| conversion_error("array_unique", message))?;
            Ok(ArrayUniqueKey::Numeric(match numeric {
                NumericValue::Int(value) => value as f64,
                NumericValue::Float(value) => value,
            }))
        }
        SORT_STRING | SORT_LOCALE_STRING | SORT_NATURAL => {
            let mut bytes = to_string(&deref_value(value))
                .map_err(|message| conversion_error("array_unique", message))?
                .as_bytes()
                .to_vec();
            if case_insensitive {
                bytes.make_ascii_lowercase();
            }
            Ok(ArrayUniqueKey::String(bytes))
        }
        _ => {
            let mut bytes = to_string(&deref_value(value))
                .map_err(|message| conversion_error("array_unique", message))?
                .as_bytes()
                .to_vec();
            if case_insensitive {
                bytes.make_ascii_lowercase();
            }
            Ok(ArrayUniqueKey::String(bytes))
        }
    }
}

pub(in crate::builtins::modules) fn array_unique_keys_match(
    left: &ArrayUniqueKey,
    right: &ArrayUniqueKey,
) -> bool {
    match (left, right) {
        (ArrayUniqueKey::Regular(left), ArrayUniqueKey::Regular(right)) => {
            equal(left, right).unwrap_or(false)
        }
        (ArrayUniqueKey::Numeric(left), ArrayUniqueKey::Numeric(right)) => left == right,
        (ArrayUniqueKey::String(left), ArrayUniqueKey::String(right)) => left == right,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::builtins::modules) enum RangeStep {
    Int(i64),
    Float(f64),
}

impl RangeStep {
    fn as_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    fn abs_f64(self) -> f64 {
        self.as_f64().abs()
    }

    fn is_integral(self) -> bool {
        match self {
            Self::Int(_) => true,
            Self::Float(value) => value.fract() == 0.0,
        }
    }

    fn abs_i64(self) -> Option<i64> {
        match self {
            Self::Int(value) => value.checked_abs(),
            Self::Float(value) if value.fract() == 0.0 && value.abs() <= i64::MAX as f64 => {
                Some(value.abs() as i64)
            }
            Self::Float(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::builtins::modules) enum RangeNumeric {
    Int(i64),
    Float(f64),
}

impl RangeNumeric {
    fn as_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    const fn is_int(self) -> bool {
        matches!(self, Self::Int(_))
    }
}

pub(in crate::builtins::modules) fn range_step_arg(
    value: &Value,
) -> Result<RangeStep, BuiltinError> {
    match range_numeric_arg("range", "#3 ($step)", value)? {
        RangeNumeric::Int(value) => Ok(RangeStep::Int(value)),
        RangeNumeric::Float(value) => Ok(RangeStep::Float(value)),
    }
}

pub(in crate::builtins::modules) fn range_numeric_arg(
    name: &str,
    argument: &str,
    value: &Value,
) -> Result<RangeNumeric, BuiltinError> {
    let value = deref_value(value);
    let numeric = match &value {
        Value::String(string) => {
            let classified = classify_php_string(string);
            match (classified.kind, classified.value) {
                (
                    NumericStringKind::IntString
                    | NumericStringKind::FloatString
                    | NumericStringKind::LeadingNumeric,
                    Some(NumericStringValue::Int(value)),
                ) => RangeNumeric::Int(value),
                (
                    NumericStringKind::IntString
                    | NumericStringKind::FloatString
                    | NumericStringKind::LeadingNumeric,
                    Some(NumericStringValue::Float(value)),
                ) => RangeNumeric::Float(value),
                _ => RangeNumeric::Int(0),
            }
        }
        _ => match to_number(&value).map_err(|message| conversion_error(name, message))? {
            NumericValue::Int(value) => RangeNumeric::Int(value),
            NumericValue::Float(value) => RangeNumeric::Float(value),
        },
    };
    validate_finite_range_number(argument, numeric)?;
    Ok(numeric)
}

pub(in crate::builtins::modules) fn validate_range_step(
    step: RangeStep,
) -> Result<(), BuiltinError> {
    let value = step.as_f64();
    if value == 0.0 {
        return Err(argument_value_error("range", "#3 ($step)", "cannot be 0"));
    }
    if !value.is_finite() {
        return Err(argument_value_error(
            "range",
            "#3 ($step)",
            &format!(
                "must be a finite number, {} provided",
                php_non_finite_name(value)
            ),
        ));
    }
    Ok(())
}

pub(in crate::builtins::modules) fn validate_finite_range_number(
    argument: &str,
    value: RangeNumeric,
) -> Result<(), BuiltinError> {
    let value = value.as_f64();
    if value.is_finite() {
        return Ok(());
    }
    Err(argument_value_error(
        "range",
        argument,
        &format!(
            "must be a finite number, {} provided",
            php_non_finite_name(value)
        ),
    ))
}

pub(in crate::builtins::modules) fn php_non_finite_name(value: f64) -> &'static str {
    if value.is_nan() { "NAN" } else { "INF" }
}

pub(in crate::builtins::modules) fn range_string_values(
    context: &mut BuiltinContext<'_>,
    start: &Value,
    end: &Value,
    step: RangeStep,
    span: RuntimeSourceSpan,
) -> Result<Option<Vec<Value>>, BuiltinError> {
    let (Value::String(start), Value::String(end)) = (deref_value(start), deref_value(end)) else {
        return Ok(None);
    };
    let start = RangeStringOperand::new("#1 ($start)", &start);
    let end = RangeStringOperand::new("#2 ($end)", &end);
    warn_ignored_range_string_bytes(context, start, span.clone());
    warn_ignored_range_string_bytes(context, end, span.clone());

    if start.full_numeric
        && end.full_numeric
        && (start.value.len() != 1 || end.value.len() != 1 || !step.is_integral())
    {
        return Ok(None);
    }

    if start.character_candidate && end.character_candidate && !step.is_integral() {
        if !start.full_numeric || !end.full_numeric {
            range_warning(
                context,
                "Argument #3 ($step) must be of type int when generating an array of characters, inputs converted to 0",
                span,
            );
        }
        return Ok(None);
    }

    if start.character_candidate && !end.character_candidate {
        warn_range_empty_string(context, end, span.clone());
        range_warning(
            context,
            "Argument #2 ($end) must be a single byte string if argument #1 ($start) is a single byte string, argument #1 ($start) converted to 0",
            span,
        );
        return Ok(None);
    }
    if !start.character_candidate && end.character_candidate {
        warn_range_empty_string(context, start, span.clone());
        range_warning(
            context,
            "Argument #1 ($start) must be a single byte string if argument #2 ($end) is a single byte string, argument #2 ($end) converted to 0",
            span,
        );
        return Ok(None);
    }
    if !start.character_candidate || !end.character_candidate {
        return Ok(None);
    }

    let start = i32::from(start.first_byte.expect("character candidate has a byte"));
    let end = i32::from(end.first_byte.expect("character candidate has a byte"));
    if start < end && step.as_f64() < 0.0 {
        return Err(range_increasing_step_error());
    }
    let Some(step) = step.abs_i64() else {
        return Ok(None);
    };
    let step = i32::try_from(step).map_err(|_| range_step_span_error())?;
    let distance = (start - end).abs();
    if step > distance && distance != 0 {
        return Err(range_step_span_error());
    }
    let count = distance / step.max(1) + 1;
    ensure_range_size(count as usize)?;
    let direction = if start <= end { 1 } else { -1 };
    let mut out = Vec::with_capacity(count as usize);
    let mut current = start;
    loop {
        out.push(Value::string(vec![current as u8]));
        if current == end {
            break;
        }
        let next = current + direction * step;
        if (direction > 0 && next > end) || (direction < 0 && next < end) {
            break;
        }
        current = next;
    }
    Ok(Some(out))
}

#[derive(Clone, Copy)]
struct RangeStringOperand<'a> {
    argument: &'static str,
    value: &'a PhpString,
    first_byte: Option<u8>,
    character_candidate: bool,
    full_numeric: bool,
}

impl<'a> RangeStringOperand<'a> {
    fn new(argument: &'static str, value: &'a PhpString) -> Self {
        let full_numeric = range_string_is_full_numeric(value);
        let first_byte = value.as_bytes().first().copied();
        let character_candidate = first_byte.is_some() && (value.len() == 1 || !full_numeric);
        Self {
            argument,
            value,
            first_byte,
            character_candidate,
            full_numeric,
        }
    }
}

pub(in crate::builtins::modules) fn range_string_is_full_numeric(value: &PhpString) -> bool {
    let classified = classify_php_string(value);
    matches!(
        classified.kind,
        NumericStringKind::IntString | NumericStringKind::FloatString
    )
}

fn warn_ignored_range_string_bytes(
    context: &mut BuiltinContext<'_>,
    operand: RangeStringOperand<'_>,
    span: RuntimeSourceSpan,
) {
    if operand.value.len() <= 1 || operand.full_numeric {
        return;
    }
    range_warning(
        context,
        &format!(
            "Argument {} must be a single byte, subsequent bytes are ignored",
            operand.argument
        ),
        span,
    );
}

fn warn_range_empty_string(
    context: &mut BuiltinContext<'_>,
    operand: RangeStringOperand<'_>,
    span: RuntimeSourceSpan,
) {
    if !operand.value.is_empty() {
        return;
    }
    range_warning(
        context,
        &format!(
            "Argument {} must not be empty, casted to 0",
            operand.argument
        ),
        span,
    );
}

pub(in crate::builtins::modules) fn range_warning(
    context: &mut BuiltinContext<'_>,
    message: &str,
    span: RuntimeSourceSpan,
) {
    context.php_warning(
        "E_PHP_RUNTIME_RANGE_WARNING",
        format!("range(): {message}"),
        span,
    );
}

pub(in crate::builtins::modules) fn range_null_deprecation(
    context: &mut BuiltinContext<'_>,
    value: &Value,
    argument: &str,
    span: RuntimeSourceSpan,
) {
    if !matches!(deref_value(value), Value::Null) {
        return;
    }
    context.php_deprecation(
        "E_PHP_RUNTIME_RANGE_NULL_ARG",
        format!(
            "range(): Passing null to parameter {argument} of type string|int|float is deprecated"
        ),
        span,
    );
}

pub(in crate::builtins::modules) fn warn_range_null_string_boundary(
    context: &mut BuiltinContext<'_>,
    start: &Value,
    end: &Value,
    span: RuntimeSourceSpan,
) {
    match (deref_value(start), deref_value(end)) {
        (Value::Null, Value::String(end)) => {
            let end = RangeStringOperand::new("#2 ($end)", &end);
            if end.character_candidate {
                range_warning(
                    context,
                    "Argument #1 ($start) must be a single byte string if argument #2 ($end) is a single byte string, argument #2 ($end) converted to 0",
                    span,
                );
            }
        }
        (Value::String(start), Value::Null) => {
            let start = RangeStringOperand::new("#1 ($start)", &start);
            if start.character_candidate {
                range_warning(
                    context,
                    "Argument #2 ($end) must be a single byte string if argument #1 ($start) is a single byte string, argument #1 ($start) converted to 0",
                    span,
                );
            }
        }
        _ => {}
    }
}

pub(in crate::builtins::modules) fn range_numeric_values(
    start: RangeNumeric,
    end: RangeNumeric,
    step: RangeStep,
) -> Result<Vec<Value>, BuiltinError> {
    if start.as_f64() < end.as_f64() && step.as_f64() < 0.0 {
        return Err(range_increasing_step_error());
    }
    let distance = (end.as_f64() - start.as_f64()).abs();
    let step_abs = step.abs_f64();
    if distance != 0.0 && step_abs > distance {
        return Err(range_step_span_error());
    }
    let use_int_values = start.is_int() && end.is_int() && step.is_integral();
    if use_int_values {
        let RangeNumeric::Int(start) = start else {
            unreachable!("use_int_values requires integer start")
        };
        let RangeNumeric::Int(end) = end else {
            unreachable!("use_int_values requires integer end")
        };
        let step = step.abs_i64().ok_or_else(range_step_span_error)?;
        let count = range_int_count(start, end, step)?;
        return range_int_values(start, end, step, count);
    }
    let count = range_float_count(start.as_f64(), end.as_f64(), step_abs)?;
    Ok(range_float_values(
        start.as_f64(),
        end.as_f64(),
        step_abs,
        step,
        count,
    ))
}

pub(in crate::builtins::modules) fn range_float_count(
    start: f64,
    end: f64,
    step_abs: f64,
) -> Result<usize, BuiltinError> {
    let distance = (end - start).abs();
    if !distance.is_finite() || !step_abs.is_finite() || step_abs <= 0.0 {
        return Err(value_error(
            "range",
            "The supplied range exceeds the maximum array size",
        ));
    }
    let step_count = distance / step_abs;
    let rounded_step_count = step_count.round();
    let steps = if (step_count - rounded_step_count).abs()
        <= f64::EPSILON * step_count.abs().max(1.0) * 16.0
    {
        rounded_step_count
    } else {
        step_count.floor()
    };
    if !steps.is_finite() {
        return Err(range_float_size_error(start, end, step_abs, f64::INFINITY));
    }
    let count = steps + 1.0;
    if count > RANGE_MAX_ELEMENTS as f64 {
        return Err(range_float_size_error(start, end, step_abs, count));
    }
    Ok(count as usize)
}

pub(in crate::builtins::modules) fn range_int_count(
    start: i64,
    end: i64,
    step: i64,
) -> Result<usize, BuiltinError> {
    if step <= 0 {
        return Err(argument_value_error("range", "#3 ($step)", "cannot be 0"));
    }
    let distance = if start <= end {
        i128::from(end) - i128::from(start)
    } else {
        i128::from(start) - i128::from(end)
    } as u128;
    let count = distance / step as u128 + 1;
    if count > RANGE_MAX_ELEMENTS as u128 {
        return Err(range_int_size_error(start, end, step, count));
    }
    usize::try_from(count).map_err(|_| range_int_size_error(start, end, step, count))
}

pub(in crate::builtins::modules) fn range_int_values(
    start: i64,
    end: i64,
    step: i64,
    count: usize,
) -> Result<Vec<Value>, BuiltinError> {
    if step <= 0 {
        return Err(argument_value_error("range", "#3 ($step)", "cannot be 0"));
    }
    let mut out = Vec::with_capacity(count);
    let direction = if start <= end { 1_i64 } else { -1_i64 };
    let mut current = start;
    loop {
        out.push(Value::Int(current));
        let Some(next) = current.checked_add(direction.saturating_mul(step)) else {
            break;
        };
        if (direction > 0 && next > end) || (direction < 0 && next < end) {
            break;
        }
        current = next;
    }
    Ok(out)
}

pub(in crate::builtins::modules) fn range_float_values(
    start: f64,
    end: f64,
    step: f64,
    original_step: RangeStep,
    count: usize,
) -> Vec<Value> {
    let direction = if start <= end { 1.0 } else { -1.0 };
    if original_step.is_integral() {
        let mut out = Vec::with_capacity(count);
        let mut current = start;
        let delta = direction * step;
        for _ in 0..count {
            out.push(Value::float(current));
            current += delta;
        }
        return out;
    }
    (0..count)
        .map(|index| Value::float(start + direction * step * index as f64))
        .collect()
}

pub(in crate::builtins::modules) fn ensure_range_size(count: usize) -> Result<(), BuiltinError> {
    if count <= RANGE_MAX_ELEMENTS {
        return Ok(());
    }
    Err(value_error(
        "range",
        "The supplied range exceeds the maximum array size",
    ))
}

pub(in crate::builtins::modules) fn range_float_size_error(
    start: f64,
    end: f64,
    step: f64,
    count: f64,
) -> BuiltinError {
    let excess = count - RANGE_MAX_ELEMENTS as f64;
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        format!(
            "The supplied range exceeds the maximum array size by {} elements: start={}, end={}, step={}. Max size: {}",
            range_float_size_component(excess),
            range_float_endpoint_component(start),
            range_float_endpoint_component(end),
            float_to_php_string(step),
            RANGE_MAX_ELEMENTS
        ),
    )
}

pub(in crate::builtins::modules) fn range_int_size_error(
    start: i64,
    end: i64,
    step: i64,
    count: u128,
) -> BuiltinError {
    let excess = count.saturating_sub(RANGE_MAX_ELEMENTS as u128);
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        format!(
            "The supplied range exceeds the maximum array size by {excess} elements: start={start}, end={end}, step={step}. Calculated size: {count}. Maximum size: {RANGE_MAX_ELEMENTS}."
        ),
    )
}

pub(in crate::builtins::modules) fn range_float_size_component(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

pub(in crate::builtins::modules) fn range_float_endpoint_component(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        float_to_php_string(value)
    }
}

pub(in crate::builtins::modules) fn ensure_array_fill_size(
    count: usize,
) -> Result<(), BuiltinError> {
    if count <= RANGE_MAX_ELEMENTS {
        return Ok(());
    }
    Err(value_error(
        "array_fill",
        "The supplied range exceeds the maximum array size",
    ))
}

pub(in crate::builtins::modules) fn range_step_span_error() -> BuiltinError {
    argument_value_error(
        "range",
        "#3 ($step)",
        "must be less than the range spanned by argument #1 ($start) and argument #2 ($end)",
    )
}

pub(in crate::builtins::modules) fn range_increasing_step_error() -> BuiltinError {
    argument_value_error(
        "range",
        "#3 ($step)",
        "must be greater than 0 for increasing ranges",
    )
}

pub(in crate::builtins::modules) fn count_recursive(array: &crate::PhpArray) -> usize {
    let mut count = array.len();
    for (_, value) in array.iter() {
        if let Value::Array(child) = deref_value(value) {
            count += count_recursive(&child);
        }
    }
    count
}

pub(in crate::builtins::modules) fn array_entries(
    array: &crate::PhpArray,
) -> Vec<(ArrayKey, Value)> {
    array
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub(in crate::builtins::modules) fn array_from_entries_preserve(
    entries: Vec<(ArrayKey, Value)>,
) -> crate::PhpArray {
    let mut array = crate::PhpArray::new();
    for (key, value) in entries {
        array.insert(key, value);
    }
    array
}

pub(in crate::builtins::modules) fn array_from_entries_reindex_ints(
    entries: Vec<(ArrayKey, Value)>,
) -> crate::PhpArray {
    let mut array = crate::PhpArray::new();
    for (key, value) in entries {
        match key {
            ArrayKey::Int(_) => {
                array.append(value);
            }
            ArrayKey::String(key) => {
                array.insert(ArrayKey::String(key), value);
            }
        }
    }
    array
}

pub(in crate::builtins::modules) fn array_from_entries_for_slice(
    entries: Vec<(ArrayKey, Value)>,
    preserve_keys: bool,
) -> crate::PhpArray {
    if preserve_keys {
        return array_from_entries_preserve(entries);
    }
    array_from_entries_reindex_ints(entries)
}

pub(in crate::builtins::modules) fn normalize_slice_start(len: usize, offset: i64) -> usize {
    if offset >= 0 {
        (offset as usize).min(len)
    } else {
        len.saturating_sub(offset.unsigned_abs() as usize)
    }
}

/// Shared `array_slice`-family offset/length math: the resolved
/// `start..end` element range (empty when length consumes the range).
pub(crate) fn slice_bounds(len: usize, offset: i64, length: Option<i64>) -> (usize, usize) {
    let start = normalize_slice_start(len, offset);
    let end = match length {
        None => len,
        Some(length) if length >= 0 => start.saturating_add(length as usize).min(len),
        Some(length) => len.saturating_sub(length.unsigned_abs() as usize),
    };
    (start, end.max(start))
}

pub(in crate::builtins::modules) fn slice_entries(
    entries: Vec<(ArrayKey, Value)>,
    offset: i64,
    length: Option<i64>,
) -> Vec<(ArrayKey, Value)> {
    let (start, end) = slice_bounds(entries.len(), offset, length);
    entries[start..end].to_vec()
}

pub(in crate::builtins::modules) fn splice_length(
    total: usize,
    start: usize,
    length: i64,
) -> Result<usize, BuiltinError> {
    Ok(if length >= 0 {
        (length as usize).min(total.saturating_sub(start))
    } else {
        total
            .saturating_sub(start)
            .saturating_sub(length.unsigned_abs() as usize)
    })
}

pub(in crate::builtins::modules) fn splice_replacement_values(
    name: &str,
    value: &Value,
) -> Result<Vec<Value>, BuiltinError> {
    match deref_value(value) {
        Value::Array(array) => Ok(array.iter().map(|(_, value)| value.clone()).collect()),
        value => Ok(vec![string_arg(name, &value).map(Value::String)?]),
    }
}

pub(in crate::builtins::modules) fn merge_recursive_into(
    output: &mut crate::PhpArray,
    input: &crate::PhpArray,
) {
    for (key, value) in input.iter() {
        match key {
            ArrayKey::Int(_) => {
                output.append(value.clone());
            }
            ArrayKey::String(key) => {
                let out_key = ArrayKey::String(key.clone());
                if let Some(existing) = output.get(&out_key).cloned() {
                    let merged = merge_recursive_values(existing, value.clone());
                    output.insert(out_key, merged);
                } else {
                    output.insert(out_key, value.clone());
                }
            }
        }
    }
}

pub(in crate::builtins::modules) fn merge_recursive_values(left: Value, right: Value) -> Value {
    match (deref_value(&left), deref_value(&right)) {
        (Value::Array(mut left), Value::Array(right)) => {
            merge_recursive_into(&mut left, &right);
            Value::Array(left)
        }
        (left, right) => Value::packed_array(vec![left, right]),
    }
}

pub(in crate::builtins::modules) fn replace_recursive_into(
    output: &mut crate::PhpArray,
    input: &crate::PhpArray,
) {
    for (key, value) in input.iter() {
        let replacement = if let Some(existing) = output.get(&key).cloned() {
            replace_recursive_values(existing, value.clone())
        } else {
            value.clone()
        };
        output.insert(key.clone(), replacement);
    }
}

pub(in crate::builtins::modules) fn replace_recursive_values(left: Value, right: Value) -> Value {
    match (deref_value(&left), deref_value(&right)) {
        (Value::Array(mut left), Value::Array(right)) => {
            replace_recursive_into(&mut left, &right);
            Value::Array(left)
        }
        (_, right) => right,
    }
}

pub(in crate::builtins::modules) fn string_list_arg(
    name: &str,
    value: &Value,
) -> Result<Vec<crate::PhpString>, BuiltinError> {
    match deref_value(value) {
        Value::Array(array) => array
            .iter()
            .map(|(_, value)| string_arg(name, value))
            .collect::<Result<Vec<_>, _>>(),
        value => Ok(vec![string_arg(name, &value)?]),
    }
}

pub(in crate::builtins::modules) fn replace_subject(
    subject: &Value,
    search: &[crate::PhpString],
    replace: &[crate::PhpString],
    repeat_single_replacement: bool,
    count: &mut i64,
) -> BuiltinResult {
    match deref_value(subject) {
        Value::Array(array) => Ok(Value::Array(crate::PhpArray::from_packed(
            array
                .iter()
                .map(|(_, value)| {
                    replace_subject(value, search, replace, repeat_single_replacement, count)
                })
                .collect::<Result<Vec<_>, _>>()?,
        ))),
        value => {
            let mut bytes = string_arg("str_replace", &value)?.into_bytes();
            for (index, needle) in search.iter().enumerate() {
                if needle.is_empty() {
                    continue;
                }
                let replacement = if repeat_single_replacement {
                    replace.first()
                } else {
                    replace.get(index)
                }
                .map_or(b"".as_slice(), crate::PhpString::as_bytes);
                bytes = replace_all(&bytes, needle.as_bytes(), replacement, count);
            }
            Ok(Value::string(bytes))
        }
    }
}

pub(in crate::builtins::modules) fn replace_all(
    bytes: &[u8],
    needle: &[u8],
    replacement: &[u8],
    count: &mut i64,
) -> Vec<u8> {
    let mut occurrences = 0_usize;
    let mut start = 0;
    while let Some(index) = find_bytes_from(bytes, needle, start, false) {
        occurrences += 1;
        start = index + needle.len();
    }
    if occurrences == 0 {
        return bytes.to_vec();
    }
    let mut output = Vec::with_capacity(
        bytes.len() - occurrences * needle.len() + occurrences * replacement.len(),
    );
    let mut start = 0;
    while let Some(index) = find_bytes_from(bytes, needle, start, false) {
        output.extend_from_slice(&bytes[start..index]);
        output.extend_from_slice(replacement);
        *count += 1;
        start = index + needle.len();
    }
    output.extend_from_slice(&bytes[start..]);
    output
}

pub(in crate::builtins::modules) fn replace_map(
    bytes: &[u8],
    replacements: &[(Vec<u8>, Vec<u8>)],
) -> Vec<u8> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if let Some((needle, replacement)) = replacements
            .iter()
            .find(|(needle, _)| !needle.is_empty() && bytes[index..].starts_with(needle))
        {
            output.extend_from_slice(replacement);
            index += needle.len();
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    output
}

pub(in crate::builtins::modules) fn change_first_ascii(
    string: crate::PhpString,
    uppercase: bool,
) -> Vec<u8> {
    let mut bytes = string.into_bytes();
    if let Some(first) = bytes.first_mut() {
        *first = if uppercase {
            first.to_ascii_uppercase()
        } else {
            first.to_ascii_lowercase()
        };
    }
    bytes
}

pub(in crate::builtins::modules) fn repeat_pad(pad: &[u8], length: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(length);
    while output.len() < length {
        let remaining = length - output.len();
        output.extend_from_slice(&pad[..pad.len().min(remaining)]);
    }
    output
}

#[derive(Clone, Copy, Debug)]
struct PrintfSpec {
    arg_position: Option<usize>,
    left_align: bool,
    force_sign: bool,
    space_sign: bool,
    zero_pad: bool,
    pad_byte: u8,
    width: Option<usize>,
    precision: Option<usize>,
    specifier: u8,
}

pub(in crate::builtins::modules) fn php_format(
    name: &str,
    format: &[u8],
    args: &[Value],
    context: &mut BuiltinContext<'_>,
    span: RuntimeSourceSpan,
) -> Result<Vec<u8>, BuiltinError> {
    let mut output = Vec::new();
    let mut format_index = 0;
    let mut arg_index = 0;

    while format_index < format.len() {
        if format[format_index] != b'%' {
            output.push(format[format_index]);
            format_index += 1;
            continue;
        }
        format_index += 1;
        if format_index >= format.len() {
            return Err(value_error(name, "incomplete format specifier"));
        }
        if format[format_index] == b'%' {
            output.push(b'%');
            format_index += 1;
            continue;
        }

        let (spec, next_index) = parse_printf_spec(name, format, format_index)?;
        format_index = next_index;
        let value_index = if let Some(position) = spec.arg_position {
            position
        } else {
            let position = arg_index;
            arg_index += 1;
            position
        };
        let Some(value) = args.get(value_index) else {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_PRINTF_ARGUMENTS",
                format!("builtin {name} has too few arguments for format string"),
            ));
        };
        output.extend_from_slice(&format_printf_value(
            name,
            &spec,
            value,
            context,
            span.clone(),
        )?);
    }

    Ok(output)
}

fn parse_printf_spec(
    name: &str,
    format: &[u8],
    mut index: usize,
) -> Result<(PrintfSpec, usize), BuiltinError> {
    let mut spec = PrintfSpec {
        arg_position: None,
        left_align: false,
        force_sign: false,
        space_sign: false,
        zero_pad: false,
        pad_byte: b' ',
        width: None,
        precision: None,
        specifier: 0,
    };

    let positional_start = index;
    while format
        .get(index)
        .copied()
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        index += 1;
    }
    if index > positional_start && format.get(index) == Some(&b'$') {
        let position = parse_ascii_usize(name, &format[positional_start..index], "position")?;
        if !(1..2_147_483_647).contains(&position) {
            return Err(printf_value_error(
                "Argument number specifier must be greater than zero and less than 2147483647",
            ));
        }
        spec.arg_position = Some(position - 1);
        index += 1;
    } else {
        index = positional_start;
    }

    loop {
        match format.get(index).copied() {
            Some(b'-') => spec.left_align = true,
            Some(b'+') => spec.force_sign = true,
            Some(b' ') => spec.space_sign = true,
            Some(b'0') => spec.zero_pad = true,
            Some(b'\'') => {
                index += 1;
                spec.pad_byte = *format
                    .get(index)
                    .ok_or_else(|| value_error(name, "missing custom padding character"))?;
            }
            _ => break,
        }
        index += 1;
    }

    let width_start = index;
    while format
        .get(index)
        .copied()
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        index += 1;
    }
    if index > width_start {
        spec.width = Some(parse_ascii_usize(
            name,
            &format[width_start..index],
            "width",
        )?);
    }

    if format.get(index) == Some(&b'.') {
        index += 1;
        let precision_start = index;
        while format
            .get(index)
            .copied()
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            index += 1;
        }
        spec.precision = Some(if index == precision_start {
            0
        } else {
            parse_ascii_usize(name, &format[precision_start..index], "precision")?
        });
    }

    while matches!(format.get(index), Some(b'h' | b'l' | b'L')) {
        index += 1;
    }

    let Some(specifier) = format.get(index).copied() else {
        return Err(value_error(name, "incomplete format specifier"));
    };
    if !matches!(
        specifier,
        b's' | b'd'
            | b'u'
            | b'f'
            | b'F'
            | b'e'
            | b'E'
            | b'g'
            | b'G'
            | b'x'
            | b'X'
            | b'o'
            | b'b'
            | b'c'
            | b'%'
    ) {
        if specifier == b'$' {
            return Err(printf_value_error(
                "Argument number specifier must be greater than zero and less than 2147483647",
            ));
        }
        return Err(printf_value_error(&format!(
            "Unknown format specifier \"{}\"",
            specifier as char
        )));
    }
    spec.specifier = specifier;
    Ok((spec, index + 1))
}

pub(in crate::builtins::modules) fn printf_value_error(message: &str) -> BuiltinError {
    BuiltinError::new("E_PHP_RUNTIME_BUILTIN_VALUE", message)
}

pub(in crate::builtins::modules) fn parse_ascii_usize(
    name: &str,
    digits: &[u8],
    field: &str,
) -> Result<usize, BuiltinError> {
    std::str::from_utf8(digits)
        .ok()
        .and_then(|text| text.parse::<usize>().ok())
        .ok_or_else(|| value_error(name, &format!("invalid format {field}")))
}

fn format_printf_value(
    name: &str,
    spec: &PrintfSpec,
    value: &Value,
    context: &mut BuiltinContext<'_>,
    span: RuntimeSourceSpan,
) -> Result<Vec<u8>, BuiltinError> {
    // PHP prints non-finite floats as bare `INF`/`-INF`/`NaN` for the float
    // specifiers, ignoring width, zero-fill, precision, and the `+` flag, so
    // bypass the normal formatting and padding path.
    if matches!(spec.specifier, b'f' | b'F' | b'e' | b'E' | b'g' | b'G')
        && let Some(text) = non_finite_float_text(float_arg(name, value)?)
    {
        return Ok(text.as_bytes().to_vec());
    }
    let bytes = match spec.specifier {
        b's' => {
            let mut bytes = string_cast_value(context, value, span)
                .map_err(|message| conversion_error(name, message))?
                .into_bytes();
            if let Some(precision) = spec.precision {
                bytes.truncate(precision);
            }
            bytes
        }
        b'c' => vec![printf_int_arg(name, value, context, span.clone())?.rem_euclid(256) as u8],
        b'd' => format_signed_decimal(
            name,
            spec,
            printf_int_arg(name, value, context, span.clone())?,
        )?
        .into_bytes(),
        b'u' => (printf_int_arg(name, value, context, span.clone())? as u64)
            .to_string()
            .into_bytes(),
        b'x' if spec.precision.is_some() => Vec::new(),
        b'X' if spec.precision.is_some() => Vec::new(),
        b'o' if spec.precision.is_some() => Vec::new(),
        b'b' if spec.precision.is_some() => Vec::new(),
        b'x' => format!(
            "{:x}",
            printf_int_arg(name, value, context, span.clone())? as u64
        )
        .into_bytes(),
        b'X' => format!(
            "{:X}",
            printf_int_arg(name, value, context, span.clone())? as u64
        )
        .into_bytes(),
        b'o' => format!(
            "{:o}",
            printf_int_arg(name, value, context, span.clone())? as u64
        )
        .into_bytes(),
        b'b' => format!("{:b}", printf_int_arg(name, value, context, span)? as u64).into_bytes(),
        b'f' | b'F' => format_float_decimal(name, spec, float_arg(name, value)?)?.into_bytes(),
        b'e' | b'E' => format_float_scientific(name, spec, float_arg(name, value)?)?.into_bytes(),
        b'g' | b'G' => format_float_general(name, spec, float_arg(name, value)?)?.into_bytes(),
        b'%' => b"%".to_vec(),
        _ => unreachable!("parse_printf_spec validates specifier"),
    };
    Ok(apply_printf_padding(spec, bytes))
}

fn format_signed_decimal(
    name: &str,
    spec: &PrintfSpec,
    value: i64,
) -> Result<String, BuiltinError> {
    let negative = value < 0;
    let digits = if negative {
        (-(value as i128)).to_string()
    } else {
        (value as i128).to_string()
    };
    Ok(format_numeric_sign(name, spec, negative, digits))
}

/// PHP renders non-finite floats as bare `INF`, `-INF`, or `NaN`.
pub(in crate::builtins::modules) fn non_finite_float_text(value: f64) -> Option<&'static str> {
    if value.is_finite() {
        None
    } else if value.is_nan() {
        Some("NaN")
    } else if value.is_sign_negative() {
        Some("-INF")
    } else {
        Some("INF")
    }
}

fn format_float_decimal(name: &str, spec: &PrintfSpec, value: f64) -> Result<String, BuiltinError> {
    if let Some(text) = non_finite_float_text(value) {
        return Ok(text.to_string());
    }
    let mut precision = spec.precision.unwrap_or(6);
    let negative = value.is_sign_negative();
    if spec.left_align
        && spec.zero_pad
        && let Some(width) = spec.width
    {
        let sign_len = usize::from(negative || spec.force_sign);
        let integer_digits = format!("{:.0}", value.abs().trunc()).len();
        precision = precision.max(width.saturating_sub(sign_len + integer_digits + 1));
    }
    let digits = format!("{:.precision$}", value.abs());
    Ok(format_numeric_sign(name, spec, negative, digits))
}

fn format_float_scientific(
    name: &str,
    spec: &PrintfSpec,
    value: f64,
) -> Result<String, BuiltinError> {
    if let Some(text) = non_finite_float_text(value) {
        return Ok(text.to_string());
    }
    let precision = spec.precision.unwrap_or(6);
    let negative = value.is_sign_negative();
    let uppercase = spec.specifier == b'E';
    let digits = format_scientific_abs(value.abs(), precision, uppercase, false);
    Ok(format_numeric_sign(name, spec, negative, digits))
}

fn format_float_general(name: &str, spec: &PrintfSpec, value: f64) -> Result<String, BuiltinError> {
    if let Some(text) = non_finite_float_text(value) {
        return Ok(text.to_string());
    }
    let precision = spec.precision.unwrap_or(6).max(1);
    let negative = value.is_sign_negative();
    let abs = value.abs();
    let exponent = if abs == 0.0 {
        0
    } else {
        abs.log10().floor() as i32
    };
    let uppercase = spec.specifier == b'G';
    let digits = if abs != 0.0 && (exponent < -4 || exponent >= precision as i32) {
        format_scientific_abs(abs, precision.saturating_sub(1), uppercase, true)
    } else {
        let decimals = if exponent >= 0 {
            precision.saturating_sub(exponent as usize + 1)
        } else {
            precision + (-exponent as usize) - 1
        };
        trim_float_fraction(format!("{abs:.decimals$}"))
    };
    Ok(format_numeric_sign(name, spec, negative, digits))
}

pub(in crate::builtins::modules) fn format_scientific_abs(
    value: f64,
    precision: usize,
    uppercase: bool,
    trim_fraction: bool,
) -> String {
    let marker = if uppercase { 'E' } else { 'e' };
    let formatted = if uppercase {
        format!("{value:.precision$E}")
    } else {
        format!("{value:.precision$e}")
    };
    let Some((mantissa, exponent)) = formatted.split_once(marker) else {
        return formatted;
    };
    let mut mantissa = if trim_fraction {
        let trimmed = trim_float_fraction(mantissa.to_owned());
        if precision > 0 && !trimmed.contains('.') {
            format!("{trimmed}.0")
        } else {
            trimmed
        }
    } else {
        mantissa.to_owned()
    };
    let exponent_value = exponent.parse::<i32>().unwrap_or(0);
    let exponent_sign = if exponent_value < 0 { '-' } else { '+' };
    let exponent_digits = exponent_value.abs().to_string();
    mantissa.push(marker);
    mantissa.push(exponent_sign);
    mantissa.push_str(&exponent_digits);
    mantissa
}

pub(in crate::builtins::modules) fn trim_float_fraction(mut text: String) -> String {
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

fn format_numeric_sign(_name: &str, spec: &PrintfSpec, negative: bool, digits: String) -> String {
    if negative {
        format!("-{digits}")
    } else if spec.force_sign {
        format!("+{digits}")
    } else {
        digits
    }
}

fn apply_printf_padding(spec: &PrintfSpec, mut bytes: Vec<u8>) -> Vec<u8> {
    let Some(width) = spec.width else {
        return bytes;
    };
    if bytes.len() >= width {
        return bytes;
    }
    let pad_len = width - bytes.len();
    let pad_byte = if spec.zero_pad && !spec.left_align && spec.pad_byte == b' ' {
        b'0'
    } else {
        spec.pad_byte
    };
    let mut output = Vec::with_capacity(width);
    if spec.left_align {
        output.extend_from_slice(&bytes);
        output.extend(std::iter::repeat_n(b' ', pad_len));
    } else if pad_byte == b'0' && matches!(bytes.first(), Some(b'-' | b'+' | b' ')) {
        output.push(bytes[0]);
        output.extend(std::iter::repeat_n(pad_byte, pad_len));
        output.extend_from_slice(&bytes[1..]);
    } else {
        output.extend(std::iter::repeat_n(pad_byte, pad_len));
        output.append(&mut bytes);
    }
    output
}

pub(in crate::builtins::modules) fn deref_value(value: &Value) -> Value {
    match value {
        Value::Reference(cell) => cell.get(),
        value => value.clone(),
    }
}

pub(in crate::builtins::modules) fn php_gettype(value: &Value) -> &'static str {
    match deref_value(value) {
        Value::Null => "NULL",
        Value::Bool(_) => "boolean",
        Value::Int(_) => "integer",
        Value::Float(_) => "double",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => "object",
        Value::Resource(resource) if resource.kind() == ResourceKind::Closed => "resource (closed)",
        Value::Resource(_) => "resource",
        Value::Callable(_) => "object",
        Value::Uninitialized => "NULL",
        Value::Reference(_) => unreachable!("deref_value removes references"),
    }
}

pub(in crate::builtins::modules) fn php_debug_type(value: &Value) -> String {
    match deref_value(value) {
        Value::Null | Value::Uninitialized => "null".to_owned(),
        Value::Bool(_) => "bool".to_owned(),
        Value::Int(_) => "int".to_owned(),
        Value::Float(_) => "float".to_owned(),
        Value::String(_) => "string".to_owned(),
        Value::Array(_) => "array".to_owned(),
        Value::Object(object) => object.display_name(),
        Value::Resource(resource) => format!("resource ({})", resource.resource_type()),
        Value::Fiber(_) => "Fiber".to_owned(),
        Value::Generator(_) => "Generator".to_owned(),
        Value::Callable(_) => "Closure".to_owned(),
        Value::Reference(_) => unreachable!("deref_value removes references"),
    }
}

pub(in crate::builtins::modules) fn runtime_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => "object",
        Value::Resource(_) => "resource",
        Value::Callable(_) => "callable",
        Value::Reference(_) => "reference",
        Value::Uninitialized => "uninitialized",
    }
}

#[cfg(test)]
mod tests {
    use super::super::debug_output::{php_float_debug_string, php_float_export_string};
    use super::{
        BuiltinCompatibility, BuiltinContext, JSON_ERROR_SYNTAX, JSON_PRESERVE_ZERO_FRACTION,
        JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE, PHP_RAND_MAX, RuntimeSourceSpan,
        SORT_FLAG_CASE, SORT_NUMERIC, SORT_REGULAR, SORT_STRING,
    };
    use crate::builtins::context::{
        JSON_BIGINT_AS_STRING, JSON_ERROR_CTRL_CHAR, JSON_ERROR_DEPTH, JSON_ERROR_NONE,
        JSON_ERROR_STATE_MISMATCH, JSON_FORCE_OBJECT, JSON_HEX_AMP, JSON_HEX_APOS, JSON_HEX_QUOT,
        JSON_HEX_TAG, JSON_NUMERIC_CHECK, JSON_OBJECT_AS_ARRAY, JSON_PRETTY_PRINT,
        JSON_THROW_ON_ERROR,
    };
    use crate::{
        ArrayKey, BuiltinRegistry, ClassEntry, ClassFlags, ClosurePayload, FilesystemCapabilities,
        ObjectRef, OutputBuffer, PhpArray, PhpString, ReferenceCell, ResourceTable,
        RuntimeHttpResponseState, StreamFlags, StreamMetadata, StrtokState, Value, datetime,
        normalize_class_name, pcre,
    };
    use std::path::PathBuf;

    fn call(name: &str, args: Vec<Value>, output: &mut OutputBuffer) -> Value {
        let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
        let mut context = BuiltinContext::new(output);
        (entry.function())(&mut context, args, RuntimeSourceSpan::default()).expect("builtin ok")
    }

    fn call_error(name: &str, args: Vec<Value>, output: &mut OutputBuffer) -> String {
        let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
        let mut context = BuiltinContext::new(output);
        (entry.function())(&mut context, args, RuntimeSourceSpan::default())
            .expect_err("builtin should fail")
            .message()
            .to_owned()
    }

    #[test]
    fn variable_type_aliases_and_numeric_strings_match_php() {
        let mut output = OutputBuffer::new();
        assert_eq!(
            call("is_integer", vec![Value::Int(1)], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_long", vec![Value::Int(1)], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_double", vec![Value::float(1.5)], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_numeric", vec![Value::string("  1.5e2 ")], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_numeric", vec![Value::string("1.5x")], &mut output),
            Value::Bool(false)
        );
        assert_eq!(
            call("is_numeric", vec![Value::Bool(true)], &mut output),
            Value::Bool(false)
        );
    }

    #[test]
    fn mail_accepts_common_sender_argument_shapes() {
        let mut output = OutputBuffer::new();
        assert_eq!(
            call(
                "mail",
                vec![
                    Value::string("admin@example.test"),
                    Value::string("Subject"),
                    Value::string("Body"),
                    Value::string("Header: value"),
                    Value::string("-fsender@example.test"),
                ],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "mail",
                vec![
                    Value::string("admin@example.test"),
                    Value::string("Subject"),
                    Value::string("Body"),
                    Value::packed_array(vec![Value::string("Header: value")]),
                ],
                &mut output
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn gethostbyname_returns_original_host_for_overlong_names() {
        let mut output = OutputBuffer::new();
        let hostname = "a".repeat(256);
        assert_eq!(
            call(
                "gethostbyname",
                vec![Value::string(hostname.clone())],
                &mut output
            ),
            Value::string(hostname)
        );
    }

    #[test]
    fn legacy_random_builtins_return_bounded_ints() {
        let mut output = OutputBuffer::new();
        assert_eq!(
            call("getrandmax", vec![], &mut output),
            Value::Int(i64::from(PHP_RAND_MAX))
        );
        assert_eq!(
            call("mt_getrandmax", vec![], &mut output),
            Value::Int(i64::from(PHP_RAND_MAX))
        );
        for name in ["rand", "mt_rand"] {
            for _ in 0..8 {
                let Value::Int(value) = call(name, vec![Value::Int(3), Value::Int(5)], &mut output)
                else {
                    panic!("{name} should return an int");
                };
                assert!(
                    (3..=5).contains(&value),
                    "{name} returned value outside requested range: {value}"
                );
            }
            let Value::Int(value) = call(name, vec![], &mut output) else {
                panic!("{name} without args should return an int");
            };
            assert!((0..=i64::from(PHP_RAND_MAX)).contains(&value));
            assert_eq!(
                call_error(name, vec![Value::Int(2), Value::Int(1)], &mut output),
                format!("builtin {name}: max must be greater than or equal to min")
            );
        }
    }

    #[test]
    fn checkdate_matches_gregorian_bounds() {
        let mut output = OutputBuffer::new();
        assert_eq!(
            call(
                "checkdate",
                vec![Value::Int(2), Value::Int(29), Value::Int(2006)],
                &mut output
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "checkdate",
                vec![Value::Int(2), Value::Int(29), Value::Int(2000)],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "checkdate",
                vec![Value::Int(1), Value::Int(1), Value::Int(32768)],
                &mut output
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn variable_debug_float_helpers_match_php_shapes() {
        assert_eq!(php_float_debug_string(1e-5_f64.into(), -1), "1.0E-5");
        assert_eq!(php_float_debug_string((-1e-5_f64).into(), -1), "-1.0E-5");
        assert_eq!(
            php_float_export_string((-0.1_f64).into(), 17),
            "-0.10000000000000001"
        );
        assert_eq!(
            php_float_export_string(1e-5_f64.into(), 17),
            "1.0000000000000001E-5"
        );
        assert_eq!(php_float_export_string(100000.0_f64.into(), 17), "100000.0");
    }

    #[test]
    fn quotemeta_escapes_regex_metacharacters() {
        let mut output = OutputBuffer::new();
        assert_eq!(
            call("quotemeta", vec![Value::string("1+1=2")], &mut output),
            Value::string("1\\+1=2")
        );
        assert_eq!(
            call(
                "quotemeta",
                vec![Value::string("a.b\\c+d*e?f[g^h]i$j(k)l")],
                &mut output,
            ),
            Value::string("a\\.b\\\\c\\+d\\*e\\?f\\[g\\^h\\]i\\$j\\(k\\)l")
        );
        assert_eq!(
            call("quotemeta", vec![Value::string("")], &mut output),
            Value::string("")
        );
        assert_eq!(
            call("quotemeta", vec![Value::string("no specials")], &mut output),
            Value::string("no specials")
        );
    }

    #[test]
    fn sprintf_renders_non_finite_floats_without_padding() {
        let mut output = OutputBuffer::new();
        assert_eq!(
            call(
                "sprintf",
                vec![
                    Value::string("%f|%e|%g"),
                    Value::float(f64::INFINITY),
                    Value::float(f64::INFINITY),
                    Value::float(f64::INFINITY),
                ],
                &mut output,
            ),
            Value::string("INF|INF|INF")
        );
        assert_eq!(
            call(
                "sprintf",
                vec![Value::string("%.17g"), Value::float(f64::NEG_INFINITY)],
                &mut output,
            ),
            Value::string("-INF")
        );
        assert_eq!(
            call(
                "sprintf",
                vec![Value::string("%f"), Value::float(f64::NAN)],
                &mut output,
            ),
            Value::string("NaN")
        );
        // PHP ignores width, zero-fill, and the `+` flag for non-finite floats.
        assert_eq!(
            call(
                "sprintf",
                vec![
                    Value::string("[%08.2f][%+f]"),
                    Value::float(f64::INFINITY),
                    Value::float(f64::INFINITY),
                ],
                &mut output,
            ),
            Value::string("[INF][INF]")
        );
    }

    fn call_with_fs(
        name: &str,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        cwd: PathBuf,
        filesystem: FilesystemCapabilities,
    ) -> Value {
        let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
        let mut context = BuiltinContext::with_runtime(output, cwd, filesystem, None);
        (entry.function())(&mut context, args, RuntimeSourceSpan::default()).expect("builtin ok")
    }

    fn call_with_fs_resources(
        name: &str,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        cwd: PathBuf,
        filesystem: FilesystemCapabilities,
        resources: &mut ResourceTable,
    ) -> Value {
        let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
        let mut context = BuiltinContext::with_runtime(output, cwd, filesystem, Some(resources));
        (entry.function())(&mut context, args, RuntimeSourceSpan::default()).expect("builtin ok")
    }

    fn call_in_context(context: &mut BuiltinContext<'_>, name: &str, args: Vec<Value>) -> Value {
        let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
        (entry.function())(context, args, RuntimeSourceSpan::default()).expect("builtin ok")
    }

    fn call_with_http_response(
        name: &str,
        args: Vec<Value>,
        response: &mut RuntimeHttpResponseState,
    ) -> Value {
        let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        context.set_http_response_state(response);
        (entry.function())(&mut context, args, RuntimeSourceSpan::default()).expect("builtin ok")
    }

    fn array_strings(value: Value) -> Vec<String> {
        let Value::Array(array) = value else {
            panic!("expected array");
        };
        array
            .iter()
            .map(|(_, value)| match value {
                Value::String(text) => text.to_string_lossy(),
                other => panic!("expected string entry, got {other:?}"),
            })
            .collect()
    }

    fn array_value(entries: &[(&str, Value)]) -> Value {
        let mut array = PhpArray::new();
        for (key, value) in entries {
            array.insert(
                ArrayKey::String(PhpString::from_test_str(key)),
                value.clone(),
            );
        }
        Value::Array(array)
    }

    #[test]
    fn setcookie_emits_encoded_set_cookie_header() {
        let mut response = RuntimeHttpResponseState::default();

        assert_eq!(
            call_with_http_response(
                "setcookie",
                vec![
                    Value::string("login"),
                    Value::string("hello world"),
                    Value::Int(0),
                    Value::string("/"),
                    Value::string("example.test"),
                    Value::Bool(true),
                    Value::Bool(true),
                ],
                &mut response,
            ),
            Value::Bool(true)
        );

        assert_eq!(
            response.headers_list(),
            vec!["Set-Cookie: login=hello%20world; Path=/; Domain=example.test; Secure; HttpOnly"]
        );
    }

    #[test]
    fn setrawcookie_preserves_safe_raw_value() {
        let mut response = RuntimeHttpResponseState::default();

        assert_eq!(
            call_with_http_response(
                "setrawcookie",
                vec![
                    Value::string("raw"),
                    Value::string("a=b"),
                    Value::Int(0),
                    Value::string("/raw"),
                ],
                &mut response,
            ),
            Value::Bool(true)
        );

        assert_eq!(
            response.headers_list(),
            vec!["Set-Cookie: raw=a=b; Path=/raw"]
        );
    }

    #[test]
    fn setcookie_options_array_supports_expires_and_samesite() {
        let mut response = RuntimeHttpResponseState::default();

        assert_eq!(
            call_with_http_response(
                "setcookie",
                vec![
                    Value::string("prefs"),
                    Value::string("dark"),
                    array_value(&[
                        ("expires", Value::Int(1_609_459_200)),
                        ("path", Value::string("/app")),
                        ("secure", Value::Bool(true)),
                        ("httponly", Value::Bool(true)),
                        ("samesite", Value::string("Strict")),
                    ]),
                ],
                &mut response,
            ),
            Value::Bool(true)
        );

        assert_eq!(
            response.headers_list(),
            vec![
                "Set-Cookie: prefs=dark; Expires=Fri, 01 Jan 2021 00:00:00 GMT; Path=/app; Secure; HttpOnly; SameSite=Strict"
            ]
        );
    }

    #[test]
    fn setcookie_rejects_response_splitting_and_invalid_names() {
        let mut response = RuntimeHttpResponseState::default();

        assert_eq!(
            call_with_http_response(
                "setcookie",
                vec![Value::string("bad\r\nname"), Value::string("ok")],
                &mut response,
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_http_response(
                "setcookie",
                vec![Value::string("good"), Value::string("bad\r\nvalue")],
                &mut response,
            ),
            Value::Bool(false)
        );

        assert!(response.headers.is_empty());
    }

    #[test]
    fn http_response_builtins_track_headers_status_and_cookies() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let mut response = RuntimeHttpResponseState::default();
        context.set_http_response_state(&mut response);

        assert_eq!(
            call_in_context(&mut context, "header", vec![Value::string("X-Test: one")]),
            Value::Null
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "header",
                vec![Value::string("X-Test: two"), Value::Bool(false)]
            ),
            Value::Null
        );
        assert_eq!(
            call_in_context(&mut context, "http_response_code", vec![Value::Int(201)]),
            Value::Int(200)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "setcookie",
                vec![
                    Value::string("sid"),
                    Value::string("a b"),
                    Value::Int(1),
                    Value::string("/")
                ],
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "setrawcookie",
                vec![Value::string("raw"), Value::string("a/b")]
            ),
            Value::Bool(true)
        );

        let headers = array_strings(call_in_context(&mut context, "headers_list", Vec::new()));
        assert_eq!(
            headers,
            vec![
                "X-Test: one",
                "X-Test: two",
                "Set-Cookie: sid=a%20b; Expires=Thu, 01 Jan 1970 00:00:01 GMT; Path=/",
                "Set-Cookie: raw=a/b",
            ]
        );
        assert_eq!(
            call_in_context(&mut context, "http_response_code", Vec::new()),
            Value::Int(201)
        );
    }

    #[test]
    fn setcookie_supports_array_options_and_rejects_invalid_names() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let mut response = RuntimeHttpResponseState::default();
        context.set_http_response_state(&mut response);
        let mut options = PhpArray::new();
        options.insert(
            ArrayKey::String(PhpString::from_test_str("path")),
            Value::string("/admin"),
        );
        options.insert(
            ArrayKey::String(PhpString::from_test_str("secure")),
            Value::Bool(true),
        );
        options.insert(
            ArrayKey::String(PhpString::from_test_str("httponly")),
            Value::Bool(true),
        );
        options.insert(
            ArrayKey::String(PhpString::from_test_str("samesite")),
            Value::string("Lax"),
        );

        assert_eq!(
            call_in_context(
                &mut context,
                "setcookie",
                vec![
                    Value::string("prefs"),
                    Value::string("x"),
                    Value::Array(options)
                ],
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "setcookie",
                vec![Value::string("bad name"), Value::string("x")],
            ),
            Value::Bool(false)
        );

        let headers = array_strings(call_in_context(&mut context, "headers_list", Vec::new()));
        assert_eq!(
            headers,
            vec!["Set-Cookie: prefs=x; Path=/admin; Secure; HttpOnly; SameSite=Lax"]
        );
        assert!(output.to_string_lossy().contains("invalid cookie name"));
    }

    #[test]
    fn builtins_registry_is_sorted_and_classified() {
        let registry = BuiltinRegistry::new();
        let names = registry
            .entries()
            .iter()
            .map(|entry| entry.name())
            .collect::<Vec<_>>();
        let mut sorted = names.clone();
        sorted.sort_unstable();

        assert_eq!(names, sorted);
        assert!(registry.contains("print"));
        assert!(registry.contains("strlen"));
        assert!(
            registry
                .entries()
                .iter()
                .all(|entry| entry.compatibility() == BuiltinCompatibility::Php)
        );
    }

    #[test]
    fn tokenizer_builtins_use_lexer_lexer_names_and_lines() {
        let mut output = OutputBuffer::new();
        let tokens = call(
            "token_get_all",
            vec![Value::string("<?php echo $name + 1;")],
            &mut output,
        );
        let Value::Array(tokens) = tokens else {
            panic!("expected token array");
        };
        let first = tokens.get(&ArrayKey::Int(0)).expect("open tag token");
        let Value::Array(first) = first else {
            panic!("expected named token entry");
        };
        let id = first.get(&ArrayKey::Int(0)).expect("token id").clone();
        assert_eq!(
            call("token_name", vec![id], &mut output),
            Value::string("T_OPEN_TAG")
        );
        assert_eq!(first.get(&ArrayKey::Int(1)), Some(&Value::string("<?php ")));
        assert_eq!(first.get(&ArrayKey::Int(2)), Some(&Value::Int(1)));

        let names = tokens
            .iter()
            .filter_map(|(_, value)| match value {
                Value::Array(entry) => entry.get(&ArrayKey::Int(0)).cloned(),
                _ => None,
            })
            .map(|id| call("token_name", vec![id], &mut output))
            .collect::<Vec<_>>();
        assert!(names.contains(&Value::string("T_ECHO")));
        assert!(names.contains(&Value::string("T_VARIABLE")));
        assert!(names.contains(&Value::string("T_LNUMBER")));
        assert!(
            tokens
                .iter()
                .any(|(_, value)| matches!(value, Value::String(text) if text.as_bytes() == b"+"))
        );
    }

    #[test]
    fn tokenizer_builtins_cover_modern_php_85_tokens() {
        let mut output = OutputBuffer::new();
        let tokens = call(
            "token_get_all",
            vec![Value::string(
                "<?php class C { public(set) string $name { get => $this->name; } }",
            )],
            &mut output,
        );
        let Value::Array(tokens) = tokens else {
            panic!("expected token array");
        };
        let names = tokens
            .iter()
            .filter_map(|(_, value)| match value {
                Value::Array(entry) => entry.get(&ArrayKey::Int(0)).cloned(),
                _ => None,
            })
            .map(|id| call("token_name", vec![id], &mut output))
            .collect::<Vec<_>>();
        assert!(names.contains(&Value::string("T_PUBLIC_SET")));
        assert!(names.contains(&Value::string("T_VARIABLE")));
        assert_eq!(
            call("token_name", vec![Value::Int(-1)], &mut output),
            Value::string("UNKNOWN")
        );
    }

    #[test]
    fn builtins_cover_scalar_type_queries_and_print() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call("gettype", vec![Value::Int(7)], &mut output),
            Value::string("integer")
        );
        assert_eq!(
            call("is_int", vec![Value::Int(7)], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_string", vec![Value::string("x")], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_bool", vec![Value::Bool(false)], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_null", vec![Value::Null], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_array", vec![Value::packed_array(vec![])], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_float", vec![Value::float(1.5)], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_scalar", vec![Value::string("x")], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "is_countable",
                vec![Value::packed_array(vec![])],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "is_iterable",
                vec![Value::packed_array(vec![])],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call("print", vec![Value::string("p")], &mut output),
            Value::Int(1)
        );
        assert_eq!(output.to_string_lossy(), "p");
    }

    #[test]
    fn variable_type_builtins_cover_objects_references_and_casts() {
        let mut output = OutputBuffer::new();
        let object = Value::Object(ObjectRef::new_with_display_name(
            &empty_class("DebugBox"),
            "DebugBox",
        ));
        let reference = Value::Reference(ReferenceCell::new(Value::Int(42)));

        assert_eq!(
            call("get_debug_type", vec![object.clone()], &mut output),
            Value::string("DebugBox")
        );
        assert_eq!(
            call("is_object", vec![object], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("gettype", vec![reference.clone()], &mut output),
            Value::string("integer")
        );
        assert_eq!(
            call("is_int", vec![reference], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("boolval", vec![Value::string("0")], &mut output),
            Value::Bool(false)
        );
        assert_eq!(
            call("intval", vec![Value::string("12abc")], &mut output),
            Value::Int(12)
        );
        assert_eq!(
            call(
                "intval",
                vec![Value::string("ff"), Value::Int(16)],
                &mut output
            ),
            Value::Int(255)
        );
        assert_eq!(
            call(
                "intval",
                vec![Value::string("0b1010"), Value::Int(0)],
                &mut output
            ),
            Value::Int(10)
        );
        assert_eq!(
            call(
                "intval",
                vec![Value::string("0b1010"), Value::Int(2)],
                &mut output
            ),
            Value::Int(10)
        );
        assert_eq!(
            call("intval", vec![Value::Int(123), Value::Int(16)], &mut output),
            Value::Int(123)
        );
        assert_eq!(
            call("floatval", vec![Value::string("1.5x")], &mut output),
            Value::float(1.5)
        );
        assert_eq!(
            call("strval", vec![Value::Bool(true)], &mut output),
            Value::string("1")
        );
    }

    #[test]
    fn string_cast_builtins_warn_for_array_to_string() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "strval",
                vec![Value::packed_array(vec![Value::string("x")])],
                &mut output,
            ),
            Value::string("Array")
        );
        assert_eq!(
            call(
                "sprintf",
                vec![
                    Value::string("[%s]"),
                    Value::packed_array(vec![Value::string("x")])
                ],
                &mut output,
            ),
            Value::string("[Array]")
        );

        let warnings = output.to_string_lossy();
        assert_eq!(warnings.matches("Array to string conversion").count(), 2);
    }

    #[test]
    fn trim_builtins_support_php_charlists() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "trim",
                vec![Value::string(b" \t\r\n\0\x0bABC\0\x0b ".to_vec())],
                &mut output,
            ),
            Value::string("ABC")
        );
        assert_eq!(
            call(
                "trim",
                vec![
                    Value::string(b"\n\rExample string\n\r".to_vec()),
                    Value::string(b"\x00..\x1f".to_vec()),
                ],
                &mut output,
            ),
            Value::string("Example string")
        );
        assert_eq!(
            call(
                "trim",
                vec![Value::string("  Hello World\n"), Value::string("..a")],
                &mut output,
            ),
            Value::string("  Hello World\n")
        );
        assert!(
            output
                .to_string_lossy()
                .contains("trim(): Invalid '..'-range, no character to the left of '..'")
        );
    }

    #[test]
    fn wordwrap_handles_php_width_edge_cases() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "wordwrap",
                vec![
                    Value::string("Testing wordrap function"),
                    Value::Int(1),
                    Value::string(" "),
                    Value::Bool(true),
                ],
                &mut output,
            ),
            Value::string("T e s t i n g w o r d r a p f u n c t i o n")
        );
        assert_eq!(
            call(
                "wordwrap",
                vec![
                    Value::string("testing wordwrap function"),
                    Value::Int(0),
                    Value::string("<br />\\n"),
                    Value::Bool(false),
                ],
                &mut output,
            ),
            Value::string("testing<br />\\nwordwrap<br />\\nfunction")
        );
        assert_eq!(
            call_error(
                "wordwrap",
                vec![
                    Value::string("testing"),
                    Value::Int(0),
                    Value::string("<br />\\n"),
                    Value::Bool(true),
                ],
                &mut output,
            ),
            "wordwrap(): Argument #4 ($cut_long_words) cannot be true when argument #2 ($width) is 0"
        );
        assert_eq!(
            call(
                "wordwrap",
                vec![
                    Value::string("123  123ab123"),
                    Value::Int(3),
                    Value::string("ab")
                ],
                &mut output,
            ),
            Value::string("123ab 123ab123")
        );
        assert_eq!(
            call(
                "wordwrap",
                vec![
                    Value::string("123ab123ab123"),
                    Value::Int(3),
                    Value::string("ab"),
                    Value::Bool(true),
                ],
                &mut output,
            ),
            Value::string("123ab123ab123")
        );
        assert_eq!(
            call(
                "wordwrap",
                vec![
                    Value::string("123 1234567890 123"),
                    Value::Int(10),
                    Value::string("|=="),
                    Value::Bool(true),
                ],
                &mut output,
            ),
            Value::string("123|==1234567890|==123")
        );
    }

    #[test]
    fn wordwrap_reports_memory_limit_before_huge_break_allocation() {
        let mut output = OutputBuffer::new();
        let error = call_error(
            "wordwrap",
            vec![
                Value::string(vec![b'x'; 65_534]),
                Value::Int(1),
                Value::string(vec![b'x'; 65_535]),
            ],
            &mut output,
        );

        assert_eq!(
            error,
            "Allowed memory size of 134217728 bytes exhausted (tried to allocate 4294705155 bytes)"
        );
        let output = output.to_string_lossy();
        assert!(output.contains("Fatal error: Allowed memory size of 134217728 bytes exhausted"));
        assert!(output.contains("(tried to allocate 4294705155 bytes)"));
    }

    #[test]
    fn resource_type_builtins_report_open_and_closed_handles() {
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();
        let resource = Value::Resource(resources.register_stream(
            StreamFlags::new(true, true, false),
            StreamMetadata::new("php", "stream", "r+", "php://memory"),
        ));

        assert_eq!(
            call("is_resource", vec![resource.clone()], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call("get_resource_id", vec![resource.clone()], &mut output),
            Value::Int(1)
        );
        assert_eq!(
            call("get_resource_type", vec![resource.clone()], &mut output),
            Value::string("stream")
        );
        assert_eq!(
            call("gettype", vec![resource.clone()], &mut output),
            Value::string("resource")
        );
        assert_eq!(
            call("get_debug_type", vec![resource.clone()], &mut output),
            Value::string("resource (stream)")
        );

        assert!(resources.close(crate::ResourceId::new(1)));
        assert!(!resources.close(crate::ResourceId::new(1)));
        assert_eq!(
            call("get_resource_type", vec![resource.clone()], &mut output),
            Value::string("Unknown")
        );
        assert_eq!(
            call("get_resource_id", vec![Value::Null], &mut output),
            Value::Bool(false)
        );
        assert_eq!(
            call("get_resource_type", vec![Value::Null], &mut output),
            Value::Bool(false)
        );
    }

    #[test]
    fn get_resources_returns_id_keyed_snapshot_and_filters_by_type() {
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();
        let first = resources.register_stream(
            StreamFlags::new(true, false, false),
            StreamMetadata::new("php", "stream", "r", "php://memory"),
        );
        let context_resource = resources.register_stream_context(PhpArray::new());
        let second = resources.register_stream(
            StreamFlags::new(true, true, false),
            StreamMetadata::new("php", "stream", "r+", "php://temp"),
        );
        assert!(resources.close(first.id()));

        assert_resource_array(
            call_with_fs_resources(
                "get_resources",
                vec![],
                &mut output,
                PathBuf::from("/tmp"),
                FilesystemCapabilities::none(),
                &mut resources,
            ),
            &[
                (1, first.clone()),
                (2, context_resource.clone()),
                (3, second.clone()),
            ],
        );
        assert_resource_array(
            call_with_fs_resources(
                "get_resources",
                vec![Value::string("Unknown")],
                &mut output,
                PathBuf::from("/tmp"),
                FilesystemCapabilities::none(),
                &mut resources,
            ),
            &[(1, first.clone())],
        );
        assert_resource_array(
            call_with_fs_resources(
                "get_resources",
                vec![Value::string("stream")],
                &mut output,
                PathBuf::from("/tmp"),
                FilesystemCapabilities::none(),
                &mut resources,
            ),
            &[(3, second.clone())],
        );
        assert_resource_array(
            call_with_fs_resources(
                "get_resources",
                vec![Value::string("stream-context")],
                &mut output,
                PathBuf::from("/tmp"),
                FilesystemCapabilities::none(),
                &mut resources,
            ),
            &[(2, context_resource.clone())],
        );

        let entry = BuiltinRegistry::new()
            .get("get_resources")
            .expect("builtin exists");
        let mut context = BuiltinContext::with_runtime(
            &mut output,
            PathBuf::from("/tmp"),
            FilesystemCapabilities::none(),
            Some(&mut resources),
        );
        let error = (entry.function())(
            &mut context,
            vec![Value::string("not-a-type")],
            RuntimeSourceSpan::default(),
        )
        .expect_err("invalid resource type should fail");
        assert_eq!(
            error.message(),
            "get_resources(): Argument #1 ($type) must be a valid resource type"
        );
    }

    fn assert_resource_array(value: Value, expected: &[(i64, crate::ResourceRef)]) {
        let Value::Array(array) = value else {
            panic!("expected array");
        };
        let actual = array.iter().collect::<Vec<_>>();
        assert_eq!(actual.len(), expected.len());
        for ((key, value), (expected_key, expected_resource)) in actual.iter().zip(expected) {
            assert_eq!(*key, ArrayKey::Int(*expected_key));
            assert_eq!(**value, Value::Resource(expected_resource.clone()));
        }
    }

    #[test]
    fn path_helpers_cover_basename_dirname_and_pathinfo() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "basename",
                vec![Value::string("/tmp/example.php"), Value::string(".php")],
                &mut output
            ),
            Value::string("example")
        );
        assert_eq!(
            call(
                "basename",
                vec![Value::string("example.php"), Value::string("example.php")],
                &mut output
            ),
            Value::string("example.php")
        );
        assert_eq!(
            call("dirname", vec![Value::string("/tmp/a/b.php")], &mut output),
            Value::string("/tmp/a")
        );
        assert_eq!(
            call("dirname", vec![Value::string("")], &mut output),
            Value::string("")
        );
        let Value::Array(info) = call("pathinfo", vec![Value::string("/tmp/a/b.php")], &mut output)
        else {
            panic!("pathinfo should return array");
        };
        assert_eq!(
            info.get(&ArrayKey::String(PhpString::from_test_str("dirname"))),
            Some(&Value::string("/tmp/a"))
        );
        assert_eq!(
            info.get(&ArrayKey::String(PhpString::from_test_str("basename"))),
            Some(&Value::string("b.php"))
        );
        assert_eq!(
            info.get(&ArrayKey::String(PhpString::from_test_str("extension"))),
            Some(&Value::string("php"))
        );
        assert_eq!(
            info.get(&ArrayKey::String(PhpString::from_test_str("filename"))),
            Some(&Value::string("b"))
        );
        let Value::Array(empty_info) = call("pathinfo", vec![Value::string("")], &mut output)
        else {
            panic!("pathinfo should return array");
        };
        assert_eq!(
            empty_info.get(&ArrayKey::String(PhpString::from_test_str("dirname"))),
            None
        );
        assert_eq!(
            empty_info.get(&ArrayKey::String(PhpString::from_test_str("basename"))),
            Some(&Value::string(""))
        );
        assert_eq!(
            empty_info.get(&ArrayKey::String(PhpString::from_test_str("filename"))),
            Some(&Value::string(""))
        );

        let Value::Array(dot_info) = call("pathinfo", vec![Value::string(".")], &mut output) else {
            panic!("pathinfo should return array");
        };
        assert_eq!(
            dot_info.get(&ArrayKey::String(PhpString::from_test_str("extension"))),
            Some(&Value::string(""))
        );
        assert_eq!(
            dot_info.get(&ArrayKey::String(PhpString::from_test_str("filename"))),
            Some(&Value::string(""))
        );

        let Value::Array(dotfile_info) =
            call("pathinfo", vec![Value::string(".cvsignore")], &mut output)
        else {
            panic!("pathinfo should return array");
        };
        assert_eq!(
            dotfile_info.get(&ArrayKey::String(PhpString::from_test_str("extension"))),
            Some(&Value::string("cvsignore"))
        );
        assert_eq!(
            dotfile_info.get(&ArrayKey::String(PhpString::from_test_str("filename"))),
            Some(&Value::string(""))
        );

        assert_eq!(
            call(
                "pathinfo",
                vec![
                    Value::string("/usr/include/arpa/inet.h"),
                    Value::Int(1 | 4 | 8),
                ],
                &mut output
            ),
            Value::string("/usr/include/arpa")
        );
        assert_eq!(
            call(
                "pathinfo",
                vec![
                    Value::string("/usr/include/arpa/inet.h"),
                    Value::Int(2 | 4 | 8),
                ],
                &mut output
            ),
            Value::string("inet.h")
        );
    }

    #[test]
    fn stat_builtins_are_restricted_to_allowed_roots() {
        let root = std::env::temp_dir().join(format!("phrust-stdlib-stat-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temp root");
        let file = root.join("fixture.txt");
        std::fs::write(&file, b"fixture").expect("write fixture");
        let mut output = OutputBuffer::new();

        assert_eq!(
            call_with_fs(
                "file_exists",
                vec![Value::string(file.to_string_lossy().as_bytes().to_vec())],
                &mut output,
                PathBuf::from("."),
                FilesystemCapabilities::none()
            ),
            Value::Bool(false)
        );

        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        assert_eq!(
            call_with_fs(
                "file_exists",
                vec![Value::string("fixture.txt")],
                &mut output,
                root.clone(),
                capabilities.clone()
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "is_file",
                vec![Value::string("fixture.txt")],
                &mut output,
                root.clone(),
                capabilities.clone()
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "is_dir",
                vec![Value::string(".")],
                &mut output,
                root.clone(),
                capabilities.clone()
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "filesize",
                vec![Value::string("fixture.txt")],
                &mut output,
                root.clone(),
                capabilities.clone()
            ),
            Value::Int(7)
        );
        assert_eq!(
            call_with_fs(
                "filetype",
                vec![Value::string("fixture.txt")],
                &mut output,
                root.clone(),
                capabilities.clone()
            ),
            Value::string("file")
        );
        assert!(matches!(
            call_with_fs(
                "stat",
                vec![Value::string("fixture.txt")],
                &mut output,
                root.clone(),
                capabilities.clone()
            ),
            Value::Array(_)
        ));
        assert!(matches!(
            call_with_fs(
                "realpath",
                vec![Value::string("fixture.txt")],
                &mut output,
                root.clone(),
                capabilities
            ),
            Value::String(_)
        ));
        assert_eq!(call("clearstatcache", Vec::new(), &mut output), Value::Null);

        let _ = std::fs::remove_file(file);
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn file_get_contents_reads_php_input_from_request_context() {
        let root = std::env::temp_dir().join(format!("phrust-stdlib-input-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();
        let mut context = BuiltinContext::with_runtime(
            &mut output,
            root.clone(),
            capabilities,
            Some(&mut resources),
        );
        context.set_php_input(b"name=phrust".to_vec());

        assert_eq!(
            call_in_context(
                &mut context,
                "file_get_contents",
                vec![Value::string("php://input")]
            ),
            Value::string("name=phrust")
        );

        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn file_get_contents_accepts_offset_and_length_arguments() {
        let root = std::env::temp_dir().join(format!(
            "phrust-stdlib-file-get-contents-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        std::fs::write(root.join("fixture.txt"), b"abcdef").expect("write fixture");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();

        assert_eq!(
            call_with_fs(
                "file_get_contents",
                vec![
                    Value::string("fixture.txt"),
                    Value::Bool(false),
                    Value::Null,
                    Value::Int(0),
                    Value::Int(3),
                ],
                &mut output,
                root.clone(),
                capabilities.clone()
            ),
            Value::string("abc")
        );
        assert_eq!(
            call_with_fs(
                "file_get_contents",
                vec![
                    Value::string("fixture.txt"),
                    Value::Bool(false),
                    Value::Null,
                    Value::Int(1),
                    Value::Int(3),
                ],
                &mut output,
                root.clone(),
                capabilities
            ),
            Value::string("bcd")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_handle_builtins_cover_read_write_seek_and_modes() {
        let root =
            std::env::temp_dir().join(format!("phrust-stdlib-fileio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();

        let handle = call_with_fs_resources(
            "fopen",
            vec![Value::string("data.txt"), Value::string("w+")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        );
        assert!(matches!(handle, Value::Resource(_)));
        assert_eq!(
            call_with_fs_resources(
                "fwrite",
                vec![handle.clone(), Value::string("alpha\nbeta")],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Int(10)
        );
        assert_eq!(
            call_with_fs_resources(
                "rewind",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs_resources(
                "fgets",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string("alpha\n")
        );
        assert_eq!(
            call_with_fs_resources(
                "fgetc",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string("b")
        );
        assert_eq!(
            call_with_fs_resources(
                "fseek",
                vec![handle.clone(), Value::Int(0)],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Int(0)
        );
        assert_eq!(
            call_with_fs_resources(
                "fread",
                vec![handle.clone(), Value::Int(5)],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string("alpha")
        );
        assert_eq!(
            call_with_fs_resources(
                "ftell",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Int(5)
        );
        assert_eq!(
            call_with_fs_resources(
                "fread",
                vec![handle.clone(), Value::Int(99)],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string("\nbeta")
        );
        assert_eq!(
            call_with_fs_resources(
                "feof",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs_resources(
                "fflush",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs_resources(
                "fclose",
                vec![handle],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );

        let readable = call_with_fs_resources(
            "fopen",
            vec![Value::string("data.txt"), Value::string("r")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        );
        assert!(matches!(readable, Value::Resource(_)));
        assert_eq!(
            call_with_fs_resources(
                "fclose",
                vec![readable],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );

        assert_eq!(
            call_with_fs(
                "file_put_contents",
                vec![Value::string("append.txt"), Value::string("one")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Int(3)
        );
        let append = call_with_fs_resources(
            "fopen",
            vec![Value::string("append.txt"), Value::string("a+")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        );
        assert_eq!(
            call_with_fs_resources(
                "fwrite",
                vec![append.clone(), Value::string("two")],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Int(3)
        );
        assert_eq!(
            call_with_fs_resources(
                "fclose",
                vec![append],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "file_get_contents",
                vec![Value::string("append.txt")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::string("onetwo")
        );
        assert_eq!(
            call_with_fs(
                "file_put_contents",
                vec![
                    Value::string("append.txt"),
                    Value::string("three"),
                    Value::Int(10)
                ],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Int(5)
        );
        assert_eq!(
            call_with_fs(
                "file_get_contents",
                vec![Value::string("append.txt")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::string("onetwothree")
        );

        assert_eq!(
            call_with_fs_resources(
                "fopen",
                vec![Value::string("append.txt"), Value::string("x")],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(false)
        );
        let exclusive = call_with_fs_resources(
            "fopen",
            vec![Value::string("exclusive.txt"), Value::string("x")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        );
        assert!(matches!(exclusive, Value::Resource(_)));
        assert_eq!(
            call_with_fs_resources(
                "fclose",
                vec![exclusive],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );

        assert_eq!(
            call_with_fs(
                "file_put_contents",
                vec![Value::string("create.txt"), Value::string("keep")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Int(4)
        );
        let create = call_with_fs_resources(
            "fopen",
            vec![Value::string("create.txt"), Value::string("c+")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        );
        assert!(matches!(create, Value::Resource(_)));
        assert_eq!(
            call_with_fs_resources(
                "fclose",
                vec![create],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "file_get_contents",
                vec![Value::string("create.txt")],
                &mut output,
                root.clone(),
                capabilities,
            ),
            Value::string("keep")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_operations_are_root_constrained_and_return_false() {
        let root =
            std::env::temp_dir().join(format!("phrust-stdlib-fileops-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();

        assert_eq!(
            call_with_fs(
                "file_get_contents",
                vec![Value::string(
                    root.join("outside.txt")
                        .to_string_lossy()
                        .as_bytes()
                        .to_vec()
                )],
                &mut output,
                PathBuf::from("."),
                FilesystemCapabilities::none(),
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_fs_resources(
                "fopen",
                vec![Value::string("../escape.txt"), Value::string("w")],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(false)
        );

        assert_eq!(
            call_with_fs(
                "file_put_contents",
                vec![Value::string("fixture.txt"), Value::string("hello")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Int(5)
        );
        assert_eq!(
            call_with_fs(
                "file_get_contents",
                vec![Value::string("fixture.txt")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::string("hello")
        );

        let mut read_output = OutputBuffer::new();
        assert_eq!(
            call_with_fs(
                "readfile",
                vec![Value::string("fixture.txt")],
                &mut read_output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Int(5)
        );
        assert_eq!(read_output.to_string_lossy(), "hello");

        assert_eq!(
            call_with_fs(
                "copy",
                vec![Value::string("fixture.txt"), Value::string("fixture.txt")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_fs(
                "copy",
                vec![
                    Value::string(
                        root.join("fixture.txt")
                            .to_string_lossy()
                            .as_bytes()
                            .to_vec()
                    ),
                    Value::string("fixture.txt")
                ],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_fs(
                "copy",
                vec![Value::string("fixture.txt"), Value::string("copy.txt")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "rename",
                vec![Value::string("copy.txt"), Value::string("renamed.txt")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "touch",
                vec![Value::string("touched.txt")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "mkdir",
                vec![Value::string("nested")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "rmdir",
                vec![Value::string("nested")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs(
                "unlink",
                vec![Value::string("renamed.txt")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            ),
            Value::Bool(true)
        );

        let temp_path = call_with_fs(
            "tempnam",
            vec![Value::string("."), Value::string("pre")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        );
        assert!(matches!(temp_path, Value::String(_)));
        let tmp_handle = call_with_fs_resources(
            "tmpfile",
            Vec::new(),
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        );
        assert!(matches!(tmp_handle, Value::Resource(_)));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn directory_handles_read_rewind_and_close_with_sorted_entries() {
        let root = std::env::temp_dir().join(format!("phrust-stdlib-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        std::fs::write(root.join("b.log"), b"b").expect("write fixture");
        std::fs::write(root.join("a.txt"), b"a").expect("write fixture");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();

        let handle = call_with_fs_resources(
            "opendir",
            vec![Value::string(".")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        );
        assert!(matches!(handle, Value::Resource(_)));
        assert_eq!(
            call_with_fs_resources(
                "readdir",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string(".")
        );
        assert_eq!(
            call_with_fs_resources(
                "readdir",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string("..")
        );
        assert_eq!(
            call_with_fs_resources(
                "readdir",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string("a.txt")
        );
        assert_eq!(
            call_with_fs_resources(
                "rewinddir",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs_resources(
                "readdir",
                vec![handle.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string(".")
        );
        assert_eq!(
            call_with_fs_resources(
                "closedir",
                vec![handle],
                &mut output,
                root.clone(),
                capabilities,
                &mut resources,
            ),
            Value::Null
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn scandir_glob_and_directory_capabilities_are_normalized() {
        let root = std::env::temp_dir().join(format!("phrust-stdlib-glob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("nested")).expect("create temp root");
        std::fs::write(root.join("b.log"), b"b").expect("write fixture");
        std::fs::write(root.join("a.txt"), b"a").expect("write fixture");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();

        assert_eq!(
            call_with_fs_resources(
                "opendir",
                vec![Value::string(root.to_string_lossy().as_bytes().to_vec())],
                &mut output,
                PathBuf::from("."),
                FilesystemCapabilities::none(),
                &mut resources,
            ),
            Value::Bool(false)
        );
        assert_eq!(
            array_strings(call_with_fs(
                "scandir",
                vec![Value::string(".")],
                &mut output,
                root.clone(),
                capabilities.clone(),
            )),
            vec![".", "..", "a.txt", "b.log", "nested"]
        );
        assert_eq!(
            array_strings(call_with_fs(
                "scandir",
                vec![Value::string("."), Value::Int(1)],
                &mut output,
                root.clone(),
                capabilities.clone(),
            )),
            vec!["nested", "b.log", "a.txt", "..", "."]
        );
        let globbed = array_strings(call_with_fs(
            "glob",
            vec![Value::string("*.txt")],
            &mut output,
            root.clone(),
            capabilities,
        ));
        assert_eq!(globbed.len(), 1);
        assert!(globbed[0].ends_with("a.txt"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn getcwd_and_chdir_are_request_local_to_builtin_context() {
        let root = std::env::temp_dir().join(format!("phrust-stdlib-cwd-{}", std::process::id()));
        let nested = root.join("nested");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&nested).expect("create temp root");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut context =
            BuiltinContext::with_runtime(&mut output, root.clone(), capabilities, None);

        assert_eq!(
            call_in_context(&mut context, "getcwd", Vec::new()),
            Value::string(root.to_string_lossy().as_bytes().to_vec())
        );
        assert_eq!(
            call_in_context(&mut context, "chdir", vec![Value::string("nested")]),
            Value::Bool(true)
        );
        assert_eq!(
            call_in_context(&mut context, "getcwd", Vec::new()),
            Value::string(nested.to_string_lossy().as_bytes().to_vec())
        );
        assert_eq!(
            call_in_context(&mut context, "chdir", vec![Value::string("../..")]),
            Value::Bool(false)
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stream_metadata_contents_copy_and_local_checks_are_capability_aware() {
        let root =
            std::env::temp_dir().join(format!("phrust-stdlib-streams-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();

        assert_eq!(
            array_strings(call("stream_get_wrappers", Vec::new(), &mut output)),
            vec!["file".to_string(), "php".to_string()]
        );

        let source = call_with_fs_resources(
            "fopen",
            vec![Value::string("php://memory"), Value::string("w+")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        );
        let destination = call_with_fs_resources(
            "fopen",
            vec![Value::string("php://memory"), Value::string("w+")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        );
        assert_eq!(
            call_with_fs_resources(
                "fwrite",
                vec![source.clone(), Value::string("abcdef")],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Int(6)
        );
        assert_eq!(
            call_with_fs_resources(
                "stream_get_contents",
                vec![source.clone(), Value::Int(3), Value::Int(2)],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string("cde")
        );
        assert_eq!(
            call_with_fs_resources(
                "stream_copy_to_stream",
                vec![
                    source.clone(),
                    destination.clone(),
                    Value::Int(4),
                    Value::Int(0)
                ],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Int(4)
        );
        assert_eq!(
            call_with_fs_resources(
                "rewind",
                vec![destination.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs_resources(
                "stream_get_contents",
                vec![destination.clone()],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::string("abcd")
        );

        let Value::Array(metadata) = call_with_fs_resources(
            "stream_get_meta_data",
            vec![source.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ) else {
            panic!("expected metadata array");
        };
        assert_eq!(
            metadata.get(&ArrayKey::String(PhpString::from_test_str("wrapper_type"))),
            Some(&Value::string("PHP"))
        );
        assert_eq!(
            metadata.get(&ArrayKey::String(PhpString::from_test_str("stream_type"))),
            Some(&Value::string("MEMORY"))
        );

        assert_eq!(
            call_with_fs_resources(
                "stream_is_local",
                vec![Value::string("https://example.test/file.txt")],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_fs_resources(
                "stream_is_local",
                vec![Value::string("php://memory")],
                &mut output,
                root.clone(),
                capabilities.clone(),
                &mut resources,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_fs_resources(
                "stream_isatty",
                vec![source],
                &mut output,
                root.clone(),
                capabilities,
                &mut resources,
            ),
            Value::Bool(false)
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stream_context_options_and_include_path_resolution_are_preserved() {
        let root = std::env::temp_dir().join(format!(
            "phrust-stdlib-stream-context-{}",
            std::process::id()
        ));
        let lib = root.join("lib");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&lib).expect("create include dir");
        std::fs::write(lib.join("Foo.php"), b"<?php").expect("write include fixture");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();
        let mut context = BuiltinContext::with_runtime(
            &mut output,
            root.clone(),
            capabilities.clone(),
            Some(&mut resources),
        );
        context.set_include_path(vec![PathBuf::from("lib")]);

        let stream_context = call_in_context(&mut context, "stream_context_create", Vec::new());
        assert!(matches!(stream_context, Value::Resource(_)));
        assert_eq!(
            call_in_context(
                &mut context,
                "stream_context_set_option",
                vec![
                    stream_context.clone(),
                    Value::string("http"),
                    Value::string("timeout"),
                    Value::Int(5),
                ],
            ),
            Value::Bool(true)
        );
        let Value::Array(options) = call_in_context(
            &mut context,
            "stream_context_get_options",
            vec![stream_context.clone()],
        ) else {
            panic!("expected context options");
        };
        let Some(Value::Array(http_options)) =
            options.get(&ArrayKey::String(PhpString::from_test_str("http")))
        else {
            panic!("expected http options");
        };
        assert_eq!(
            http_options.get(&ArrayKey::String(PhpString::from_test_str("timeout"))),
            Some(&Value::Int(5))
        );

        let resolved = call_in_context(
            &mut context,
            "stream_resolve_include_path",
            vec![Value::string("Foo.php")],
        );
        let Value::String(path) = resolved else {
            panic!("expected resolved include path");
        };
        assert!(path.to_string_lossy().ends_with("lib/Foo.php"));
        assert_eq!(
            call_in_context(
                &mut context,
                "stream_resolve_include_path",
                vec![Value::string("../escape.php")],
            ),
            Value::Bool(false)
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn preg_match_and_match_all_capture_offsets_and_modifiers() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let matches = ReferenceCell::new(Value::Null);
        assert_eq!(
            call_in_context(
                &mut context,
                "preg_match",
                vec![
                    Value::string(r#"/([a-z]+)-(\d+)/i"#),
                    Value::string("ABC-123"),
                    Value::Reference(matches.clone()),
                    Value::Int(pcre::PREG_OFFSET_CAPTURE),
                ],
            ),
            Value::Int(1)
        );
        let Value::Array(captures) = matches.get() else {
            panic!("expected captures array");
        };
        assert_eq!(
            captures.get(&ArrayKey::Int(0)),
            Some(&Value::packed_array(vec![
                Value::string("ABC-123"),
                Value::Int(0)
            ]))
        );
        assert_eq!(
            captures.get(&ArrayKey::Int(2)),
            Some(&Value::packed_array(vec![
                Value::string("123"),
                Value::Int(4)
            ]))
        );

        let all = ReferenceCell::new(Value::Null);
        assert_eq!(
            call_in_context(
                &mut context,
                "preg_match_all",
                vec![
                    Value::string(r#"/([a-z]+)=(\d+)/i"#),
                    Value::string("A=1 b=22"),
                    Value::Reference(all.clone()),
                    Value::Int(pcre::PREG_SET_ORDER | pcre::PREG_OFFSET_CAPTURE),
                ],
            ),
            Value::Int(2)
        );
        let Value::Array(rows) = all.get() else {
            panic!("expected match rows");
        };
        assert_eq!(rows.len(), 2);
        let no_matches = ReferenceCell::new(Value::Null);
        assert_eq!(
            call_in_context(
                &mut context,
                "preg_match_all",
                vec![
                    Value::string(r#"/%.+?%/"#),
                    Value::string("/"),
                    Value::Reference(no_matches.clone()),
                ],
            ),
            Value::Int(0)
        );
        let Value::Array(pattern_order) = no_matches.get() else {
            panic!("expected pattern-order no-match array");
        };
        assert_eq!(pattern_order.len(), 1);
        assert_eq!(
            pattern_order.get(&ArrayKey::Int(0)),
            Some(&Value::packed_array(Vec::new()))
        );
        assert_eq!(
            call_in_context(&mut context, "preg_last_error", Vec::new()),
            Value::Int(pcre::PREG_NO_ERROR)
        );
    }

    #[test]
    fn preg_replace_split_grep_quote_callback_and_errors_are_pcre2_backed() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let count = ReferenceCell::new(Value::Null);
        assert_eq!(
            call_in_context(
                &mut context,
                "preg_replace",
                vec![
                    Value::string(r#"/([a-z]+)=(\d+)/"#),
                    Value::string(r#"$1:$2"#),
                    Value::string("a=1 b=22"),
                    Value::Int(-1),
                    Value::Reference(count.clone()),
                ],
            ),
            Value::string("a:1 b:22")
        );
        assert_eq!(count.get(), Value::Int(2));

        assert_eq!(
            call_in_context(
                &mut context,
                "preg_replace",
                vec![
                    Value::string(r#"/(ab)(c)(d)(e)(f)(g)(h)(i)(j)(k)/"#),
                    Value::string(r#"a${1}2$103"#),
                    Value::string("zabcdefghijkl"),
                ],
            ),
            Value::string("zaab2k3l")
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "preg_replace",
                vec![
                    Value::string(r#"/(a)(b)/"#),
                    Value::string(r#"\1-$1-${1}-$10-${10}-$99-$001-${001}"#),
                    Value::string("ab"),
                ],
            ),
            Value::string("a-a-a----ab1-${001}")
        );

        let array_pattern_count = ReferenceCell::new(Value::Null);
        assert_eq!(
            call_in_context(
                &mut context,
                "preg_replace",
                vec![
                    Value::packed_array(vec![Value::string("/a/"), Value::string("/b/")]),
                    Value::packed_array(vec![Value::string("A"), Value::string("B")]),
                    Value::string("abc"),
                    Value::Int(-1),
                    Value::Reference(array_pattern_count.clone()),
                ],
            ),
            Value::string("ABc")
        );
        assert_eq!(array_pattern_count.get(), Value::Int(2));

        let short_replacement_count = ReferenceCell::new(Value::Null);
        assert_eq!(
            call_in_context(
                &mut context,
                "preg_replace",
                vec![
                    Value::packed_array(vec![
                        Value::string("/a/"),
                        Value::string("/b/"),
                        Value::string("/c/"),
                    ]),
                    Value::packed_array(vec![Value::string("A")]),
                    Value::string("abc"),
                    Value::Int(-1),
                    Value::Reference(short_replacement_count.clone()),
                ],
            ),
            Value::string("A")
        );
        assert_eq!(short_replacement_count.get(), Value::Int(3));

        let mut keyed_subject = PhpArray::new();
        keyed_subject.insert(
            ArrayKey::String(PhpString::from_test_str("first")),
            Value::string("aa"),
        );
        keyed_subject.insert(ArrayKey::Int(5), Value::string("aa"));
        let keyed_subject_count = ReferenceCell::new(Value::Null);
        let keyed_subject_result = call_in_context(
            &mut context,
            "preg_replace",
            vec![
                Value::string("/a/"),
                Value::string("A"),
                Value::Array(keyed_subject),
                Value::Int(1),
                Value::Reference(keyed_subject_count.clone()),
            ],
        );
        let Value::Array(keyed_subject_result) = keyed_subject_result else {
            panic!("expected keyed preg_replace subject array");
        };
        assert_eq!(
            keyed_subject_result.get(&ArrayKey::String(PhpString::from_test_str("first"))),
            Some(&Value::string("Aa"))
        );
        assert_eq!(
            keyed_subject_result.get(&ArrayKey::Int(5)),
            Some(&Value::string("Aa"))
        );
        assert_eq!(keyed_subject_count.get(), Value::Int(2));

        let filter_scalar_count = ReferenceCell::new(Value::Null);
        assert_eq!(
            call_in_context(
                &mut context,
                "preg_filter",
                vec![
                    Value::string("/z/"),
                    Value::string("Z"),
                    Value::string("abc"),
                    Value::Int(-1),
                    Value::Reference(filter_scalar_count.clone()),
                ],
            ),
            Value::Null
        );
        assert_eq!(filter_scalar_count.get(), Value::Int(0));
        let mut filter_subject = PhpArray::new();
        filter_subject.insert(ArrayKey::Int(0), Value::string("1"));
        filter_subject.insert(ArrayKey::Int(1), Value::string("a"));
        filter_subject.insert(ArrayKey::Int(2), Value::string("B"));
        let filter_count = ReferenceCell::new(Value::Null);
        let filter_result = call_in_context(
            &mut context,
            "preg_filter",
            vec![
                Value::packed_array(vec![Value::string(r#"/\d/"#), Value::string("/[a-z]/")]),
                Value::packed_array(vec![Value::string("A:$0"), Value::string("B:$0")]),
                Value::Array(filter_subject),
                Value::Int(-1),
                Value::Reference(filter_count.clone()),
            ],
        );
        let Value::Array(filter_result) = filter_result else {
            panic!("expected keyed preg_filter subject array");
        };
        assert_eq!(filter_result.len(), 2);
        assert_eq!(
            filter_result.get(&ArrayKey::Int(0)),
            Some(&Value::string("A:1"))
        );
        assert_eq!(
            filter_result.get(&ArrayKey::Int(1)),
            Some(&Value::string("B:a"))
        );
        assert_eq!(filter_count.get(), Value::Int(2));

        assert_eq!(
            call_in_context(
                &mut context,
                "preg_replace_callback",
                vec![
                    Value::string(r#"/(foo)/"#),
                    Value::internal_builtin_callable("count"),
                    Value::string("foo foo"),
                ],
            ),
            Value::string("2 2")
        );

        assert_eq!(
            array_strings(call_in_context(
                &mut context,
                "preg_split",
                vec![
                    Value::string(r#"/[,;]\s*/"#),
                    Value::string("a, b; c"),
                    Value::Int(-1),
                    Value::Int(pcre::PREG_SPLIT_NO_EMPTY),
                ],
            )),
            ["a", "b", "c"]
        );

        let input = Value::packed_array(vec![
            Value::string("src/Foo.php"),
            Value::string("README.md"),
            Value::string("tests/FooTest.php"),
        ]);
        assert_eq!(
            array_strings(call_in_context(
                &mut context,
                "preg_grep",
                vec![Value::string(r#"/\.php$/"#), input],
            )),
            ["src/Foo.php", "tests/FooTest.php"]
        );

        assert_eq!(
            call_in_context(
                &mut context,
                "preg_quote",
                vec![Value::string("a+b/c"), Value::string("/")],
            ),
            Value::string(r#"a\+b\/c"#)
        );

        assert_eq!(
            call_in_context(
                &mut context,
                "preg_match",
                vec![Value::string("/["), Value::string("x")],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_in_context(&mut context, "preg_last_error", Vec::new()),
            Value::Int(pcre::PREG_INTERNAL_ERROR)
        );
        assert_eq!(
            call_in_context(&mut context, "preg_last_error_msg", Vec::new()),
            Value::string("Internal error")
        );
    }

    #[test]
    fn date_timezone_defaults_set_and_list_are_request_local() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            call_in_context(&mut context, "date_default_timezone_get", Vec::new()),
            Value::string("UTC")
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "date_default_timezone_set",
                vec![Value::string("Europe/Berlin")],
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_in_context(&mut context, "date_default_timezone_get", Vec::new()),
            Value::string("Europe/Berlin")
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "date_default_timezone_set",
                vec![Value::string("+0000")],
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_in_context(&mut context, "date_default_timezone_get", Vec::new()),
            Value::string("+00:00")
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "date_default_timezone_set",
                vec![Value::string("Mars/Base")],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_in_context(&mut context, "date_default_timezone_get", Vec::new()),
            Value::string("+00:00")
        );

        let identifiers = array_strings(call_in_context(
            &mut context,
            "timezone_identifiers_list",
            Vec::new(),
        ));
        assert!(identifiers.contains(&"UTC".to_string()));
        assert!(identifiers.contains(&"Europe/Berlin".to_string()));
    }

    #[test]
    fn date_functions_parse_format_and_use_request_timezone() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            call_in_context(
                &mut context,
                "date",
                vec![Value::string("Y-m-d H:i:s O"), Value::Int(0)],
            ),
            Value::string("1970-01-01 00:00:00 +0000")
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "date_default_timezone_set",
                vec![Value::string("Europe/Berlin")],
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "date",
                vec![Value::string("Y-m-d H:i:s T"), Value::Int(0)],
            ),
            Value::string("1970-01-01 01:00:00 CET")
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "gmdate",
                vec![Value::string("Y-m-d H:i:s T O P"), Value::Int(0)],
            ),
            Value::string("1970-01-01 00:00:00 GMT +0000 +00:00")
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "strtotime",
                vec![Value::string("2024-01-02 03:04:05")],
            ),
            Value::Int(1_704_164_645)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "strtotime",
                vec![Value::string("2009-02-12 12:47:41 GMT")],
            ),
            Value::Int(1_234_442_861)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "strtotime",
                vec![Value::string("+2 days"), Value::Int(0)],
            ),
            Value::Int(172_800)
        );
        assert!(matches!(
            call_in_context(&mut context, "time", Vec::new()),
            Value::Int(value) if value > 0
        ));
        let Value::Array(hrtime) = call_in_context(&mut context, "hrtime", Vec::new()) else {
            panic!("hrtime() should return an array");
        };
        let entries = super::array_entries(&hrtime);
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0].1, Value::Int(value) if value > 0));
        assert!(matches!(entries[1].1, Value::Int(value) if (0..1_000_000_000).contains(&value)));
        assert!(matches!(
            call_in_context(&mut context, "hrtime", vec![Value::Bool(true)]),
            Value::Int(value) if value > 0
        ));
    }

    #[test]
    fn spl_object_identity_builtins_use_stable_runtime_object_ids() {
        let mut output = OutputBuffer::new();
        let object = Value::Object(ObjectRef::new(&empty_class("SplBox")));

        let Value::Int(id) = call("spl_object_id", vec![object.clone()], &mut output) else {
            panic!("expected object id int");
        };
        assert!(id > 0);
        assert_eq!(
            call("spl_object_id", vec![object.clone()], &mut output),
            Value::Int(id)
        );
        assert_eq!(
            call("spl_object_hash", vec![object], &mut output),
            Value::string(format!("{id:032x}"))
        );

        let closure = Value::closure(ClosurePayload::new(1, Vec::new()));
        let Value::Int(closure_id) = call("spl_object_id", vec![closure.clone()], &mut output)
        else {
            panic!("expected closure object id int");
        };
        assert!(closure_id > 0);
        assert_eq!(
            call("spl_object_hash", vec![closure], &mut output),
            Value::string(format!("{closure_id:032x}"))
        );
    }

    #[test]
    fn datetime_objects_cover_mutable_immutable_interval_and_diff_mvp() {
        let Value::Object(datetime) = datetime::datetime_object(0, "UTC") else {
            panic!("expected DateTime object");
        };
        assert_eq!(datetime.class_name(), "datetime");
        assert_eq!(datetime.display_name(), "DateTime");
        assert_eq!(
            datetime::format_timestamp(
                datetime::object_timestamp(&datetime).expect("timestamp"),
                &datetime::object_timezone(&datetime).expect("timezone"),
                "Y-m-d H:i:s"
            ),
            "1970-01-01 00:00:00"
        );

        let updated = datetime::with_timestamp(&datetime, 60, false);
        assert!(matches!(updated, Value::Object(_)));
        assert_eq!(datetime::object_timestamp(&datetime), Some(60));

        let Value::Object(immutable) = datetime::datetime_immutable_object(0, "UTC") else {
            panic!("expected DateTimeImmutable object");
        };
        let changed = datetime::with_timestamp(&immutable, 60, true);
        let Value::Object(changed) = changed else {
            panic!("expected changed immutable object");
        };
        assert_eq!(datetime::object_timestamp(&immutable), Some(0));
        assert_eq!(datetime::object_timestamp(&changed), Some(60));
        assert_eq!(changed.class_name(), "datetimeimmutable");
        assert_eq!(changed.display_name(), "DateTimeImmutable");

        let interval_seconds = datetime::parse_interval_spec("P1DT2H").expect("interval");
        assert_eq!(interval_seconds, 93_600);
        let added = datetime::add_interval(&immutable, interval_seconds, true);
        let Value::Object(added) = added else {
            panic!("expected DateTimeImmutable after add");
        };
        assert_eq!(datetime::object_timestamp(&added), Some(93_600));
        let diff = datetime::diff_objects(&immutable, &added);
        let Value::Object(diff) = diff else {
            panic!("expected DateInterval object");
        };
        assert_eq!(diff.class_name(), "dateinterval");
        assert_eq!(diff.display_name(), "DateInterval");
        assert_eq!(diff.get_property("__seconds"), Some(Value::Int(93_600)));

        let modified = datetime::modify_object(&immutable, "+1 day", true).expect("modify");
        let Value::Object(modified) = modified else {
            panic!("expected modified object");
        };
        assert_eq!(datetime::object_timestamp(&modified), Some(86_400));
        assert!(datetime::modify_object(&immutable, "next tuesday", true).is_none());
    }

    #[test]
    fn json_builtins_cover_composer_style_documents_and_modes() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let decoded = call_in_context(
            &mut context,
            "json_decode",
            vec![
                Value::string(r#"{"name":"pkg","autoload":{"psr-4":{"App\\":"src/"}}}"#),
                Value::Bool(true),
            ],
        );
        let Value::Array(root) = decoded else {
            panic!("expected associative json array");
        };
        assert_eq!(
            root.get(&ArrayKey::String(PhpString::from_test_str("name"))),
            Some(&Value::string("pkg"))
        );
        assert!(matches!(
            root.get(&ArrayKey::String(PhpString::from_test_str("autoload"))),
            Some(Value::Array(_))
        ));

        let object = call_in_context(
            &mut context,
            "json_decode",
            vec![Value::string(r#"{"answer":42}"#)],
        );
        let Value::Object(object) = object else {
            panic!("expected stdClass object");
        };
        assert_eq!(object.class_name(), "stdclass");
        assert_eq!(object.display_name(), "stdClass");
        assert_eq!(object.get_property("answer"), Some(Value::Int(42)));

        let decoded_with_flag = call_in_context(
            &mut context,
            "json_decode",
            vec![
                Value::string(r#"{"answer":42}"#),
                Value::Null,
                Value::Int(512),
                Value::Int(JSON_OBJECT_AS_ARRAY),
            ],
        );
        assert!(matches!(decoded_with_flag, Value::Array(_)));

        let mut mixed = crate::PhpArray::new();
        mixed.insert(
            ArrayKey::String(PhpString::from_test_str("name")),
            Value::string("pkg"),
        );
        mixed.insert(
            ArrayKey::String(PhpString::from_test_str("versions")),
            Value::packed_array(vec![Value::string("1.0.0"), Value::string("1.1.0")]),
        );
        assert_eq!(
            call_in_context(&mut context, "json_encode", vec![Value::Array(mixed)]),
            Value::string(r#"{"name":"pkg","versions":["1.0.0","1.1.0"]}"#)
        );
        let mut ordered = crate::PhpArray::new();
        ordered.insert(
            ArrayKey::String(PhpString::from_test_str("url")),
            Value::string("https://example.test/a"),
        );
        ordered.insert(
            ArrayKey::String(PhpString::from_test_str("snow")),
            Value::string("☃"),
        );
        ordered.insert(
            ArrayKey::String(PhpString::from_test_str("n")),
            Value::float(1.0),
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "json_encode",
                vec![Value::Array(ordered.clone())]
            ),
            Value::string(r#"{"url":"https:\/\/example.test\/a","snow":"\u2603","n":1}"#)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "json_encode",
                vec![
                    Value::Array(ordered),
                    Value::Int(JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE)
                ]
            ),
            Value::string(r#"{"url":"https://example.test/a","snow":"☃","n":1}"#)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "json_encode",
                vec![
                    Value::packed_array(vec![Value::Int(1)]),
                    Value::Int(JSON_FORCE_OBJECT)
                ]
            ),
            Value::string(r#"{"0":1}"#)
        );
        let hex_flags = JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT;
        assert_eq!(
            call_in_context(
                &mut context,
                "json_encode",
                vec![Value::string("<tag>&'\""), Value::Int(hex_flags)]
            ),
            Value::string(r#""\u003Ctag\u003E\u0026\u0027\u0022""#)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "json_encode",
                vec![Value::string("9.4324"), Value::Int(JSON_NUMERIC_CHECK)]
            ),
            Value::string("9.4324")
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "json_encode",
                vec![
                    Value::packed_array(vec![Value::string("122321"), Value::string("plain")]),
                    Value::Int(JSON_NUMERIC_CHECK)
                ]
            ),
            Value::string(r#"[122321,"plain"]"#)
        );
        assert_eq!(
            call_in_context(&mut context, "json_encode", vec![Value::float(42.0)]),
            Value::string("42")
        );
        let flags = JSON_PRETTY_PRINT
            | JSON_UNESCAPED_SLASHES
            | JSON_UNESCAPED_UNICODE
            | JSON_PRESERVE_ZERO_FRACTION;
        let encoded_with_flags = call_in_context(
            &mut context,
            "json_encode",
            vec![
                Value::packed_array(vec![
                    Value::string("https://example.test/ü"),
                    Value::float(1.0),
                ]),
                Value::Int(flags),
            ],
        );
        let Value::String(encoded_with_flags) = encoded_with_flags else {
            panic!("expected encoded JSON string");
        };
        let encoded_with_flags = encoded_with_flags.to_string_lossy();
        assert!(encoded_with_flags.contains('\n'));
        assert!(encoded_with_flags.contains("\n    \"https://example.test/ü\""));
        assert!(encoded_with_flags.contains("https://example.test/ü"));
        assert!(encoded_with_flags.contains("1.0"));
        assert_eq!(
            call_in_context(&mut context, "json_last_error", Vec::new()),
            Value::Int(JSON_ERROR_NONE)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "json_decode",
                vec![
                    Value::string(r#"[123456789012345678901234567890]"#),
                    Value::Null,
                    Value::Int(512),
                    Value::Int(JSON_BIGINT_AS_STRING)
                ]
            ),
            Value::packed_array(vec![Value::string("123456789012345678901234567890")])
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "json_decode",
                vec![Value::string("[[1]]"), Value::Null, Value::Int(2)]
            ),
            Value::Null
        );
        assert_eq!(
            call_in_context(&mut context, "json_last_error", Vec::new()),
            Value::Int(JSON_ERROR_DEPTH)
        );
        assert_eq!(
            call_in_context(&mut context, "json_decode", vec![Value::string("[1}")]),
            Value::Null
        );
        assert_eq!(
            call_in_context(&mut context, "json_last_error", Vec::new()),
            Value::Int(JSON_ERROR_STATE_MISMATCH)
        );
        assert_eq!(
            call_in_context(&mut context, "json_decode", vec![Value::string("\"a\0b\"")]),
            Value::Null
        );
        assert_eq!(
            call_in_context(&mut context, "json_last_error", Vec::new()),
            Value::Int(JSON_ERROR_CTRL_CHAR)
        );
        assert_eq!(
            call_in_context(
                &mut context,
                "json_validate",
                vec![Value::string("[1,2,3]")]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn json_errors_are_recorded_and_throw_flag_errors() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            call_in_context(&mut context, "json_decode", vec![Value::string("{")]),
            Value::Null
        );
        assert_eq!(
            call_in_context(&mut context, "json_last_error", Vec::new()),
            Value::Int(JSON_ERROR_SYNTAX)
        );
        assert_eq!(
            call_in_context(&mut context, "json_last_error_msg", Vec::new()),
            Value::string("Syntax error")
        );
        assert_eq!(
            call_in_context(&mut context, "json_validate", vec![Value::string("{")]),
            Value::Bool(false)
        );

        let entry = BuiltinRegistry::new()
            .get("json_decode")
            .expect("json_decode exists");
        let result = (entry.function())(
            &mut context,
            vec![
                Value::string("{"),
                Value::Null,
                Value::Int(512),
                Value::Int(JSON_THROW_ON_ERROR),
            ],
            RuntimeSourceSpan::default(),
        );
        assert!(matches!(
            result,
            Err(error) if error.diagnostic_id() == "E_PHP_RUNTIME_JSON_EXCEPTION"
        ));
    }

    #[test]
    fn symlink_stat_is_conditional_on_platform_support() {
        let root = std::env::temp_dir().join(format!("phrust-stdlib-lstat-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temp root");
        let target = root.join("target.txt");
        let link = root.join("link.txt");
        std::fs::write(&target, b"target").expect("write target");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");
        #[cfg(windows)]
        {
            if std::os::windows::fs::symlink_file(&target, &link).is_err() {
                let _ = std::fs::remove_file(target);
                let _ = std::fs::remove_dir(root);
                return;
            }
        }

        let mut output = OutputBuffer::new();
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        assert_eq!(
            call_with_fs(
                "is_link",
                vec![Value::string("link.txt")],
                &mut output,
                root.clone(),
                capabilities.clone()
            ),
            Value::Bool(true)
        );
        assert!(matches!(
            call_with_fs(
                "lstat",
                vec![Value::string("link.txt")],
                &mut output,
                root.clone(),
                capabilities
            ),
            Value::Array(_)
        ));

        let _ = std::fs::remove_file(link);
        let _ = std::fs::remove_file(target);
        let _ = std::fs::remove_dir(root);
    }

    fn empty_class(name: &str) -> ClassEntry {
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

    #[test]
    fn builtins_var_dump_is_stable_for_scalars_and_arrays() {
        let mut output = OutputBuffer::new();
        let result = call(
            "var_dump",
            vec![
                Value::Null,
                Value::Bool(true),
                Value::Int(7),
                Value::float(1.0),
                Value::float(1.7000000000000002),
                Value::float(3.9000000000000004),
                Value::float(4.2),
                Value::float(f64::INFINITY),
                Value::float(f64::NAN),
                Value::float(9_223_372_036_854_776_000.0),
                Value::string("hi"),
                Value::packed_array(vec![Value::Int(1), Value::string("x")]),
            ],
            &mut output,
        );

        assert_eq!(result, Value::Null);
        assert_eq!(
            output.to_string_lossy(),
            "NULL\nbool(true)\nint(7)\nfloat(1)\nfloat(1.7000000000000002)\nfloat(3.9000000000000004)\nfloat(4.2)\nfloat(INF)\nfloat(NAN)\nfloat(9.223372036854776E+18)\nstring(2) \"hi\"\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"x\"\n}\n"
        );
    }

    #[test]
    fn var_dump_marks_array_references_to_active_arrays_as_recursion() {
        let cell = ReferenceCell::new(Value::Null);
        let mut array = PhpArray::new();
        array.append(Value::Reference(cell.clone()));
        cell.set(Value::Array(array.clone()));

        let mut output = OutputBuffer::new();
        let result = call("var_dump", vec![Value::Array(array)], &mut output);

        assert_eq!(result, Value::Null);
        assert_eq!(
            output.to_string_lossy(),
            "array(1) {\n  [0]=>\n  *RECURSION*\n}\n"
        );
    }

    #[test]
    fn var_dump_marks_object_references_to_active_objects_as_recursion() {
        let object = ObjectRef::new(&empty_class("DebugBox"));
        let cell = ReferenceCell::new(Value::Object(object.clone()));
        object.set_property("self", Value::Reference(cell));

        let mut output = OutputBuffer::new();
        let result = call("var_dump", vec![Value::Object(object)], &mut output);

        assert_eq!(result, Value::Null);
        assert!(output.to_string_lossy().contains("*RECURSION*\n"));
    }

    #[test]
    fn var_dump_prints_callable_closure_metadata() {
        let mut output = OutputBuffer::new();
        let result = call(
            "var_dump",
            vec![
                Value::user_function_callable("test1"),
                Value::closure(crate::ClosurePayload::new(3, Vec::new()).with_debug(Some(
                    crate::ClosureDebugInfo {
                        name: "{closure:/tmp/source.php:7}".to_owned(),
                        file: "/tmp/source.php".to_owned(),
                        line: 7,
                        parameters: vec![crate::ClosureDebugParameter {
                            name: "class".to_owned(),
                            required: true,
                        }],
                    },
                ))),
                Value::closure(
                    crate::ClosurePayload::new(
                        4,
                        vec![crate::ClosureCaptureValue::by_value(
                            "x".to_owned(),
                            Value::Int(2),
                        )],
                    )
                    .with_debug(Some(crate::ClosureDebugInfo {
                        name: "{closure:/tmp/source.php:9}".to_owned(),
                        file: "/tmp/source.php".to_owned(),
                        line: 9,
                        parameters: Vec::new(),
                    })),
                ),
            ],
            &mut output,
        );

        assert_eq!(result, Value::Null);
        let dumped = output.to_string_lossy();
        let closure_headers = dumped
            .lines()
            .filter(|line| line.starts_with("object(Closure)#"))
            .collect::<Vec<_>>();
        assert_eq!(closure_headers.len(), 3);
        assert_eq!(closure_headers[0], "object(Closure)#1 (1) {");
        assert!(closure_headers[1].ends_with(" (4) {"));
        assert!(closure_headers[2].ends_with(" (4) {"));
        assert_ne!(
            closure_debug_id(closure_headers[1]),
            closure_debug_id(closure_headers[2])
        );
        assert!(dumped.contains("string(27) \"{closure:/tmp/source.php:7}\""));
        assert!(dumped.contains("string(27) \"{closure:/tmp/source.php:9}\""));
        assert!(dumped.contains("[\"parameter\"]=>\n  array(1) {"));
        assert!(dumped.contains("[\"$class\"]=>\n    string(10) \"<required>\""));
        assert!(dumped.contains("[\"static\"]=>\n  array(1) {"));
    }

    fn closure_debug_id(header: &str) -> &str {
        header
            .split_once('#')
            .and_then(|(_, rest)| rest.split_once(' '))
            .map(|(id, _)| id)
            .expect("closure var_dump header should include an object handle")
    }

    #[test]
    fn var_dump_orders_closure_debug_fields_like_reference_php() {
        // Reference PHP 8.5 emits name, file, line, static, this, parameter.
        // Keep this asserted without the reference oracle so CI catches a
        // reorder even when REFERENCE_PHP is unavailable.
        let mut output = OutputBuffer::new();
        let bound_this = ObjectRef::new(&empty_class("BoundTarget"));
        call(
            "var_dump",
            vec![Value::closure(
                crate::ClosurePayload::new(
                    11,
                    vec![crate::ClosureCaptureValue::by_value(
                        "captured".to_owned(),
                        Value::Int(5),
                    )],
                )
                .with_bound_this(Some(bound_this))
                .with_debug(Some(crate::ClosureDebugInfo {
                    name: "{closure:/tmp/order.php:3}".to_owned(),
                    file: "/tmp/order.php".to_owned(),
                    line: 3,
                    parameters: vec![crate::ClosureDebugParameter {
                        name: "p".to_owned(),
                        required: true,
                    }],
                })),
            )],
            &mut output,
        );

        let dumped = output.to_string_lossy();
        let order = [
            "[\"name\"]",
            "[\"file\"]",
            "[\"line\"]",
            "[\"static\"]",
            "[\"this\"]",
            "[\"parameter\"]",
        ]
        .map(|field| {
            dumped
                .find(field)
                .unwrap_or_else(|| panic!("{field} missing from closure var_dump:\n{dumped}"))
        });
        assert!(
            order.windows(2).all(|pair| pair[0] < pair[1]),
            "closure var_dump fields out of reference order:\n{dumped}"
        );
    }

    #[test]
    fn print_r_marks_array_references_to_active_arrays_as_recursion() {
        let outer_cell = ReferenceCell::new(Value::Null);
        let inner_cell = ReferenceCell::new(Value::Null);
        let mut inner = PhpArray::new();
        inner.append(Value::Reference(outer_cell.clone()));
        inner_cell.set(Value::Array(inner.clone()));
        let mut outer = PhpArray::new();
        outer.append(Value::Reference(inner_cell));
        outer_cell.set(Value::Array(outer.clone()));

        let mut output = OutputBuffer::new();
        let result = call("print_r", vec![Value::Array(outer)], &mut output);

        assert_eq!(result, Value::Bool(true));
        assert_eq!(
            output.to_string_lossy(),
            "Array\n(\n    [0] => Array\n        (\n            [0] => Array\n *RECURSION*\n        )\n\n)\n"
        );
    }

    #[test]
    fn debug_output_builtins_cover_return_modes_and_cycles() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "print_r",
                vec![Value::packed_array(vec![Value::Int(1)]), Value::Bool(true)],
                &mut output
            ),
            Value::string("Array\n(\n    [0] => 1\n)\n")
        );
        assert_eq!(
            call(
                "print_r",
                vec![
                    Value::packed_array(vec![Value::packed_array(vec![Value::Int(1)])]),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::string(
                "Array\n(\n    [0] => Array\n        (\n            [0] => 1\n        )\n\n)\n"
            )
        );
        let object = ObjectRef::new_with_display_name(&empty_class("A"), "A");
        let mut property_array = PhpArray::new();
        property_array.insert(ArrayKey::Int(1), Value::string("foo1_value"));
        property_array.insert(ArrayKey::Int(2), Value::string("foo2_value"));
        object.set_property("a_var", Value::Array(property_array));
        assert_eq!(
            call(
                "print_r",
                vec![Value::Object(object), Value::Bool(true)],
                &mut output
            ),
            Value::string(
                "A Object\n(\n    [a_var] => Array\n        (\n            [1] => foo1_value\n            [2] => foo2_value\n        )\n\n)\n"
            )
        );
        assert_eq!(
            call(
                "var_export",
                vec![
                    Value::packed_array(vec![Value::string("x")]),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::string("array (\n  0 => 'x',\n)")
        );
        assert_eq!(
            call(
                "var_export",
                vec![
                    Value::packed_array(vec![Value::packed_array(vec![Value::Int(1)])]),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::string("array (\n  0 => \n  array (\n    0 => 1,\n  ),\n)")
        );
        let mut nul_key_array = PhpArray::new();
        nul_key_array.insert(
            ArrayKey::String(PhpString::from_bytes(vec![0])),
            Value::string("null"),
        );
        assert_eq!(
            call(
                "var_export",
                vec![Value::Array(nul_key_array), Value::Bool(true)],
                &mut output
            ),
            Value::string("array (\n  '' . \"\\0\" . '' => 'null',\n)")
        );
        assert_eq!(
            call(
                "var_export",
                vec![Value::float(1.0), Value::Bool(true)],
                &mut output
            ),
            Value::string("1.0")
        );
        assert_eq!(
            call(
                "var_export",
                vec![Value::float(-0.0), Value::Bool(true)],
                &mut output
            ),
            Value::string("-0.0")
        );
        assert_eq!(
            call(
                "var_export",
                vec![Value::float(10_000_000_000_000_000.0), Value::Bool(true)],
                &mut output
            ),
            Value::string("10000000000000000.0")
        );
        let std_class = ObjectRef::new(&empty_class("stdClass"));
        std_class.set_property("0", Value::Int(1));
        std_class.set_property("foo", Value::packed_array(vec![Value::Int(2)]));
        assert_eq!(
            call(
                "var_export",
                vec![Value::Object(std_class), Value::Bool(true)],
                &mut output
            ),
            Value::string(
                "(object) array(\n   '0' => 1,\n   'foo' => \n  array (\n    0 => 2,\n  ),\n)"
            )
        );
        let debug_box = ObjectRef::new_with_display_name(&empty_class("DebugBox"), "DebugBox");
        debug_box.set_property("x", Value::Int(1));
        assert_eq!(
            call(
                "var_export",
                vec![Value::Object(debug_box), Value::Bool(true)],
                &mut output
            ),
            Value::string("\\DebugBox::__set_state(array(\n   'x' => 1,\n))")
        );
        let fixed_array =
            ObjectRef::new_with_display_name(&empty_class("MySplFixedArray"), "MySplFixedArray");
        fixed_array.set_property("__spl_runtime_class", Value::string("splfixedarray"));
        fixed_array.set_property(
            "__entries",
            Value::packed_array(vec![Value::packed_array(vec![
                Value::Int(0),
                Value::Object(fixed_array.clone()),
            ])]),
        );
        assert_eq!(
            call(
                "var_export",
                vec![Value::Object(fixed_array), Value::Bool(true)],
                &mut output
            ),
            Value::string("\\MySplFixedArray::__set_state(array(\n   0 => NULL,\n))")
        );
        assert!(
            output
                .to_string_lossy()
                .contains("var_export does not handle circular references")
        );

        let cell = ReferenceCell::new(Value::Null);
        let mut array = PhpArray::new();
        array.append(Value::Reference(cell.clone()));
        cell.set(Value::Array(array));

        let result = call("var_dump", vec![Value::Reference(cell)], &mut output);
        assert_eq!(result, Value::Null);
        assert!(output.to_string_lossy().contains("*RECURSION*"));
    }

    #[test]
    fn version_compare_covers_platform_check_semantics() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "version_compare",
                vec![Value::string("8.5.7"), Value::string("8.5.0")],
                &mut output
            ),
            Value::Int(1)
        );
        assert_eq!(
            call(
                "version_compare",
                vec![
                    Value::string("8.5.7"),
                    Value::string("8.5.7"),
                    Value::string("eq")
                ],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "version_compare",
                vec![
                    Value::string("8.5.7-dev"),
                    Value::string("8.5.7"),
                    Value::string("<")
                ],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "version_compare",
                vec![
                    Value::string("8.5.7RC1"),
                    Value::string("8.5.7"),
                    Value::string("lt")
                ],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "version_compare",
                vec![
                    Value::string("8.5.7pl1"),
                    Value::string("8.5.7"),
                    Value::string("gt")
                ],
                &mut output
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn string_search_and_compare_builtins_are_binary_safe() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call("strlen", vec![Value::string(b"a\0b".to_vec())], &mut output),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "substr",
                vec![Value::string("abcdef"), Value::Int(-3), Value::Int(2)],
                &mut output
            ),
            Value::string("de")
        );
        assert_eq!(
            call(
                "strpos",
                vec![
                    Value::string(b"a\0b\0c".to_vec()),
                    Value::string(b"\0b".to_vec())
                ],
                &mut output
            ),
            Value::Int(1)
        );
        assert_eq!(
            call(
                "stripos",
                vec![Value::string("AbCd"), Value::string("bc")],
                &mut output
            ),
            Value::Int(1)
        );
        assert_eq!(
            call(
                "strrpos",
                vec![Value::string("abcabc"), Value::string("a"), Value::Int(-1)],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "strrpos",
                vec![Value::string("abcabc"), Value::string("a"), Value::Int(2)],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "strrpos",
                vec![
                    Value::string("abcabc"),
                    Value::string("abcabc"),
                    Value::Int(1)
                ],
                &mut output
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "strrpos",
                vec![Value::string("abcabc"), Value::string("a"), Value::Int(-4)],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "strrpos",
                vec![Value::string("abc"), Value::string("")],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "strrpos",
                vec![Value::string("abc"), Value::string(""), Value::Int(-1)],
                &mut output
            ),
            Value::Int(2)
        );
        assert_eq!(
            call_error(
                "strrpos",
                vec![Value::string("abc"), Value::string("a"), Value::Int(10)],
                &mut output
            ),
            "strrpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
        );
        assert_eq!(
            call_error(
                "strrpos",
                vec![
                    Value::string("abc"),
                    Value::string("a"),
                    Value::float(f64::INFINITY)
                ],
                &mut output
            ),
            "strrpos(): Argument #3 ($offset) must be of type int, float given"
        );
        assert_eq!(
            call(
                "strripos",
                vec![Value::string("AbCaBc"), Value::string("bc")],
                &mut output
            ),
            Value::Int(4)
        );
        assert_eq!(
            call(
                "strrchr",
                vec![Value::string("abcabc"), Value::string("ab")],
                &mut output
            ),
            Value::string("abc")
        );
        assert_eq!(
            call(
                "strrchr",
                vec![Value::string("Hello, World"), Value::string("World")],
                &mut output
            ),
            Value::string("World")
        );
        assert_eq!(
            call(
                "strrchr",
                vec![
                    Value::string("Hello, World"),
                    Value::string("World"),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::string("Hello, ")
        );
        assert_eq!(
            call(
                "strrchr",
                vec![Value::string(b"Hello\0World".to_vec()), Value::string("")],
                &mut output
            ),
            Value::string(b"\0World".to_vec())
        );
        assert_eq!(
            call(
                "strrchr",
                vec![
                    Value::string(b"Hello\0World".to_vec()),
                    Value::string(""),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::string("Hello")
        );
        assert_eq!(
            call(
                "strstr",
                vec![
                    Value::string("abcabc"),
                    Value::string("bc"),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::string("a")
        );
        assert_eq!(
            call(
                "strstr",
                vec![Value::string("abc"), Value::string("")],
                &mut output
            ),
            Value::string("abc")
        );
        assert_eq!(
            call(
                "strstr",
                vec![Value::string("abc"), Value::string(""), Value::Bool(true)],
                &mut output
            ),
            Value::string("")
        );
        assert_eq!(
            call(
                "stristr",
                vec![Value::string("AbCaBc"), Value::string("bc")],
                &mut output
            ),
            Value::string("bCaBc")
        );
        assert_eq!(
            call(
                "stristr",
                vec![Value::string("AbC"), Value::string("")],
                &mut output
            ),
            Value::string("AbC")
        );
        assert_eq!(
            call_error(
                "stristr",
                vec![Value::string("abc"), Value::Array(PhpArray::new())],
                &mut output
            ),
            "stristr(): Argument #2 ($needle) must be of type string, array given"
        );
        let mut resources = ResourceTable::new();
        let stream = resources.register_stream(
            StreamFlags::new(true, false, true),
            StreamMetadata::new("plainfile", "stream", "r", "/tmp/example.php"),
        );
        assert_eq!(
            call_error(
                "stristr",
                vec![Value::string("abc"), Value::Resource(stream)],
                &mut output
            ),
            "stristr(): Argument #2 ($needle) must be of type string, resource given"
        );
        assert_eq!(
            call(
                "strpbrk",
                vec![Value::string("abc"), Value::string("cb")],
                &mut output
            ),
            Value::string("bc")
        );
        assert_eq!(
            call_error(
                "strpbrk",
                vec![Value::string("abc"), Value::string("")],
                &mut output
            ),
            "strpbrk(): Argument #2 ($characters) must be a non-empty string"
        );
        assert_eq!(
            call(
                "substr_count",
                vec![Value::string("aaaa"), Value::string("aa")],
                &mut output
            ),
            Value::Int(2)
        );
        assert_eq!(
            call(
                "substr_count",
                vec![
                    Value::string("abcabc"),
                    Value::string("bc"),
                    Value::Int(0),
                    Value::Null
                ],
                &mut output
            ),
            Value::Int(2)
        );
        assert_eq!(
            call_error(
                "substr_count",
                vec![Value::string("abc"), Value::string("")],
                &mut output
            ),
            "substr_count(): Argument #2 ($needle) must not be empty"
        );
        assert_eq!(
            call_error(
                "substr_count",
                vec![Value::string("abc"), Value::string("a"), Value::Int(10)],
                &mut output
            ),
            "substr_count(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
        );
        assert_eq!(
            call_error(
                "substr_count",
                vec![
                    Value::string("abc"),
                    Value::string("a"),
                    Value::Int(1),
                    Value::Int(10)
                ],
                &mut output
            ),
            "substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack)"
        );
        assert_eq!(
            call(
                "substr_compare",
                vec![
                    Value::string("abc"),
                    Value::string("BC"),
                    Value::Int(1),
                    Value::Int(2),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "substr_compare",
                vec![
                    Value::string("abcde"),
                    Value::string("df"),
                    Value::Int(-2),
                    Value::Null
                ],
                &mut output
            ),
            Value::Int(-1)
        );
        assert_eq!(
            call(
                "substr_compare",
                vec![
                    Value::string("abcde"),
                    Value::string("abcdef"),
                    Value::Int(-10),
                    Value::Int(10)
                ],
                &mut output
            ),
            Value::Int(-1)
        );
        assert_eq!(
            call_error(
                "substr_compare",
                vec![
                    Value::string("abcde"),
                    Value::string("abc"),
                    Value::Int(0),
                    Value::Int(-1)
                ],
                &mut output
            ),
            "substr_compare(): Argument #4 ($length) must be greater than or equal to 0"
        );
        assert_eq!(
            call_error(
                "strncmp",
                vec![Value::string("a"), Value::string("b"), Value::Int(-1)],
                &mut output
            ),
            "strncmp(): Argument #3 ($length) must be greater than or equal to 0"
        );
        assert_eq!(
            call_error(
                "strncasecmp",
                vec![Value::string("a"), Value::string("b"), Value::Int(-1)],
                &mut output
            ),
            "strncasecmp(): Argument #3 ($length) must be greater than or equal to 0"
        );
        assert_eq!(
            call(
                "strncasecmp",
                vec![
                    Value::string(b"Hello\0world".to_vec()),
                    Value::string(b"Hello\0".to_vec()),
                    Value::Int(12)
                ],
                &mut output
            ),
            Value::Int(1)
        );
        assert_eq!(
            call(
                "strncasecmp",
                vec![
                    Value::string(b"Hello,\0world".to_vec()),
                    Value::string("Hello,world"),
                    Value::Int(12)
                ],
                &mut output
            ),
            Value::Int(-119)
        );
        assert_eq!(
            call(
                "strspn",
                vec![
                    Value::string("abc123"),
                    Value::string("abc"),
                    Value::Int(0),
                    Value::Int(4)
                ],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "strspn",
                vec![Value::string("abc"), Value::string("abc"), Value::Int(4)],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "strspn",
                vec![Value::string("abc"), Value::string("abc"), Value::Int(-4)],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "strspn",
                vec![
                    Value::string("abc"),
                    Value::string("abc"),
                    Value::Int(0),
                    Value::Int(-4)
                ],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "strcspn",
                vec![
                    Value::string("abc123"),
                    Value::string("123"),
                    Value::Int(0),
                    Value::Int(6)
                ],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "strcspn",
                vec![Value::string("abc"), Value::string("x"), Value::Int(4)],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "strcspn",
                vec![Value::string("abc"), Value::string("x"), Value::Int(-4)],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "strcspn",
                vec![
                    Value::string("abc"),
                    Value::string("x"),
                    Value::Int(0),
                    Value::Int(-4)
                ],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "str_contains",
                vec![Value::string("abc"), Value::string("")],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "str_starts_with",
                vec![Value::string("abc"), Value::string("ab")],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "str_ends_with",
                vec![Value::string("abc"), Value::string("bc")],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "strcmp",
                vec![Value::string("a"), Value::string("b")],
                &mut output
            ),
            Value::Int(-1)
        );
        assert_eq!(
            call(
                "strncmp",
                vec![Value::string("abc"), Value::string("abd"), Value::Int(2)],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "strcasecmp",
                vec![Value::string("ABC"), Value::string("abc")],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "strncasecmp",
                vec![Value::string("ABx"), Value::string("aby"), Value::Int(2)],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "addslashes",
                vec![Value::string(b"a\0b\"c\\d".to_vec())],
                &mut output
            ),
            Value::string(b"a\\0b\\\"c\\\\d".to_vec())
        );
        assert_eq!(
            call(
                "addcslashes",
                vec![Value::string(b"100%_a\\b".to_vec()), Value::string("_%\\")],
                &mut output
            ),
            Value::string(b"100\\%\\_a\\\\b".to_vec())
        );
        assert_eq!(
            call(
                "addcslashes",
                vec![Value::string("Ab-1"), Value::string("A..Za..z")],
                &mut output
            ),
            Value::string("\\A\\b-1")
        );
        assert_eq!(
            call(
                "addcslashes",
                vec![
                    Value::string(b"A\0\n\t\x7f".to_vec()),
                    Value::string(b"A\0\n\t\x7f".to_vec())
                ],
                &mut output
            ),
            Value::string(b"\\A\\000\\n\\t\\177".to_vec())
        );
        assert_eq!(
            call(
                "stripslashes",
                vec![Value::string(b"a\\0b\\\"c\\\\d".to_vec())],
                &mut output
            ),
            Value::string(b"a\0b\"c\\d".to_vec())
        );
        assert_eq!(
            call(
                "stripcslashes",
                vec![Value::string(br"hello\n\x57\157rld".to_vec())],
                &mut output
            ),
            Value::string(b"hello\nWorld".to_vec())
        );
    }

    #[test]
    fn string_builtins_report_value_errors() {
        for (name, args) in [
            (
                "strpos",
                vec![Value::string("abc"), Value::string("a"), Value::Int(4)],
            ),
            (
                "strncmp",
                vec![Value::string("a"), Value::string("a"), Value::Int(-1)],
            ),
        ] {
            let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
            let mut output = OutputBuffer::new();
            let mut context = BuiltinContext::new(&mut output);
            let error = (entry.function())(&mut context, args, RuntimeSourceSpan::default())
                .expect_err("expected value error");
            assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
        }
    }

    #[test]
    fn strtok_warns_after_delimiter_only_input_needs_new_input() {
        let mut output = OutputBuffer::new();
        let mut state = StrtokState::default();
        let diagnostics = {
            let mut context = BuiltinContext::new(&mut output);
            context.set_strtok_state(&mut state);
            assert_eq!(
                call_in_context(
                    &mut context,
                    "strtok",
                    vec![Value::string(b"\0".to_vec()), Value::string(b"\0".to_vec()),],
                ),
                Value::Bool(false)
            );
            assert_eq!(
                call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
                Value::Bool(false)
            );
            context.take_diagnostics()
        };

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), "E_PHP_RUNTIME_STRTOK_MISSING_INPUT");
        assert_eq!(
            diagnostics[0].message(),
            "strtok(): Both arguments must be provided when starting tokenization"
        );
    }

    #[test]
    fn strtok_single_trailing_delimiter_exhausts_without_warning() {
        let mut output = OutputBuffer::new();
        let mut state = StrtokState::default();
        let diagnostics = {
            let mut context = BuiltinContext::new(&mut output);
            context.set_strtok_state(&mut state);
            assert_eq!(
                call_in_context(
                    &mut context,
                    "strtok",
                    vec![
                        Value::string(b"a\0".to_vec()),
                        Value::string(b"\0".to_vec()),
                    ],
                ),
                Value::string("a")
            );
            assert_eq!(
                call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
                Value::Bool(false)
            );
            assert_eq!(
                call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
                Value::Bool(false)
            );
            context.take_diagnostics()
        };

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn strtok_warns_after_multi_trailing_delimiter_grace_false() {
        let mut output = OutputBuffer::new();
        let mut state = StrtokState::default();
        let diagnostics = {
            let mut context = BuiltinContext::new(&mut output);
            context.set_strtok_state(&mut state);
            assert_eq!(
                call_in_context(
                    &mut context,
                    "strtok",
                    vec![
                        Value::string(b"a\0\0".to_vec()),
                        Value::string(b"\0".to_vec()),
                    ],
                ),
                Value::string("a")
            );
            assert_eq!(
                call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
                Value::Bool(false)
            );
            assert_eq!(
                call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
                Value::Bool(false)
            );
            context.take_diagnostics()
        };

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), "E_PHP_RUNTIME_STRTOK_MISSING_INPUT");
    }

    #[test]
    fn string_split_replace_case_and_padding_builtins_work() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "explode",
                vec![Value::string(","), Value::string("a,b,c")],
                &mut output
            ),
            Value::packed_array(vec![
                Value::string("a"),
                Value::string("b"),
                Value::string("c")
            ])
        );
        assert_eq!(
            call(
                "implode",
                vec![
                    Value::string("|"),
                    Value::packed_array(vec![Value::string("a"), Value::string("b")]),
                ],
                &mut output,
            ),
            Value::string("a|b")
        );
        assert_eq!(
            call(
                "join",
                vec![Value::packed_array(vec![
                    Value::string("a"),
                    Value::string("b")
                ])],
                &mut output,
            ),
            Value::string("ab")
        );
        assert_eq!(
            call(
                "str_replace",
                vec![
                    Value::packed_array(vec![Value::string("a"), Value::string("b")]),
                    Value::packed_array(vec![Value::string("x"), Value::string("y")]),
                    Value::string("abca"),
                ],
                &mut output,
            ),
            Value::string("xycx")
        );
        assert_eq!(
            call(
                "str_replace",
                vec![
                    Value::packed_array(vec![Value::string("-.txt"), Value::string(".txt")]),
                    Value::string("-1.txt"),
                    Value::string("phrust-admin-core-upload.txt"),
                ],
                &mut output,
            ),
            Value::string("phrust-admin-core-upload-1.txt")
        );
        assert_eq!(
            call(
                "str_replace",
                vec![
                    Value::packed_array(vec![Value::string("a"), Value::string("b")]),
                    Value::packed_array(vec![Value::string("x")]),
                    Value::string("abca"),
                ],
                &mut output,
            ),
            Value::string("xcx")
        );
        assert_eq!(
            call(
                "strtr",
                vec![
                    Value::string("abc"),
                    Value::string("ab"),
                    Value::string("xy")
                ],
                &mut output
            ),
            Value::string("xyc")
        );
        assert_eq!(
            call(
                "strtr",
                vec![
                    Value::string("012atm"),
                    Value::string("101234567000"),
                    Value::string("atm012"),
                ],
                &mut output
            ),
            Value::string("tm0atm")
        );
        assert_eq!(
            call_error(
                "strtr",
                vec![Value::string("012atm"), Value::Int(1)],
                &mut output
            ),
            "strtr(): Argument #2 ($from) must be of type array, int given"
        );
        assert_eq!(
            call_error(
                "strtr",
                vec![
                    Value::string("012atm"),
                    Value::Array(PhpArray::new()),
                    Value::string("atm012"),
                ],
                &mut output
            ),
            "strtr(): Argument #2 ($from) must be of type string, array given"
        );
        assert_eq!(
            call(
                "strtr",
                vec![
                    Value::string("012atm"),
                    Value::Null,
                    Value::string("atm012"),
                ],
                &mut output
            ),
            Value::string("012atm")
        );
        assert!(
            output
                .to_string_lossy()
                .contains("Deprecated: strtr(): Passing null to parameter #2 ($from)")
        );
        assert_eq!(
            call("trim", vec![Value::string(" x ")], &mut output),
            Value::string("x")
        );
        assert_eq!(
            call("ltrim", vec![Value::string(" x ")], &mut output),
            Value::string("x ")
        );
        assert_eq!(
            call("rtrim", vec![Value::string(" x ")], &mut output),
            Value::string(" x")
        );
        assert_eq!(
            call("strtolower", vec![Value::string("AbC")], &mut output),
            Value::string("abc")
        );
        assert_eq!(
            call("strtoupper", vec![Value::string("AbC")], &mut output),
            Value::string("ABC")
        );
        assert_eq!(
            call("strtoupper", vec![Value::Bool(true)], &mut output),
            Value::string("1")
        );
        assert_eq!(
            call("strtoupper", vec![Value::Bool(false)], &mut output),
            Value::string("")
        );
        assert_eq!(
            call("ucfirst", vec![Value::string("abc")], &mut output),
            Value::string("Abc")
        );
        assert_eq!(
            call("lcfirst", vec![Value::string("Abc")], &mut output),
            Value::string("abc")
        );
        assert_eq!(
            call("ucwords", vec![Value::string("a b")], &mut output),
            Value::string("A B")
        );
        assert_eq!(
            call(
                "str_repeat",
                vec![Value::string("ab"), Value::Int(3)],
                &mut output
            ),
            Value::string("ababab")
        );
        assert_eq!(
            call(
                "str_pad",
                vec![
                    Value::string("x"),
                    Value::Int(3),
                    Value::string("0"),
                    Value::Int(0)
                ],
                &mut output,
            ),
            Value::string("00x")
        );
        assert_eq!(
            call("strrev", vec![Value::string("abc")], &mut output),
            Value::string("cba")
        );
        assert_eq!(
            call(
                "strnatcasecmp",
                vec![Value::string("pIc 6"), Value::string("pic   7")],
                &mut output
            ),
            Value::Int(-1)
        );
        assert_eq!(
            call(
                "strnatcasecmp",
                vec![Value::string("1.010"), Value::string("1.001")],
                &mut output
            ),
            Value::Int(1)
        );
        assert_eq!(
            call(
                "strnatcmp",
                vec![Value::string("foo   2"), Value::string("foo 10")],
                &mut output
            ),
            Value::Int(-1)
        );
    }

    #[test]
    fn highlight_string_renders_php_style_markup() {
        let mut output = OutputBuffer::new();
        assert_eq!(
            call(
                "highlight_string",
                vec![Value::string("<br /><?php echo \"foo\"; ?><br />")],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            output.to_string_lossy(),
            "<pre><code style=\"color: #000000\">&lt;br /&gt;<span style=\"color: #0000BB\">&lt;?php </span><span style=\"color: #007700\">echo </span><span style=\"color: #DD0000\">\"foo\"</span><span style=\"color: #007700\">; </span><span style=\"color: #0000BB\">?&gt;</span>&lt;br /&gt;</code></pre>"
        );

        assert_eq!(
            call(
                "highlight_string",
                vec![
                    Value::string("<?php echo \"foo[] $a \\n\"; ?>"),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::string(
                "<pre><code style=\"color: #000000\"><span style=\"color: #0000BB\">&lt;?php </span><span style=\"color: #007700\">echo </span><span style=\"color: #DD0000\">\"foo[] </span><span style=\"color: #0000BB\">$a</span><span style=\"color: #DD0000\"> \\n\"</span><span style=\"color: #007700\">; </span><span style=\"color: #0000BB\">?&gt;</span></code></pre>"
            )
        );
    }

    #[test]
    fn string_split_replace_reports_value_errors() {
        for (name, args) in [
            ("explode", vec![Value::string(""), Value::string("abc")]),
            ("str_repeat", vec![Value::string("x"), Value::Int(-1)]),
            (
                "str_pad",
                vec![Value::string("x"), Value::Int(3), Value::string("")],
            ),
        ] {
            let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
            let mut output = OutputBuffer::new();
            let mut context = BuiltinContext::new(&mut output);
            let error = (entry.function())(&mut context, args, RuntimeSourceSpan::default())
                .expect_err("expected value error");
            assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
        }
    }

    #[test]
    fn encoding_hash_html_and_url_builtins_cover_mvp_paths() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call("bin2hex", vec![Value::string("Hi")], &mut output),
            Value::string("4869")
        );
        assert_eq!(
            call("hex2bin", vec![Value::string("4869")], &mut output),
            Value::string("Hi")
        );
        assert_eq!(
            call("hex2bin", vec![Value::string("f")], &mut output),
            Value::Bool(false)
        );
        assert_eq!(
            call("hex2bin", vec![Value::string("zz")], &mut output),
            Value::Bool(false)
        );
        assert_eq!(
            call("ord", vec![Value::string("A")], &mut output),
            Value::Int(65)
        );
        assert_eq!(
            call("chr", vec![Value::Int(321)], &mut output),
            Value::string("A")
        );
        assert_eq!(
            call("md5", vec![Value::string("abc")], &mut output),
            Value::string("900150983cd24fb0d6963f7d28e17f72")
        );
        assert_eq!(
            call("sha1", vec![Value::string("abc")], &mut output),
            Value::string("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
        assert_eq!(
            call("crc32", vec![Value::string("abc")], &mut output),
            Value::Int(891_568_578)
        );
        assert_eq!(
            call("base64_encode", vec![Value::string("hi")], &mut output),
            Value::string("aGk=")
        );
        assert_eq!(
            call("base64_decode", vec![Value::string("aGk=")], &mut output),
            Value::string("hi")
        );
        assert_eq!(
            call(
                "base64_decode",
                vec![Value::string("a!Gk="), Value::Bool(false)],
                &mut output
            ),
            Value::string("hi")
        );
        assert_eq!(
            call(
                "base64_decode",
                vec![Value::string("a!Gk="), Value::Bool(true)],
                &mut output
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "htmlspecialchars",
                vec![Value::string("<a&\"'>")],
                &mut output
            ),
            Value::string("&lt;a&amp;&quot;&#039;&gt;")
        );
        assert_eq!(
            call(
                "htmlspecialchars",
                vec![
                    Value::string("?a=1&amp;b=2&#038;c=3&#x26;d=4"),
                    Value::Int(3),
                    Value::string("UTF-8"),
                    Value::Bool(false)
                ],
                &mut output
            ),
            Value::string("?a=1&amp;b=2&#038;c=3&#x26;d=4")
        );
        assert_eq!(
            call(
                "htmlspecialchars",
                vec![
                    Value::string("&bogus;"),
                    Value::Int(3),
                    Value::string("UTF-8"),
                    Value::Bool(false)
                ],
                &mut output
            ),
            Value::string("&amp;bogus;")
        );
        assert_eq!(
            call(
                "htmlspecialchars",
                vec![
                    Value::string("\"'"),
                    Value::Int(0),
                    Value::string("UTF-8"),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::string("\"'")
        );
        assert_eq!(
            call(
                "htmlspecialchars",
                vec![
                    Value::string("\"'"),
                    Value::Int(2),
                    Value::string("UTF-8"),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::string("&quot;'")
        );
        assert_eq!(
            call(
                "htmlspecialchars_decode",
                vec![Value::string("&lt;a&amp;&quot;&#039;&gt;")],
                &mut output
            ),
            Value::string("<a&\"'>")
        );
        assert_eq!(
            call(
                "html_entity_decode",
                vec![
                    Value::string("&lt;a&amp;&quot;&#039;&gt;"),
                    Value::Int(3),
                    Value::string("UTF-8")
                ],
                &mut output
            ),
            Value::string("<a&\"'>")
        );
        assert_eq!(
            call("htmlentities", vec![Value::string("<a&>")], &mut output),
            Value::string("&lt;a&amp;&gt;")
        );
        assert_eq!(
            call("urlencode", vec![Value::string("a b~")], &mut output),
            Value::string("a+b%7E")
        );
        assert_eq!(
            call("rawurlencode", vec![Value::string("a b~")], &mut output),
            Value::string("a%20b~")
        );
        assert_eq!(
            call("urldecode", vec![Value::string("a+b%7E")], &mut output),
            Value::string("a b~")
        );
        assert_eq!(
            call("rawurldecode", vec![Value::string("a%20b~")], &mut output),
            Value::string("a b~")
        );

        let mut query = PhpArray::new();
        query.insert(
            ArrayKey::String(PhpString::from_test_str("a")),
            Value::string("b"),
        );
        query.insert(
            ArrayKey::String(PhpString::from_test_str("c")),
            Value::Int(1),
        );
        assert_eq!(
            call("http_build_query", vec![Value::Array(query)], &mut output),
            Value::string("a=b&c=1")
        );
        let mut prefixed_query = PhpArray::new();
        prefixed_query.insert(
            ArrayKey::String(PhpString::from_test_str("foo")),
            Value::string("bar"),
        );
        prefixed_query.insert(ArrayKey::Int(0), Value::string("abc"));
        prefixed_query.insert(
            ArrayKey::String(PhpString::from_test_str("true")),
            Value::Bool(true),
        );
        assert_eq!(
            call(
                "http_build_query",
                vec![
                    Value::Array(prefixed_query),
                    Value::string("num"),
                    Value::string(";")
                ],
                &mut output
            ),
            Value::string("foo=bar;num0=abc;true=1")
        );
    }

    #[test]
    fn strip_tags_uses_php_tag_state_machine() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "strip_tags",
                vec![Value::string("NEAT <? cool < blah ?> STUFF")],
                &mut output
            ),
            Value::string("NEAT  STUFF")
        );
        assert_eq!(
            call(
                "strip_tags",
                vec![Value::string("NEAT <!-- cool > blah --> STUFF")],
                &mut output
            ),
            Value::string("NEAT  STUFF")
        );
        assert_eq!(
            call(
                "strip_tags",
                vec![Value::string("hello <img title=\">_<\"> world")],
                &mut output
            ),
            Value::string("hello  world")
        );
        assert_eq!(
            call(
                "strip_tags",
                vec![Value::string("<html> I am html string </html>\0<?php x ?>")],
                &mut output
            ),
            Value::string(" I am html string ")
        );
    }

    #[test]
    fn strip_tags_normalizes_allowed_tags_like_php() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "strip_tags",
                vec![
                    Value::string("<<htmL>>hello<</htmL>>"),
                    Value::string("<<html>>")
                ],
                &mut output
            ),
            Value::string("<htmL>hello</htmL>")
        );

        let mut allowed = PhpArray::new();
        allowed.append(Value::string("html"));
        assert_eq!(
            call(
                "strip_tags",
                vec![
                    Value::string("<html>hello</html><p>world</p>"),
                    Value::Array(allowed)
                ],
                &mut output
            ),
            Value::string("<html>hello</html>world")
        );

        let error = call_error(
            "strip_tags",
            vec![
                Value::string("<html>hello</html>"),
                Value::Resource(ResourceTable::new().register_stream(
                    StreamFlags::new(true, false, false),
                    StreamMetadata::new("php", "stream", "r", "memory"),
                )),
            ],
            &mut output,
        );
        assert_eq!(
            error,
            "strip_tags(): Argument #2 ($allowed_tags) must be of type array|string|null, resource given"
        );
    }

    #[test]
    fn parse_url_covers_standard_strings_module_cases() {
        let mut output = OutputBuffer::new();

        let empty = call("parse_url", vec![Value::string("")], &mut output);
        let Value::Array(empty_parts) = empty else {
            panic!("parse_url should return an array for an empty URL");
        };
        assert_eq!(
            empty_parts.get(&ArrayKey::String(PhpString::from_test_str("path"))),
            Some(&Value::string(""))
        );

        let host_port = call(
            "parse_url",
            vec![Value::string("64.246.30.37:80/")],
            &mut output,
        );
        let Value::Array(host_port_parts) = host_port else {
            panic!("parse_url should return host and port parts");
        };
        assert_eq!(
            host_port_parts.get(&ArrayKey::String(PhpString::from_test_str("host"))),
            Some(&Value::string("64.246.30.37"))
        );
        assert_eq!(
            host_port_parts.get(&ArrayKey::String(PhpString::from_test_str("port"))),
            Some(&Value::Int(80))
        );
        assert_eq!(
            host_port_parts.get(&ArrayKey::String(PhpString::from_test_str("path"))),
            Some(&Value::string("/"))
        );

        let full = Value::string("http://secret:hideout@www.php.net:80/index.php?test=1#frag");
        let full_parts = call("parse_url", vec![full.clone()], &mut output);
        let Value::Array(full_parts) = full_parts else {
            panic!("parse_url should return full URL parts");
        };
        assert_eq!(
            full_parts.get(&ArrayKey::String(PhpString::from_test_str("scheme"))),
            Some(&Value::string("http"))
        );
        assert_eq!(
            full_parts.get(&ArrayKey::String(PhpString::from_test_str("user"))),
            Some(&Value::string("secret"))
        );
        assert_eq!(
            full_parts.get(&ArrayKey::String(PhpString::from_test_str("pass"))),
            Some(&Value::string("hideout"))
        );
        assert_eq!(
            full_parts.get(&ArrayKey::String(PhpString::from_test_str("query"))),
            Some(&Value::string("test=1"))
        );
        assert_eq!(
            call("parse_url", vec![full.clone(), Value::Int(0)], &mut output),
            Value::string("http")
        );
        assert_eq!(
            call("parse_url", vec![full.clone(), Value::Int(2)], &mut output),
            Value::Int(80)
        );
        assert_eq!(
            call(
                "parse_url",
                vec![Value::string("http://1.2.3.4:/abc.asp?a=1&b=2")],
                &mut output,
            ),
            {
                let mut expected = PhpArray::new();
                expected.insert(
                    ArrayKey::String(PhpString::from_test_str("scheme")),
                    Value::string("http"),
                );
                expected.insert(
                    ArrayKey::String(PhpString::from_test_str("host")),
                    Value::string("1.2.3.4"),
                );
                expected.insert(
                    ArrayKey::String(PhpString::from_test_str("path")),
                    Value::string("/abc.asp"),
                );
                expected.insert(
                    ArrayKey::String(PhpString::from_test_str("query")),
                    Value::string("a=1&b=2"),
                );
                Value::Array(expected)
            }
        );
    }

    #[test]
    fn substr_treats_null_length_like_omitted_length() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "substr",
                vec![Value::string("abcdef"), Value::Int(2), Value::Null],
                &mut output,
            ),
            Value::string("cdef")
        );
    }

    #[test]
    fn encoding_builtins_report_value_errors() {
        let entry = BuiltinRegistry::new().get("ord").expect("builtin exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let error = (entry.function())(
            &mut context,
            vec![Value::string("")],
            RuntimeSourceSpan::default(),
        )
        .expect_err("expected value error");
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
    }

    #[test]
    fn pack_unpack_cover_standard_integer_formats_and_cursor_ops() {
        let mut output = OutputBuffer::new();

        let packed = call(
            "pack",
            vec![
                Value::string("ll"),
                Value::Int(0x0102_0304),
                Value::Int(0x0506_0708),
            ],
            &mut output,
        );
        assert_eq!(
            packed,
            Value::string(vec![0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05])
        );

        let mut expected_numeric = PhpArray::new();
        expected_numeric.insert(ArrayKey::Int(1), Value::Int(0x0102_0304));
        expected_numeric.insert(ArrayKey::Int(2), Value::Int(0x0506_0708));
        assert_eq!(
            call(
                "unpack",
                vec![
                    Value::string("l2"),
                    Value::string(vec![
                        b'p', b'a', b'd', 0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05
                    ]),
                    Value::Int(3),
                ],
                &mut output,
            ),
            Value::Array(expected_numeric)
        );

        let mut expected_named = PhpArray::new();
        expected_named.insert(
            ArrayKey::String(PhpString::from_test_str("a")),
            Value::Int(1),
        );
        expected_named.insert(
            ArrayKey::String(PhpString::from_test_str("b")),
            Value::Int(1),
        );
        expected_named.insert(
            ArrayKey::String(PhpString::from_test_str("c")),
            Value::Int(2),
        );
        expected_named.insert(
            ArrayKey::String(PhpString::from_test_str("d")),
            Value::Int(2),
        );
        let packed_unsigned = call(
            "pack",
            vec![Value::string("VV"), Value::Int(1), Value::Int(2)],
            &mut output,
        );
        assert_eq!(
            call(
                "unpack",
                vec![Value::string("V1a/X4/V1b/V1c/X4/V1d"), packed_unsigned],
                &mut output,
            ),
            Value::Array(expected_named)
        );
    }

    #[test]
    fn formatting_builtins_cover_common_printf_surface() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "sprintf",
                vec![
                    Value::string("%04d|%-5s|%.2f|%08x|%X|%o|%c|%%"),
                    Value::Int(7),
                    Value::string("x"),
                    Value::float(1.25),
                    Value::Int(255),
                    Value::Int(255),
                    Value::Int(8),
                    Value::Int(65),
                ],
                &mut output,
            ),
            Value::string("0007|x    |1.25|000000ff|FF|10|A|%")
        );
        assert_eq!(
            call(
                "sprintf",
                vec![
                    Value::string("%'_5s|%+d|% d"),
                    Value::string("x"),
                    Value::Int(7),
                    Value::Int(7)
                ],
                &mut output,
            ),
            Value::string("____x|+7|7")
        );
        assert_eq!(
            call(
                "sprintf",
                vec![Value::string("%-010.2f"), Value::float(2.5)],
                &mut output,
            ),
            Value::string("2.50000000")
        );
        assert_eq!(
            call(
                "sprintf",
                vec![
                    Value::string("%3$s %1$s %2$04d %4$'#5s %5$ls"),
                    Value::string("one"),
                    Value::Int(2),
                    Value::string("three"),
                    Value::string("x"),
                    Value::string("wide"),
                ],
                &mut output,
            ),
            Value::string("three one 0002 ####x wide")
        );
        assert_eq!(
            call(
                "sprintf",
                vec![Value::string("% %%d"), Value::Int(1234), Value::Int(-5678)],
                &mut output,
            ),
            Value::string("%-5678")
        );
        assert_eq!(
            call(
                "sprintf",
                vec![
                    Value::string("%b|%e|%E|%g|%G|%.3g|%.3G"),
                    Value::Int(-5),
                    Value::Int(1000),
                    Value::Int(1000),
                    Value::float(1.25),
                    Value::float(0.0000123),
                    Value::Int(1000),
                    Value::float(1234567.0)
                ],
                &mut output,
            ),
            Value::string(
                "1111111111111111111111111111111111111111111111111111111111111011|1.000000e+3|1.000000E+3|1.25|1.23E-5|1.0e+3|1.23E+6"
            )
        );
        assert_eq!(
            call(
                "sprintf",
                vec![
                    Value::string("%.4d|%04.4u|%10.4o|%-10.4x|%04.4b"),
                    Value::Int(123),
                    Value::Int(123),
                    Value::Int(123),
                    Value::Int(123),
                    Value::Int(123)
                ],
                &mut output,
            ),
            Value::string("123|0123|          |          |0000")
        );

        assert_eq!(
            call(
                "printf",
                vec![Value::string("[%04d]"), Value::Int(7)],
                &mut output
            ),
            Value::Int(6)
        );
        assert_eq!(output.to_string_lossy(), "[0007]");

        let args = Value::packed_array(vec![Value::string("id"), Value::Int(9)]);
        assert_eq!(
            call(
                "vsprintf",
                vec![Value::string("%s:%d"), args.clone()],
                &mut output,
            ),
            Value::string("id:9")
        );
        assert_eq!(
            call("vprintf", vec![Value::string("%s:%d"), args], &mut output),
            Value::Int(4)
        );
        assert_eq!(output.to_string_lossy(), "[0007]id:9");
    }

    #[test]
    fn formatting_builtins_report_missing_args_and_stream_writes() {
        for (name, args, expected_id) in [
            (
                "sprintf",
                vec![Value::string("%s %s"), Value::string("only-one")],
                "E_PHP_RUNTIME_PRINTF_ARGUMENTS",
            ),
            (
                "fprintf",
                vec![Value::Null, Value::string("%s"), Value::string("x")],
                "E_PHP_RUNTIME_BUILTIN_TYPE",
            ),
        ] {
            let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
            let mut output = OutputBuffer::new();
            let mut context = BuiltinContext::new(&mut output);
            let error = (entry.function())(&mut context, args, RuntimeSourceSpan::default())
                .expect_err("expected formatting error");
            assert_eq!(error.diagnostic_id(), expected_id);
        }

        let entry = BuiltinRegistry::new()
            .get("vfprintf")
            .expect("builtin exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let error = (entry.function())(
            &mut context,
            vec![
                Value::string("stream"),
                Value::string("%s"),
                Value::Array(PhpArray::new()),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect_err("expected stream type error");
        assert_eq!(
            error.message(),
            "vfprintf(): Argument #1 ($stream) must be of type resource, string given"
        );
        let error = (entry.function())(
            &mut context,
            vec![
                Value::Resource(ResourceTable::new().register_stream(
                    StreamFlags::new(true, true, true),
                    StreamMetadata::new("php", "stream", "w+", "php://memory"),
                )),
                Value::Array(PhpArray::new()),
                Value::Array(PhpArray::new()),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect_err("expected format type error");
        assert_eq!(
            error.message(),
            "vfprintf(): Argument #2 ($format) must be of type string, array given"
        );
        let error = (entry.function())(
            &mut context,
            vec![
                Value::Resource(ResourceTable::new().register_stream(
                    StreamFlags::new(true, true, true),
                    StreamMetadata::new("php", "stream", "w+", "php://memory"),
                )),
                Value::string("%s"),
                Value::Null,
            ],
            RuntimeSourceSpan::default(),
        )
        .expect_err("expected values type error");
        assert_eq!(
            error.message(),
            "vfprintf(): Argument #3 ($values) must be of type array, null given"
        );
        assert_eq!(
            call_error(
                "vfprintf",
                vec![
                    Value::Resource(ResourceTable::new().register_stream(
                        StreamFlags::new(true, true, true),
                        StreamMetadata::new("php", "stream", "w+", "php://memory"),
                    )),
                    Value::string("Foo %y fake"),
                    Value::packed_array(vec![Value::string("x")]),
                ],
                &mut output,
            ),
            "Unknown format specifier \"y\""
        );
        assert_eq!(
            call_error(
                "vfprintf",
                vec![
                    Value::Resource(ResourceTable::new().register_stream(
                        StreamFlags::new(true, true, true),
                        StreamMetadata::new("php", "stream", "w+", "php://memory"),
                    )),
                    Value::string("Foo %$c-0202Sd"),
                    Value::packed_array(vec![Value::Int(2)]),
                ],
                &mut output,
            ),
            "Argument number specifier must be greater than zero and less than 2147483647"
        );

        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();
        let stream = resources.register_stream(
            StreamFlags::new(true, true, true),
            StreamMetadata::new("php", "stream", "w+", "php://memory"),
        );
        assert_eq!(
            call(
                "fprintf",
                vec![
                    Value::Resource(stream.clone()),
                    Value::string("%s:%d"),
                    Value::string("id"),
                    Value::Int(7)
                ],
                &mut output
            ),
            Value::Int(4)
        );
        assert_eq!(
            call(
                "vfprintf",
                vec![
                    Value::Resource(stream.clone()),
                    Value::string("|%s:%d|"),
                    Value::packed_array(vec![Value::string("next"), Value::Int(8)])
                ],
                &mut output
            ),
            Value::Int(8)
        );
        stream.rewind().expect("memory stream rewind");
        assert_eq!(
            stream.read_to_end().expect("memory stream read"),
            b"id:7|next:8|"
        );

        let mut stdout_output = OutputBuffer::new();
        let stdout = ResourceTable::new().register_stream(
            StreamFlags::new(true, true, true),
            StreamMetadata::new("php", "stream", "w+", "php://stdout"),
        );
        assert_eq!(
            call(
                "fprintf",
                vec![
                    Value::Resource(stdout.clone()),
                    Value::string("stdout:%d"),
                    Value::Int(3)
                ],
                &mut stdout_output
            ),
            Value::Int(8)
        );
        assert_eq!(
            call(
                "fwrite",
                vec![Value::Resource(stdout), Value::string("|tail")],
                &mut stdout_output
            ),
            Value::Int(5)
        );
        assert_eq!(stdout_output.to_string_lossy(), "stdout:3|tail");
    }

    #[test]
    fn math_numeric_builtins_cover_common_paths() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call("abs", vec![Value::Int(-7)], &mut output),
            Value::Int(7)
        );
        assert_eq!(
            call("abs", vec![Value::string("-2.5")], &mut output),
            Value::float(2.5)
        );
        assert_eq!(
            call(
                "min",
                vec![Value::packed_array(vec![
                    Value::Int(3),
                    Value::Int(1),
                    Value::Int(2)
                ])],
                &mut output
            ),
            Value::Int(1)
        );
        assert_eq!(
            call(
                "max",
                vec![Value::Int(3), Value::Int(1), Value::Int(2)],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "round",
                vec![Value::float(12.345), Value::Int(2)],
                &mut output
            ),
            Value::float(12.35)
        );
        assert_eq!(
            call("floor", vec![Value::float(3.9)], &mut output),
            Value::float(3.0)
        );
        assert_eq!(
            call("ceil", vec![Value::float(3.1)], &mut output),
            Value::float(4.0)
        );
        assert_eq!(
            call("deg2rad", vec![Value::Int(23)], &mut output),
            Value::float((23.0 / 180.0) * std::f64::consts::PI)
        );
        assert_eq!(
            call(
                "rad2deg",
                vec![Value::float(9_223_372_034_707_292_160.0)],
                &mut output
            ),
            Value::float((9_223_372_034_707_292_160.0 / std::f64::consts::PI) * 180.0)
        );
        assert_eq!(
            call("sqrt", vec![Value::Int(9)], &mut output),
            Value::float(3.0)
        );
        assert_eq!(
            call("pow", vec![Value::Int(2), Value::Int(3)], &mut output),
            Value::Int(8)
        );
        assert!(matches!(
            call(
                "pow",
                vec![Value::Int(i64::MIN), Value::Int(i64::MAX)],
                &mut output
            ),
            Value::Float(value) if value.to_f64().is_infinite() && value.to_f64().is_sign_negative()
        ));
        assert_eq!(
            call("intdiv", vec![Value::Int(7), Value::Int(2)], &mut output),
            Value::Int(3)
        );
        assert_eq!(
            call("fmod", vec![Value::Int(7), Value::Int(2)], &mut output),
            Value::float(1.0)
        );
        assert_eq!(
            call("fdiv", vec![Value::Int(7), Value::Int(2)], &mut output),
            Value::float(3.5)
        );
        assert!(matches!(
            call("fdiv", vec![Value::Int(1), Value::Int(0)], &mut output),
            Value::Float(value) if value.to_f64().is_infinite()
        ));
        assert!(matches!(
            call("fdiv", vec![Value::Int(0), Value::Int(0)], &mut output),
            Value::Float(value) if value.to_f64().is_nan()
        ));
        assert_eq!(
            call("is_finite", vec![Value::float(1.5)], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "is_infinite",
                vec![Value::float(f64::INFINITY)],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call("is_nan", vec![Value::float(f64::NAN)], &mut output),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "number_format",
                vec![Value::float(1234.567), Value::Int(2)],
                &mut output
            ),
            Value::string("1,234.57")
        );
        assert_eq!(
            call(
                "number_format",
                vec![
                    Value::float(1234.5),
                    Value::Int(1),
                    Value::string(","),
                    Value::string(".")
                ],
                &mut output
            ),
            Value::string("1.234,5")
        );
        assert_eq!(
            call(
                "number_format",
                vec![Value::Int(i64::MAX), Value::Int(5)],
                &mut output
            ),
            Value::string("9,223,372,036,854,775,807.00000")
        );
        assert_eq!(
            call(
                "number_format",
                vec![Value::Int(i64::MAX), Value::Int(0)],
                &mut output
            ),
            Value::string("9,223,372,036,854,775,807")
        );
        assert_eq!(
            call(
                "number_format",
                vec![Value::Int(i64::MAX), Value::Int(-5)],
                &mut output
            ),
            Value::string("9,223,372,036,854,800,000")
        );
        assert_eq!(
            call(
                "number_format",
                vec![Value::float(9_223_372_036_854_775_808.0), Value::Int(-1)],
                &mut output
            ),
            Value::string("9,223,372,036,854,775,808")
        );
    }

    #[test]
    fn math_numeric_builtins_report_value_errors() {
        let entry = BuiltinRegistry::new()
            .get("intdiv")
            .expect("builtin exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let error = (entry.function())(
            &mut context,
            vec![Value::Int(1), Value::Int(0)],
            RuntimeSourceSpan::default(),
        )
        .expect_err("expected value error");
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");

        let entry = BuiltinRegistry::new().get("fmod").expect("builtin exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        assert!(matches!(
            (entry.function())(
                &mut context,
                vec![Value::Int(1), Value::Int(0)],
                RuntimeSourceSpan::default()
            ),
            Ok(Value::Float(value)) if value.to_f64().is_nan()
        ));
    }

    #[test]
    fn array_basic_builtins_cover_keys_values_and_list_checks() {
        let mut output = OutputBuffer::new();
        let mut mixed = PhpArray::new();
        mixed.insert(ArrayKey::Int(1), Value::string("one"));
        mixed.insert(
            ArrayKey::String(PhpString::from_test_str("01")),
            Value::string("zero-one"),
        );
        mixed.insert(
            ArrayKey::String(PhpString::from_test_str("name")),
            Value::string("n"),
        );
        let before = mixed.clone();

        assert_eq!(
            call("count", vec![Value::Array(mixed.clone())], &mut output),
            Value::Int(3)
        );
        assert_eq!(
            call("sizeof", vec![Value::packed_array(vec![])], &mut output),
            Value::Int(0)
        );
        assert_eq!(
            call(
                "array_key_exists",
                vec![Value::string("1"), Value::Array(mixed.clone())],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "key_exists",
                vec![Value::string("name"), Value::Array(mixed.clone())],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call("array_keys", vec![Value::Array(mixed.clone())], &mut output),
            Value::packed_array(vec![
                Value::Int(1),
                Value::string("01"),
                Value::string("name")
            ])
        );
        assert_eq!(
            call(
                "array_values",
                vec![Value::Array(mixed.clone())],
                &mut output
            ),
            Value::packed_array(vec![
                Value::string("one"),
                Value::string("zero-one"),
                Value::string("n")
            ])
        );
        assert_eq!(
            call(
                "array_sum",
                vec![Value::packed_array(vec![
                    Value::Int(2),
                    Value::string("3"),
                    Value::Bool(true)
                ])],
                &mut output
            ),
            Value::Int(6)
        );
        assert_eq!(
            call(
                "array_sum",
                vec![Value::packed_array(vec![Value::Int(2), Value::float(0.5)])],
                &mut output
            ),
            Value::float(2.5)
        );
        assert_eq!(
            call(
                "array_is_list",
                vec![Value::packed_array(vec![Value::Int(1)])],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "array_is_list",
                vec![Value::Array(mixed.clone())],
                &mut output
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "array_key_first",
                vec![Value::Array(mixed.clone())],
                &mut output
            ),
            Value::Int(1)
        );
        assert_eq!(
            call(
                "array_key_last",
                vec![Value::Array(mixed.clone())],
                &mut output
            ),
            Value::string("name")
        );
        assert_eq!(
            call(
                "array_combine",
                vec![
                    Value::packed_array(vec![Value::string("x"), Value::Int(2)]),
                    Value::packed_array(vec![Value::string("ex"), Value::string("two")])
                ],
                &mut output
            ),
            {
                let mut combined = PhpArray::new();
                combined.insert(
                    ArrayKey::String(PhpString::from_test_str("x")),
                    Value::string("ex"),
                );
                combined.insert(ArrayKey::Int(2), Value::string("two"));
                Value::Array(combined)
            }
        );
        assert_eq!(mixed, before);
    }

    #[test]
    fn array_basic_builtins_cover_strict_search_and_columns() {
        let mut output = OutputBuffer::new();
        let haystack = Value::packed_array(vec![Value::Int(0), Value::string("7"), Value::Int(7)]);

        assert_eq!(
            call(
                "in_array",
                vec![Value::Int(7), haystack.clone(), Value::Bool(false)],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "in_array",
                vec![Value::string("7"), haystack.clone(), Value::Bool(true)],
                &mut output
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "array_search",
                vec![Value::string("7"), haystack.clone(), Value::Bool(true)],
                &mut output
            ),
            Value::Int(1)
        );
        assert_eq!(
            call(
                "array_search",
                vec![Value::string("missing"), haystack, Value::Bool(false)],
                &mut output
            ),
            Value::Bool(false)
        );

        let mut first = PhpArray::new();
        first.insert(
            ArrayKey::String(PhpString::from_test_str("id")),
            Value::Int(2),
        );
        first.insert(
            ArrayKey::String(PhpString::from_test_str("name")),
            Value::string("Ada"),
        );
        let mut second = PhpArray::new();
        second.insert(
            ArrayKey::String(PhpString::from_test_str("id")),
            Value::Int(3),
        );
        second.insert(
            ArrayKey::String(PhpString::from_test_str("name")),
            Value::string("Grace"),
        );
        let rows = Value::packed_array(vec![Value::Array(first), Value::Array(second)]);

        let mut expected = PhpArray::new();
        expected.insert(ArrayKey::Int(2), Value::string("Ada"));
        expected.insert(ArrayKey::Int(3), Value::string("Grace"));
        assert_eq!(
            call(
                "array_column",
                vec![rows, Value::string("name"), Value::string("id")],
                &mut output
            ),
            Value::Array(expected)
        );
    }

    #[test]
    fn array_unique_preserves_keys_and_honors_comparison_flags() {
        let mut output = OutputBuffer::new();
        let mut input = PhpArray::new();
        input.insert(ArrayKey::Int(10), Value::string("01"));
        input.insert(
            ArrayKey::String(PhpString::from_test_str("one")),
            Value::Int(1),
        );
        input.insert(ArrayKey::Int(11), Value::string("1"));
        input.insert(
            ArrayKey::String(PhpString::from_test_str("upper")),
            Value::string("A"),
        );
        input.insert(
            ArrayKey::String(PhpString::from_test_str("lower")),
            Value::string("a"),
        );

        let mut expected_string = PhpArray::new();
        expected_string.insert(ArrayKey::Int(10), Value::string("01"));
        expected_string.insert(
            ArrayKey::String(PhpString::from_test_str("one")),
            Value::Int(1),
        );
        expected_string.insert(
            ArrayKey::String(PhpString::from_test_str("upper")),
            Value::string("A"),
        );
        expected_string.insert(
            ArrayKey::String(PhpString::from_test_str("lower")),
            Value::string("a"),
        );
        assert_eq!(
            call(
                "array_unique",
                vec![Value::Array(input.clone())],
                &mut output
            ),
            Value::Array(expected_string)
        );

        let mut numeric_input = PhpArray::new();
        numeric_input.insert(ArrayKey::Int(10), Value::string("01"));
        numeric_input.insert(
            ArrayKey::String(PhpString::from_test_str("one")),
            Value::Int(1),
        );
        numeric_input.insert(ArrayKey::Int(11), Value::string("1"));
        let mut expected_numeric = PhpArray::new();
        expected_numeric.insert(ArrayKey::Int(10), Value::string("01"));
        assert_eq!(
            call(
                "array_unique",
                vec![
                    Value::Array(numeric_input.clone()),
                    Value::Int(SORT_NUMERIC)
                ],
                &mut output
            ),
            Value::Array(expected_numeric.clone())
        );
        assert_eq!(
            call(
                "array_unique",
                vec![Value::Array(numeric_input), Value::Int(SORT_REGULAR)],
                &mut output
            ),
            Value::Array(expected_numeric)
        );

        let mut expected_case = PhpArray::new();
        expected_case.insert(ArrayKey::Int(10), Value::string("01"));
        expected_case.insert(
            ArrayKey::String(PhpString::from_test_str("one")),
            Value::Int(1),
        );
        expected_case.insert(
            ArrayKey::String(PhpString::from_test_str("upper")),
            Value::string("A"),
        );
        assert_eq!(
            call(
                "array_unique",
                vec![
                    Value::Array(input),
                    Value::Int(SORT_STRING | SORT_FLAG_CASE)
                ],
                &mut output
            ),
            Value::Array(expected_case)
        );
    }

    #[test]
    fn array_intersect_builtins_cover_value_assoc_and_empty_callback_cases() {
        let mut output = OutputBuffer::new();
        let mut first = PhpArray::new();
        first.insert(ArrayKey::Int(0), Value::Int(0));
        first.insert(ArrayKey::Int(1), Value::Int(1));
        first.insert(
            ArrayKey::String(PhpString::from_test_str("two")),
            Value::string("2"),
        );
        let second = Value::packed_array(vec![Value::string("1"), Value::Int(2)]);

        let mut expected = PhpArray::new();
        expected.insert(ArrayKey::Int(1), Value::Int(1));
        expected.insert(
            ArrayKey::String(PhpString::from_test_str("two")),
            Value::string("2"),
        );
        assert_eq!(
            call(
                "array_intersect",
                vec![Value::Array(first.clone()), second],
                &mut output
            ),
            Value::Array(expected)
        );

        let mut assoc_second = PhpArray::new();
        assoc_second.insert(ArrayKey::Int(1), Value::string("1"));
        assoc_second.insert(
            ArrayKey::String(PhpString::from_test_str("two")),
            Value::Int(2),
        );
        let mut expected_assoc = PhpArray::new();
        expected_assoc.insert(ArrayKey::Int(1), Value::Int(1));
        expected_assoc.insert(
            ArrayKey::String(PhpString::from_test_str("two")),
            Value::string("2"),
        );
        assert_eq!(
            call(
                "array_intersect_assoc",
                vec![Value::Array(first.clone()), Value::Array(assoc_second)],
                &mut output
            ),
            Value::Array(expected_assoc)
        );

        let mut key_second = PhpArray::new();
        key_second.insert(ArrayKey::Int(1), Value::string("different"));
        key_second.insert(
            ArrayKey::String(PhpString::from_test_str("two")),
            Value::Bool(false),
        );
        let mut expected_key = PhpArray::new();
        expected_key.insert(ArrayKey::Int(1), Value::Int(1));
        expected_key.insert(
            ArrayKey::String(PhpString::from_test_str("two")),
            Value::string("2"),
        );
        assert_eq!(
            call(
                "array_intersect_key",
                vec![Value::Array(first.clone()), Value::Array(key_second)],
                &mut output
            ),
            Value::Array(expected_key)
        );

        let diff_second = Value::packed_array(vec![Value::string("1")]);
        let mut expected_diff = PhpArray::new();
        expected_diff.insert(ArrayKey::Int(0), Value::Int(0));
        expected_diff.insert(
            ArrayKey::String(PhpString::from_test_str("two")),
            Value::string("2"),
        );
        assert_eq!(
            call(
                "array_diff",
                vec![Value::Array(first.clone()), diff_second],
                &mut output
            ),
            Value::Array(expected_diff)
        );

        let mut assoc_diff_second = PhpArray::new();
        assoc_diff_second.insert(ArrayKey::Int(1), Value::string("1"));
        let mut expected_diff_assoc = PhpArray::new();
        expected_diff_assoc.insert(ArrayKey::Int(0), Value::Int(0));
        expected_diff_assoc.insert(
            ArrayKey::String(PhpString::from_test_str("two")),
            Value::string("2"),
        );
        assert_eq!(
            call(
                "array_diff_assoc",
                vec![Value::Array(first.clone()), Value::Array(assoc_diff_second)],
                &mut output
            ),
            Value::Array(expected_diff_assoc)
        );

        let mut key_diff_second = PhpArray::new();
        key_diff_second.insert(ArrayKey::Int(0), Value::string("different"));
        let mut expected_diff_key = PhpArray::new();
        expected_diff_key.insert(ArrayKey::Int(1), Value::Int(1));
        expected_diff_key.insert(
            ArrayKey::String(PhpString::from_test_str("two")),
            Value::string("2"),
        );
        assert_eq!(
            call(
                "array_diff_key",
                vec![Value::Array(first.clone()), Value::Array(key_diff_second)],
                &mut output
            ),
            Value::Array(expected_diff_key)
        );

        let empty = Value::packed_array(Vec::new());
        for name in [
            "array_intersect_ukey",
            "array_uintersect",
            "array_intersect_uassoc",
        ] {
            assert_eq!(
                call(
                    name,
                    vec![Value::Array(first.clone()), empty.clone(), Value::Null],
                    &mut output
                ),
                Value::packed_array(Vec::new())
            );
        }
        assert_eq!(
            call(
                "array_uintersect_uassoc",
                vec![Value::Array(first), empty, Value::Null, Value::Null],
                &mut output
            ),
            Value::packed_array(Vec::new())
        );
    }

    #[test]
    fn shuffle_mutates_array_by_reference_and_reindexes_values() {
        let mut output = OutputBuffer::new();
        let cell = ReferenceCell::new(Value::Array({
            let mut array = PhpArray::new();
            array.insert(ArrayKey::Int(5), Value::string("a"));
            array.insert(
                ArrayKey::String(PhpString::from_test_str("name")),
                Value::string("b"),
            );
            array.insert(ArrayKey::Int(9), Value::string("c"));
            array
        }));
        assert_eq!(
            call("shuffle", vec![Value::Reference(cell.clone())], &mut output),
            Value::Bool(true)
        );
        let Value::Array(array) = cell.get() else {
            panic!("shuffle should leave an array in the reference cell");
        };
        assert!(array.is_packed_fast());
        assert_eq!(array.len(), 3);
        let mut values = array
            .iter()
            .map(|(_, value)| match value {
                Value::String(value) => value.to_string_lossy(),
                other => panic!("unexpected shuffled value: {other:?}"),
            })
            .collect::<Vec<_>>();
        values.sort();
        assert_eq!(values, ["a", "b", "c"]);
    }

    #[test]
    fn array_pointer_builtins_track_current_key_and_mutating_moves() {
        let mut output = OutputBuffer::new();
        let cell = ReferenceCell::new(Value::Array({
            let mut array = PhpArray::new();
            array.append(Value::string("zero"));
            array.append(Value::string("one"));
            array.insert(ArrayKey::Int(200), Value::string("two"));
            array
        }));

        assert_eq!(
            call("current", vec![cell.get()], &mut output),
            Value::string("zero")
        );
        assert_eq!(call("key", vec![cell.get()], &mut output), Value::Int(0));
        assert_eq!(
            call("next", vec![Value::Reference(cell.clone())], &mut output),
            Value::string("one")
        );
        assert_eq!(call("key", vec![cell.get()], &mut output), Value::Int(1));
        assert_eq!(
            call("end", vec![Value::Reference(cell.clone())], &mut output),
            Value::string("two")
        );
        assert_eq!(call("key", vec![cell.get()], &mut output), Value::Int(200));
        assert_eq!(
            call("prev", vec![Value::Reference(cell.clone())], &mut output),
            Value::string("one")
        );
        assert_eq!(
            call("reset", vec![Value::Reference(cell.clone())], &mut output),
            Value::string("zero")
        );

        let empty = ReferenceCell::new(Value::packed_array(Vec::new()));
        assert_eq!(
            call("current", vec![empty.get()], &mut output),
            Value::Bool(false)
        );
        assert_eq!(call("key", vec![empty.get()], &mut output), Value::Null);
    }

    #[test]
    fn array_range_builtin_covers_numeric_and_string_sequences() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call("range", vec![Value::Int(1), Value::Int(5)], &mut output),
            Value::packed_array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
                Value::Int(5)
            ])
        );
        assert_eq!(
            call(
                "range",
                vec![Value::Int(5), Value::Int(1), Value::Int(2)],
                &mut output
            ),
            Value::packed_array(vec![Value::Int(5), Value::Int(3), Value::Int(1)])
        );
        assert_eq!(
            call(
                "range",
                vec![Value::Int(1), Value::Int(2), Value::float(0.5)],
                &mut output
            ),
            Value::packed_array(vec![
                Value::float(1.0),
                Value::float(1.5),
                Value::float(2.0)
            ])
        );
        assert_eq!(
            call(
                "range",
                vec![Value::float(4.5), Value::float(4.2), Value::float(0.1)],
                &mut output
            ),
            Value::packed_array(vec![
                Value::float(4.5),
                Value::float(4.4),
                Value::float(4.3),
                Value::float(4.2)
            ])
        );
        assert_eq!(
            call(
                "range",
                vec![Value::float(9.9), Value::string("0")],
                &mut output
            ),
            Value::packed_array(vec![
                Value::float(9.9),
                Value::float(8.9),
                Value::float(7.9),
                Value::float(6.9),
                Value::float(5.9),
                Value::float(4.9),
                Value::float(3.9000000000000004),
                Value::float(2.9000000000000004),
                Value::float(1.9000000000000004),
                Value::float(0.9000000000000004),
            ])
        );
        assert_eq!(
            call(
                "range",
                vec![Value::string("a"), Value::string("e"), Value::Int(2)],
                &mut output
            ),
            Value::packed_array(vec![
                Value::string("a"),
                Value::string("c"),
                Value::string("e")
            ])
        );
        assert_eq!(
            call(
                "range",
                vec![Value::string("1"), Value::string("3")],
                &mut output
            ),
            Value::packed_array(vec![
                Value::string("1"),
                Value::string("2"),
                Value::string("3")
            ])
        );
        assert_eq!(
            call(
                "range",
                vec![Value::string("1"), Value::string("10"), Value::string("3")],
                &mut output
            ),
            Value::packed_array(vec![
                Value::Int(1),
                Value::Int(4),
                Value::Int(7),
                Value::Int(10)
            ])
        );
    }

    #[test]
    fn array_range_builtin_reports_step_value_errors() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call_error(
                "range",
                vec![Value::Int(1), Value::Int(7), Value::Int(0)],
                &mut output
            ),
            "range(): Argument #3 ($step) cannot be 0"
        );
        assert_eq!(
            call_error(
                "range",
                vec![
                    Value::float(1.0),
                    Value::float(7.0),
                    Value::float(f64::INFINITY)
                ],
                &mut output
            ),
            "range(): Argument #3 ($step) must be a finite number, INF provided"
        );
        assert_eq!(
            call_error(
                "range",
                vec![Value::Int(1), Value::Int(7), Value::float(7.5)],
                &mut output
            ),
            "range(): Argument #3 ($step) must be less than the range spanned by argument #1 ($start) and argument #2 ($end)"
        );
        assert_eq!(
            call_error(
                "range",
                vec![Value::Int(1), Value::Int(3), Value::Int(-1)],
                &mut output
            ),
            "range(): Argument #3 ($step) must be greater than 0 for increasing ranges"
        );
        assert_eq!(
            call_error(
                "range",
                vec![Value::string("a"), Value::string("c"), Value::Int(-1)],
                &mut output
            ),
            "range(): Argument #3 ($step) must be greater than 0 for increasing ranges"
        );
    }

    #[test]
    fn array_range_builtin_warns_for_invalid_string_inputs() {
        let mut output = OutputBuffer::new();
        assert_eq!(
            call(
                "range",
                vec![Value::string("AA"), Value::string("BB")],
                &mut output
            ),
            Value::packed_array(vec![Value::string("A"), Value::string("B")])
        );
        let warnings = output.to_string_lossy();
        assert!(warnings.contains(
            "range(): Argument #1 ($start) must be a single byte, subsequent bytes are ignored"
        ));
        assert!(warnings.contains(
            "range(): Argument #2 ($end) must be a single byte, subsequent bytes are ignored"
        ));

        let mut output = OutputBuffer::new();
        assert_eq!(
            call(
                "range",
                vec![Value::string("Z"), Value::string("")],
                &mut output
            ),
            Value::packed_array(vec![Value::Int(0)])
        );
        let warnings = output.to_string_lossy();
        assert!(warnings.contains("range(): Argument #2 ($end) must not be empty, casted to 0"));
        assert!(warnings.contains(
            "range(): Argument #2 ($end) must be a single byte string if argument #1 ($start) is a single byte string, argument #1 ($start) converted to 0"
        ));

        let mut output = OutputBuffer::new();
        assert_eq!(
            call(
                "range",
                vec![Value::string("A"), Value::string("H"), Value::float(2.6)],
                &mut output
            ),
            Value::packed_array(vec![Value::float(0.0)])
        );
        assert!(output.to_string_lossy().contains(
            "range(): Argument #3 ($step) must be of type int when generating an array of characters, inputs converted to 0"
        ));

        let mut output = OutputBuffer::new();
        assert_eq!(
            call(
                "range",
                vec![Value::string("1"), Value::string("2"), Value::float(0.1)],
                &mut output
            ),
            Value::packed_array(vec![
                Value::float(1.0),
                Value::float(1.1),
                Value::float(1.2),
                Value::float(1.3),
                Value::float(1.4),
                Value::float(1.5),
                Value::float(1.6),
                Value::float(1.7000000000000002),
                Value::float(1.8),
                Value::float(1.9),
                Value::float(2.0)
            ])
        );
        assert!(output.is_empty());
    }

    #[test]
    fn array_range_builtin_deprecates_null_boundaries() {
        let mut output = OutputBuffer::new();
        assert_eq!(
            call("range", vec![Value::Null, Value::Null], &mut output),
            Value::packed_array(vec![Value::Int(0)])
        );
        let warnings = output.to_string_lossy();
        assert!(warnings.contains(
            "range(): Passing null to parameter #1 ($start) of type string|int|float is deprecated"
        ));
        assert!(warnings.contains(
            "range(): Passing null to parameter #2 ($end) of type string|int|float is deprecated"
        ));

        let mut output = OutputBuffer::new();
        assert_eq!(
            call("range", vec![Value::Null, Value::string("e")], &mut output),
            Value::packed_array(vec![Value::Int(0)])
        );
        let warnings = output.to_string_lossy();
        assert!(warnings.contains(
            "range(): Passing null to parameter #1 ($start) of type string|int|float is deprecated"
        ));
        assert!(warnings.contains(
            "range(): Argument #1 ($start) must be a single byte string if argument #2 ($end) is a single byte string, argument #2 ($end) converted to 0"
        ));
    }

    #[test]
    fn array_range_builtin_reports_oversized_ranges_without_panicking() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call_error(
                "range",
                vec![Value::float(1.0), Value::float(f64::INFINITY)],
                &mut output
            ),
            "range(): Argument #2 ($end) must be a finite number, INF provided"
        );
        let error = call_error(
            "range",
            vec![Value::Int(i64::MIN), Value::Int(i64::MAX), Value::Int(1)],
            &mut output,
        );
        assert!(error.contains("The supplied range exceeds the maximum array size by "));
        assert!(error.contains("start=-9223372036854775808, end=9223372036854775807, step=1"));
        assert!(error.contains("Maximum size: 1000000."));
        assert_eq!(
            call_error(
                "range",
                vec![Value::Int(1), Value::Int(3), Value::Int(i64::MIN)],
                &mut output
            ),
            "range(): Argument #3 ($step) must be greater than 0 for increasing ranges"
        );
    }

    #[test]
    fn array_stack_builtins_mutate_only_references() {
        let mut output = OutputBuffer::new();
        let cell = ReferenceCell::new(Value::packed_array(vec![Value::Int(1), Value::Int(2)]));

        assert_eq!(
            call(
                "array_push",
                vec![Value::Reference(cell.clone()), Value::Int(3)],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            cell.get(),
            Value::packed_array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
        assert_eq!(
            call(
                "array_pop",
                vec![Value::Reference(cell.clone())],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "array_unshift",
                vec![Value::Reference(cell.clone()), Value::Int(0)],
                &mut output
            ),
            Value::Int(3)
        );
        assert_eq!(
            call(
                "array_shift",
                vec![Value::Reference(cell.clone())],
                &mut output
            ),
            Value::Int(0)
        );
        assert_eq!(
            cell.get(),
            Value::packed_array(vec![Value::Int(1), Value::Int(2)])
        );
    }

    #[test]
    fn array_slice_merge_and_transform_builtins_work() {
        let mut output = OutputBuffer::new();
        let mut keyed = PhpArray::new();
        keyed.insert(ArrayKey::Int(2), Value::string("two"));
        keyed.insert(
            ArrayKey::String(PhpString::from_test_str("a")),
            Value::Int(1),
        );
        keyed.insert(ArrayKey::Int(4), Value::string("four"));

        let mut expected_slice = PhpArray::new();
        expected_slice.insert(
            ArrayKey::String(PhpString::from_test_str("a")),
            Value::Int(1),
        );
        expected_slice.append(Value::string("four"));
        assert_eq!(
            call(
                "array_slice",
                vec![Value::Array(keyed.clone()), Value::Int(1), Value::Int(2)],
                &mut output
            ),
            Value::Array(expected_slice)
        );
        let mut expected_reverse = PhpArray::new();
        expected_reverse.append(Value::string("four"));
        expected_reverse.insert(
            ArrayKey::String(PhpString::from_test_str("a")),
            Value::Int(1),
        );
        expected_reverse.append(Value::string("two"));
        assert_eq!(
            call(
                "array_reverse",
                vec![Value::Array(keyed.clone()), Value::Bool(false)],
                &mut output
            ),
            Value::Array(expected_reverse)
        );
        assert_eq!(
            call(
                "array_pad",
                vec![
                    Value::packed_array(vec![Value::Int(1)]),
                    Value::Int(3),
                    Value::Int(0)
                ],
                &mut output
            ),
            Value::packed_array(vec![Value::Int(1), Value::Int(0), Value::Int(0)])
        );
        let mut expected_fill = PhpArray::new();
        expected_fill.insert(ArrayKey::Int(-2), Value::string("x"));
        expected_fill.insert(ArrayKey::Int(-1), Value::string("x"));
        expected_fill.insert(ArrayKey::Int(0), Value::string("x"));
        assert_eq!(
            call(
                "array_fill",
                vec![Value::Int(-2), Value::Int(3), Value::string("x")],
                &mut output
            ),
            Value::Array(expected_fill)
        );
        let mut expected_fill_keys = PhpArray::new();
        expected_fill_keys.insert(
            ArrayKey::String(PhpString::from_test_str("name")),
            Value::Bool(true),
        );
        expected_fill_keys.insert(ArrayKey::Int(2), Value::Bool(true));
        expected_fill_keys.insert(
            ArrayKey::String(PhpString::from_test_str("1.5")),
            Value::Bool(true),
        );
        expected_fill_keys.insert(
            ArrayKey::String(PhpString::from_test_str("")),
            Value::Bool(true),
        );
        assert_eq!(
            call(
                "array_fill_keys",
                vec![
                    Value::packed_array(vec![
                        Value::string("name"),
                        Value::string("2"),
                        Value::float(1.5),
                        Value::Bool(false),
                        Value::Null,
                    ]),
                    Value::Bool(true),
                ],
                &mut output
            ),
            Value::Array(expected_fill_keys)
        );
        assert_eq!(
            call_error(
                "array_fill",
                vec![Value::Int(0), Value::Int(-1), Value::Null],
                &mut output
            ),
            "array_fill(): Argument #2 ($count) must be greater than or equal to 0"
        );

        let mut left = PhpArray::new();
        left.insert(ArrayKey::Int(0), Value::string("x"));
        left.insert(
            ArrayKey::String(PhpString::from_test_str("k")),
            Value::Int(1),
        );
        let mut right = PhpArray::new();
        right.insert(ArrayKey::Int(7), Value::string("y"));
        right.insert(
            ArrayKey::String(PhpString::from_test_str("k")),
            Value::Int(2),
        );
        let mut expected_merge = PhpArray::new();
        expected_merge.append(Value::string("x"));
        expected_merge.insert(
            ArrayKey::String(PhpString::from_test_str("k")),
            Value::Int(2),
        );
        expected_merge.append(Value::string("y"));
        assert_eq!(
            call(
                "array_merge",
                vec![Value::Array(left.clone()), Value::Array(right.clone())],
                &mut output
            ),
            Value::Array(expected_merge)
        );

        let mut expected_replace = keyed.clone();
        expected_replace.insert(ArrayKey::Int(7), Value::string("y"));
        expected_replace.insert(
            ArrayKey::String(PhpString::from_test_str("k")),
            Value::Int(2),
        );
        assert_eq!(
            call(
                "array_replace",
                vec![Value::Array(keyed), Value::Array(right)],
                &mut output
            ),
            Value::Array(expected_replace)
        );

        let mut rand_input = PhpArray::new();
        rand_input.insert(ArrayKey::Int(2), Value::string("two"));
        rand_input.insert(
            ArrayKey::String(PhpString::from_test_str("name")),
            Value::string("n"),
        );
        let rand_key = call(
            "array_rand",
            vec![Value::Array(rand_input.clone())],
            &mut output,
        );
        assert!(
            matches!(rand_key, Value::Int(2))
                || matches!(rand_key, Value::String(ref key) if key.as_bytes() == b"name")
        );
        let rand_keys = call(
            "array_rand",
            vec![Value::Array(rand_input), Value::Int(2)],
            &mut output,
        );
        let Value::Array(rand_keys) = rand_keys else {
            panic!("array_rand with num > 1 should return a packed array");
        };
        assert_eq!(rand_keys.len(), 2);
        let mut returned = rand_keys
            .iter()
            .map(|(_, value)| match value {
                Value::Int(value) => format!("int:{value}"),
                Value::String(value) => format!("str:{}", value.to_string_lossy()),
                other => panic!("unexpected array_rand key value: {other:?}"),
            })
            .collect::<Vec<_>>();
        returned.sort();
        assert_eq!(returned, ["int:2", "str:name"]);
        assert_eq!(
            call_error("array_rand", vec![Value::packed_array(vec![])], &mut output),
            "builtin array_rand: Array is empty"
        );

        let mut nested_left = PhpArray::new();
        nested_left.insert(ArrayKey::Int(0), Value::string("keep"));
        nested_left.insert(
            ArrayKey::String(PhpString::from_test_str("inner")),
            Value::Int(1),
        );
        let mut recursive_left = PhpArray::new();
        recursive_left.insert(
            ArrayKey::String(PhpString::from_test_str("nested")),
            Value::Array(nested_left),
        );
        recursive_left.insert(ArrayKey::Int(2), Value::string("old"));
        let mut nested_right = PhpArray::new();
        nested_right.insert(
            ArrayKey::String(PhpString::from_test_str("inner")),
            Value::Int(2),
        );
        nested_right.insert(
            ArrayKey::String(PhpString::from_test_str("added")),
            Value::Bool(true),
        );
        let mut recursive_right = PhpArray::new();
        recursive_right.insert(
            ArrayKey::String(PhpString::from_test_str("nested")),
            Value::Array(nested_right),
        );
        recursive_right.insert(ArrayKey::Int(2), Value::string("new"));
        let mut expected_nested = PhpArray::new();
        expected_nested.insert(ArrayKey::Int(0), Value::string("keep"));
        expected_nested.insert(
            ArrayKey::String(PhpString::from_test_str("inner")),
            Value::Int(2),
        );
        expected_nested.insert(
            ArrayKey::String(PhpString::from_test_str("added")),
            Value::Bool(true),
        );
        let mut expected_recursive = PhpArray::new();
        expected_recursive.insert(
            ArrayKey::String(PhpString::from_test_str("nested")),
            Value::Array(expected_nested),
        );
        expected_recursive.insert(ArrayKey::Int(2), Value::string("new"));
        assert_eq!(
            call(
                "array_replace_recursive",
                vec![Value::Array(recursive_left), Value::Array(recursive_right)],
                &mut output
            ),
            Value::Array(expected_recursive)
        );
    }

    #[test]
    fn array_splice_chunk_flip_and_recursive_merge_work() {
        let mut output = OutputBuffer::new();
        let cell = ReferenceCell::new(Value::packed_array(vec![
            Value::string("a"),
            Value::string("b"),
            Value::string("c"),
        ]));
        assert_eq!(
            call(
                "array_splice",
                vec![
                    Value::Reference(cell.clone()),
                    Value::Int(1),
                    Value::Int(1),
                    Value::packed_array(vec![Value::string("x")])
                ],
                &mut output
            ),
            Value::packed_array(vec![Value::string("b")])
        );
        assert_eq!(
            cell.get(),
            Value::packed_array(vec![
                Value::string("a"),
                Value::string("x"),
                Value::string("c")
            ])
        );

        assert_eq!(
            call(
                "array_chunk",
                vec![
                    Value::packed_array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
                    Value::Int(2)
                ],
                &mut output
            ),
            Value::packed_array(vec![
                Value::packed_array(vec![Value::Int(1), Value::Int(2)]),
                Value::packed_array(vec![Value::Int(3)])
            ])
        );
        let mut keyed_chunk_input = PhpArray::new();
        keyed_chunk_input.insert(
            ArrayKey::String(PhpString::from_test_str("key1")),
            Value::Int(1),
        );
        keyed_chunk_input.insert(
            ArrayKey::String(PhpString::from_test_str("key2")),
            Value::Int(2),
        );
        keyed_chunk_input.insert(
            ArrayKey::String(PhpString::from_test_str("key3")),
            Value::Int(3),
        );
        assert_eq!(
            call(
                "array_chunk",
                vec![Value::Array(keyed_chunk_input.clone()), Value::Int(2)],
                &mut output
            ),
            Value::packed_array(vec![
                Value::packed_array(vec![Value::Int(1), Value::Int(2)]),
                Value::packed_array(vec![Value::Int(3)])
            ])
        );
        let mut expected_preserved_chunk = PhpArray::new();
        expected_preserved_chunk.insert(
            ArrayKey::String(PhpString::from_test_str("key1")),
            Value::Int(1),
        );
        expected_preserved_chunk.insert(
            ArrayKey::String(PhpString::from_test_str("key2")),
            Value::Int(2),
        );
        let mut expected_preserved_tail = PhpArray::new();
        expected_preserved_tail.insert(
            ArrayKey::String(PhpString::from_test_str("key3")),
            Value::Int(3),
        );
        assert_eq!(
            call(
                "array_chunk",
                vec![
                    Value::Array(keyed_chunk_input),
                    Value::Int(2),
                    Value::Bool(true)
                ],
                &mut output
            ),
            Value::packed_array(vec![
                Value::Array(expected_preserved_chunk),
                Value::Array(expected_preserved_tail)
            ])
        );

        let mut flip_input = PhpArray::new();
        flip_input.insert(
            ArrayKey::String(PhpString::from_test_str("a")),
            Value::Int(1),
        );
        flip_input.insert(
            ArrayKey::String(PhpString::from_test_str("b")),
            Value::string("x"),
        );
        let mut expected_flip = PhpArray::new();
        expected_flip.insert(ArrayKey::Int(1), Value::string("a"));
        expected_flip.insert(
            ArrayKey::String(PhpString::from_test_str("x")),
            Value::string("b"),
        );
        assert_eq!(
            call("array_flip", vec![Value::Array(flip_input)], &mut output),
            Value::Array(expected_flip)
        );
        let mut flip_reference_input = PhpArray::new();
        flip_reference_input.insert(
            ArrayKey::String(PhpString::from_test_str("template")),
            Value::Reference(ReferenceCell::new(Value::string("Page No Title"))),
        );
        let mut expected_reference_flip = PhpArray::new();
        expected_reference_flip.insert(
            ArrayKey::String(PhpString::from_test_str("Page No Title")),
            Value::string("template"),
        );
        assert_eq!(
            call(
                "array_flip",
                vec![Value::Array(flip_reference_input)],
                &mut output
            ),
            Value::Array(expected_reference_flip)
        );
        let mut flip_skip_input = PhpArray::new();
        flip_skip_input.insert(
            ArrayKey::String(PhpString::from_test_str("d")),
            Value::Bool(true),
        );
        flip_skip_input.insert(
            ArrayKey::String(PhpString::from_test_str("E")),
            Value::Bool(false),
        );
        flip_skip_input.insert(ArrayKey::String(PhpString::from_test_str("F")), Value::Null);
        flip_skip_input.insert(ArrayKey::Int(0), Value::string("G"));
        let diagnostics = {
            let mut context = BuiltinContext::new(&mut output);
            assert_eq!(
                call_in_context(
                    &mut context,
                    "array_flip",
                    vec![Value::Array(flip_skip_input)]
                ),
                Value::Array({
                    let mut expected = PhpArray::new();
                    expected.insert(
                        ArrayKey::String(PhpString::from_test_str("G")),
                        Value::Int(0),
                    );
                    expected
                })
            );
            context.take_diagnostics()
        };
        assert_eq!(diagnostics.len(), 3);
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.id() == "E_PHP_RUNTIME_ARRAY_FLIP_ENTRY_SKIPPED")
        );

        let mut first = PhpArray::new();
        first.insert(
            ArrayKey::String(PhpString::from_test_str("k")),
            Value::Int(1),
        );
        let mut second = PhpArray::new();
        second.insert(
            ArrayKey::String(PhpString::from_test_str("k")),
            Value::Int(2),
        );
        let mut expected = PhpArray::new();
        expected.insert(
            ArrayKey::String(PhpString::from_test_str("k")),
            Value::packed_array(vec![Value::Int(1), Value::Int(2)]),
        );
        assert_eq!(
            call(
                "array_merge_recursive",
                vec![Value::Array(first), Value::Array(second)],
                &mut output
            ),
            Value::Array(expected)
        );
    }

    #[test]
    fn serialization_builtins_roundtrip_and_fail_closed() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call("serialize", vec![Value::Int(1)], &mut output),
            Value::string("i:1;")
        );
        assert_eq!(
            call("unserialize", vec![Value::string("i:1;")], &mut output),
            Value::Int(1)
        );
        assert_eq!(
            call(
                "unserialize",
                vec![Value::string("bad payload")],
                &mut output
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn setlocale_reports_supported_c_locale_and_rejects_missing_locales() {
        let mut output = OutputBuffer::new();

        assert_eq!(
            call(
                "setlocale",
                vec![Value::Int(6), Value::string("C")],
                &mut output
            ),
            Value::string("C")
        );
        assert_eq!(
            call(
                "setlocale",
                vec![Value::Int(6), Value::string("invalid")],
                &mut output
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "setlocale",
                vec![Value::Int(0), Value::string("fr_FR")],
                &mut output
            ),
            Value::Bool(false)
        );
    }
}
