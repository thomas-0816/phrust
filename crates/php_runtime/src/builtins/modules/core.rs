//! Core builtin implementations and cross-module helpers.

use super::super::context::{
    JSON_ERROR_DEPTH, JSON_ERROR_INF_OR_NAN, JSON_ERROR_NON_BACKED_ENUM, JSON_ERROR_RECURSION,
    JSON_ERROR_SYNTAX, JSON_ERROR_UNSUPPORTED_TYPE, JSON_ERROR_UTF8, JSON_FORCE_OBJECT,
    JSON_HEX_AMP, JSON_HEX_APOS, JSON_HEX_QUOT, JSON_HEX_TAG, JSON_INVALID_UTF8_IGNORE,
    JSON_INVALID_UTF8_SUBSTITUTE, JSON_NUMERIC_CHECK, JSON_PARTIAL_OUTPUT_ON_ERROR,
    JSON_PRESERVE_ZERO_FRACTION, JSON_PRETTY_PRINT, JSON_THROW_ON_ERROR,
    JSON_UNESCAPED_LINE_TERMINATORS, JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE,
    JsonBuiltinServices, PcreCallbackServiceAccess, PcreServiceAccess, json_error_message,
};
use super::super::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use super::debug_output::DebugFormatter;
pub(in crate::builtins::modules) use super::debug_output::php_float_debug_string;
mod encoding;
mod haval;
mod http;
mod password;
mod serialization;
mod snefru;
mod snefru_tables;

use crate::convert::{float_to_php_string, php_float_to_int};
use crate::layout_stats;
use crate::numeric_string::{NumericStringKind, NumericStringValue, classify, classify_php_string};
use crate::{
    ArrayKey, ClassEntry, ClassFlags, FloatValue, NumericValue, ObjectRef, OutputBuffer, PhpArray,
    PhpString, ResourceKind, StreamWrapperRegistry, UnserializeOptions, Value, compare, equal,
    identical, normalize_class_name, pcre, serialize_with_precision, to_bool, to_float, to_int,
    to_number, to_string, unserialize as unserialize_value,
};
pub(in crate::builtins::modules) use encoding::{
    HTML_ESCAPE_DEFAULT_FLAGS, HashOptions, PHP_QUERY_RFC3986, build_query_pairs,
    direct_hash_hmac_into, direct_hash_hmac_output_length, direct_hash_into,
    direct_hash_output_length, direct_html_entity_decode_into,
    direct_html_entity_decode_output_length, direct_html_escape_into,
    direct_html_escape_output_length, format_array_values, hash_digest_bytes,
    hash_digest_bytes_with_options, hex_encode, hex_nibble, hmac_digest_bytes,
    hmac_hash_algorithm_value_error, html_entity_decode_with_flags, html_escape_with_options,
    html_translation_table, htmlentities_escape_with_options, htmlspecialchars_decode_with_flags,
    parse_hash_options,
};
pub use http::{NativeCookieOptions, build_native_cookie_header_value};
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
    BuiltinEntry::new("sleep", builtin_sleep, BuiltinCompatibility::Php),
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
        "error_clear_last",
        builtin_error_handling_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "error_get_last",
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
    BuiltinEntry::new("getmygid", builtin_getmygid, BuiltinCompatibility::Php),
    BuiltinEntry::new("getmyuid", builtin_getmyuid, BuiltinCompatibility::Php),
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
    BuiltinEntry::new("gc_disable", builtin_gc_disable, BuiltinCompatibility::Php),
    BuiltinEntry::new("gc_enable", builtin_gc_enable, BuiltinCompatibility::Php),
    BuiltinEntry::new("gc_enabled", builtin_gc_enabled, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gc_mem_caches",
        builtin_gc_mem_caches,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gc_status", builtin_gc_status, BuiltinCompatibility::Php),
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
    BuiltinEntry::new("phpinfo", builtin_phpinfo, BuiltinCompatibility::Php),
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
        "settype",
        builtin_settype_requires_vm,
        BuiltinCompatibility::Php,
    ),
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

fn builtin_getmygid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("getmygid", &args, 0)?;
    Ok(Value::Int(current_process_gid()))
}

fn builtin_getmyuid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("getmyuid", &args, 0)?;
    Ok(Value::Int(current_process_uid()))
}

#[allow(unsafe_code)] // direct libc call, result checked
#[cfg(unix)]
pub(in crate::builtins::modules) fn current_process_uid() -> i64 {
    unsafe { libc::getuid() as i64 }
}

#[cfg(not(unix))]
pub(in crate::builtins::modules) fn current_process_uid() -> i64 {
    0
}

#[allow(unsafe_code)] // direct libc call, result checked
#[cfg(unix)]
pub(in crate::builtins::modules) fn current_process_gid() -> i64 {
    unsafe { libc::getgid() as i64 }
}

#[cfg(not(unix))]
pub(in crate::builtins::modules) fn current_process_gid() -> i64 {
    0
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

/// Fills a caller-owned native byte slice from the platform CSPRNG without
/// constructing a runtime `Value`.
#[must_use]
pub fn native_random_fill(bytes: &mut [u8]) -> bool {
    getrandom::fill(bytes).is_ok()
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
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("gc_enabled", &args, 0)?;
    Ok(Value::Bool(context.gc_enabled()))
}

pub(in crate::builtins::modules) fn builtin_gc_disable(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("gc_disable", &args, 0)?;
    context.set_gc_enabled(false);
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_gc_enable(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("gc_enable", &args, 0)?;
    context.set_gc_enabled(true);
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_gc_mem_caches(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("gc_mem_caches", &args, 0)?;
    Ok(Value::Int(0))
}

pub(in crate::builtins::modules) fn builtin_gc_status(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("gc_status", &args, 0)?;

    let mut status = PhpArray::new();
    status.insert(string_key("running"), Value::Bool(false));
    status.insert(string_key("protected"), Value::Bool(false));
    status.insert(string_key("full"), Value::Bool(false));
    status.insert(string_key("runs"), Value::Int(0));
    status.insert(string_key("collected"), Value::Int(0));
    status.insert(string_key("threshold"), Value::Int(10001));
    status.insert(string_key("buffer_size"), Value::Int(16384));
    status.insert(string_key("roots"), Value::Int(0));
    status.insert(
        string_key("application_time"),
        Value::Float(FloatValue::from_f64(0.0)),
    );
    status.insert(
        string_key("collector_time"),
        Value::Float(FloatValue::from_f64(0.0)),
    );
    status.insert(
        string_key("destructor_time"),
        Value::Float(FloatValue::from_f64(0.0)),
    );
    status.insert(
        string_key("free_time"),
        Value::Float(FloatValue::from_f64(0.0)),
    );
    Ok(Value::Array(status))
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

pub(in crate::builtins::modules) fn builtin_sleep(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("sleep", &args, 1)?;
    let seconds = int_arg("sleep", &args[0])?;
    if seconds < 0 {
        return Err(value_error(
            "sleep",
            "Argument #1 ($seconds) must be greater than or equal to 0",
        ));
    }
    std::thread::sleep(std::time::Duration::from_secs(seconds as u64));
    Ok(Value::Int(0))
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

pub(in crate::builtins::modules) fn builtin_phpinfo(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("phpinfo", "zero or one argument"));
    }
    if let Some(flags) = args.first() {
        let _ = int_arg("phpinfo", flags)?;
    }
    let jit = if pcre::is_jit_available() {
        "enabled"
    } else {
        "disabled"
    };
    context.output().write_bytes(b"pcre\n");
    context
        .output()
        .write_bytes(format!("PCRE JIT Support => {jit}\n").as_bytes());
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_token_get_all(
    context: &mut BuiltinContext<'_>,
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
    let result = crate::tokenizer::tokenize_with_diagnostics(&source, flags)?;
    for diagnostic in result.diagnostics {
        context.record_diagnostic(diagnostic);
    }
    Ok(crate::tokenizer::baseline_token_get_all_value(
        result.tokens,
    ))
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

pub(in crate::builtins::modules) fn builtin_settype_requires_vm(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("settype", &args, 2)?;
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_CALLABLE_CONTEXT_REQUIRED",
        "settype requires VM cast dispatch",
    ))
}

pub(in crate::builtins::modules) fn builtin_get_resource_id(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("get_resource_id", &args, 1)?;
    let value = deref_value(args.first().expect("checked arity"));
    let Value::Resource(resource) = value else {
        return Err(argument_type_error(
            "get_resource_id",
            "#1 ($resource)",
            "resource",
            &value,
        ));
    };
    Ok(Value::Int(resource.id().get() as i64))
}

pub(in crate::builtins::modules) fn builtin_get_resource_type(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("get_resource_type", &args, 1)?;
    let value = deref_value(args.first().expect("checked arity"));
    let Value::Resource(resource) = value else {
        return Err(argument_type_error(
            "get_resource_type",
            "#1 ($resource)",
            "resource",
            &value,
        ));
    };
    Ok(Value::string(resource.resource_type().into_bytes()))
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
    Ok(Value::Int(native_parse_intval_string_base(
        text.as_bytes(),
        base,
    )))
}

/// Parses an authoritative native byte string using PHP's explicit `intval`
/// base rules without constructing a runtime `Value`.
#[doc(hidden)]
pub fn native_parse_intval_string_base(bytes: &[u8], base: i64) -> i64 {
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
    pub(in crate::builtins::modules) count_all: bool,
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
        let (count, count_all) = if index < format.len() && format[index] == b'*' {
            index += 1;
            (None, true)
        } else {
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
            (count, false)
        };
        let count = if count_all { None } else { count };

        let label = if allow_labels {
            let label_start = index;
            while index < format.len() && format[index] != b'/' {
                index += 1;
            }
            (label_start != index).then(|| format[label_start..index].to_vec())
        } else {
            None
        };

        specs.push(PackFormatSpec {
            code,
            count,
            count_all,
            label,
        });
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
        Value::Object(object) if normalize_class_name(&object.class_name()) == "phptoken" => {
            match object.get_property("text") {
                Some(Value::String(text)) => Ok(text),
                _ => to_string(value),
            }
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
) -> Result<i64, BuiltinError> {
    match deref_value(value) {
        Value::Float(value) => Ok(php_float_to_int(value.to_f64())),
        value => int_arg(name, &value),
    }
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
                format!(
                    "parse_url(): Argument #2 ($component) must be a valid URL component identifier, {other} given"
                ),
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
            if port_start < e {
                let mut port_end = port_start;
                while port_end < e && bytes[port_end].is_ascii_digit() {
                    port_end += 1;
                }
                if port_end == port_start || port_end - port_start > 5 {
                    return None;
                }
                parsed.port = Some(parse_url_port(&bytes[port_start..port_end])?);
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
    active_arrays: Vec<u64>,
    active_objects: Vec<u64>,
    active_references: Vec<u64>,
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
                JSON_ERROR_INF_OR_NAN | JSON_ERROR_NON_BACKED_ENUM => {
                    JsonValue::Number(JsonNumber::from(0))
                }
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
            if object.is_enum() {
                let json = match object.enum_backing_type() {
                    Some(_) => {
                        if let Some(value) = object.get_property("value") {
                            php_value_to_json_inner(&value, flags, state)
                        } else {
                            state.error_json(JSON_ERROR_NON_BACKED_ENUM)
                        }
                    }
                    None => state.error_json(JSON_ERROR_NON_BACKED_ENUM),
                };
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

pub(in crate::builtins::modules) fn compile_preg_pattern<S: PcreServiceAccess>(
    context: &mut S,
    function_name: &str,
    pattern: PhpString,
    span: RuntimeSourceSpan,
) -> Option<std::sync::Arc<pcre::CompiledPattern>> {
    let limits = pcre_match_limits_from_ini(context);
    match context.pcre_cache().compile_with_limits(&pattern, limits) {
        Ok(compiled) => Some(compiled),
        Err(error) => {
            if error.code() == pcre::PREG_INTERNAL_ERROR {
                context.php_warning(
                    "E_PHP_RUNTIME_PCRE_WARNING",
                    format!("{function_name}(): {}", error.message()),
                    span,
                );
            }
            context.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
            None
        }
    }
}

fn pcre_match_limits_from_ini<S: PcreServiceAccess>(context: &S) -> pcre::PcreMatchLimits {
    pcre::PcreMatchLimits {
        backtrack_limit: pcre_ini_u32(context, "pcre.backtrack_limit"),
        recursion_limit: pcre_ini_u32(context, "pcre.recursion_limit"),
        jit: context
            .ini_get("pcre.jit")
            .is_none_or(|value| !matches!(value.trim(), "" | "0" | "Off" | "off" | "false")),
    }
}

fn pcre_ini_u32<S: PcreServiceAccess>(context: &S, name: &str) -> Option<u32> {
    context
        .ini_get(name)
        .and_then(|value| value.trim().parse::<u32>().ok())
}

pub(in crate::builtins::modules) fn preg_failure<S: PcreServiceAccess>(
    context: &mut S,
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
    capture_names: &[Option<String>],
) -> Value {
    let capture_count = capture_names.len();
    let mut grouped: Vec<PhpArray> = std::iter::repeat_with(PhpArray::new)
        .take(capture_count)
        .collect();
    let mut mark_group = PhpArray::new();
    for (match_index, match_value) in matches.into_iter().enumerate() {
        let Value::Array(captures) = match_value else {
            continue;
        };
        for (key, value) in captures.iter() {
            match key {
                ArrayKey::Int(index) => {
                    let index = index as usize;
                    while grouped.len() <= index {
                        grouped.push(PhpArray::new());
                    }
                    grouped[index].append(value.clone());
                }
                ArrayKey::String(name) if name.as_bytes() == b"MARK" => {
                    mark_group.insert(ArrayKey::Int(match_index as i64), value.clone());
                }
                ArrayKey::String(_) => {}
            }
        }
    }
    let mut output = PhpArray::new();
    for (index, group) in grouped.into_iter().enumerate() {
        let value = Value::Array(group);
        if let Some(Some(name)) = capture_names.get(index) {
            output.insert(
                ArrayKey::String(name.as_bytes().to_vec().into()),
                value.clone(),
            );
        }
        output.append(value);
    }
    if !mark_group.is_empty() {
        output.insert(
            ArrayKey::String(PhpString::from("MARK")),
            Value::Array(mark_group),
        );
    }
    Value::Array(output)
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
    pcre::validate_utf8_subject_for_pattern(compiled, subject)?;
    let mut output = Vec::new();
    let mut last_end = 0usize;
    let mut local_count = 0i64;
    compiled.for_each_php_match(
        subject,
        0,
        |captures| {
            let Some(full) = captures.get(0) else {
                return Ok(true);
            };
            if limit >= 0 && local_count >= limit {
                return Ok(false);
            }
            output.extend_from_slice(&subject[last_end..full.start()]);
            output.extend_from_slice(&expand_preg_replacement(replacement, &captures));
            last_end = full.end();
            local_count += 1;
            *count += 1;
            Ok(true)
        },
        std::convert::identity,
    )?;
    output.extend_from_slice(&subject[last_end..]);
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::builtins::modules) fn preg_replace_callback_subject<S: PcreCallbackServiceAccess>(
    context: &mut S,
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
                if let Err(error) =
                    pcre::validate_utf8_subject_for_pattern(compiled, text.as_bytes())
                {
                    context
                        .set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
                    return Ok(Value::Null);
                }
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
            if let Err(error) = pcre::validate_utf8_subject_for_pattern(compiled, text.as_bytes()) {
                context.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
                return Ok(Value::Null);
            }
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

#[allow(clippy::too_many_arguments)]
pub(in crate::builtins::modules) fn preg_replace_callback_bytes<S: PcreCallbackServiceAccess>(
    context: &mut S,
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
    compiled.for_each_php_match(
        subject,
        0,
        |captures| {
            let Some(full) = captures.get(0) else {
                return Ok(true);
            };
            if limit >= 0 && local_count >= limit {
                return Ok(false);
            }
            output.extend_from_slice(&subject[last_end..full.start()]);
            let callback_result = context.invoke_builtin(
                callback,
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
            Ok(true)
        },
        |error| BuiltinError::new("E_PHP_RUNTIME_PCRE_ERROR", error.message().to_string()),
    )?;
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
    context: &mut JsonBuiltinServices<'_>,
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
        name: normalize_class_name("stdClass").into(),
        parent: None,
        interfaces: vec![],
        methods: vec![],
        properties: vec![],
        constants: vec![],
        enum_cases: vec![],
        attributes: vec![],
        enum_backing_type: None,
        constructor_id: None,
        flags: ClassFlags {
            has_complete_method_table: true,
            ..ClassFlags::default()
        },
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
    Ok(super::strings::native_string_search_slice(
        haystack.as_bytes(),
        needle.as_bytes(),
        case_insensitive,
        before_needle,
    )
    .map_or(Value::Bool(false), |bytes| Value::string(bytes.to_vec())))
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
    super::strings::baseline_substr_replace(
        subject.as_bytes(),
        replacement.as_bytes(),
        offset,
        length,
    )
    .map(Value::string)
    .ok_or_else(|| value_error(name, "length is out of range"))
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

pub(in crate::builtins::modules) fn materialize_array_builtin_value(value: &Value) -> Value {
    match value {
        Value::Null => Value::Null,
        Value::Bool(value) => Value::Bool(*value),
        Value::Int(value) => Value::Int(*value),
        Value::Float(value) => Value::Float(*value),
        Value::Uninitialized => Value::Uninitialized,
        Value::String(_)
        | Value::Array(_)
        | Value::Object(_)
        | Value::Resource(_)
        | Value::Fiber(_)
        | Value::Generator(_)
        | Value::Callable(_)
        | Value::Reference(_) => {
            let _source = layout_stats::enter_layout_source_family(
                layout_stats::SOURCE_ARRAY_BUILTIN_OUTPUT_MATERIALIZATION,
            );
            value.clone()
        }
    }
}

pub(in crate::builtins::modules) fn materialize_array_builtin_array(array: &PhpArray) -> Value {
    let _source = layout_stats::enter_layout_source_family(
        layout_stats::SOURCE_ARRAY_BUILTIN_OUTPUT_MATERIALIZATION,
    );
    Value::Array(array.clone())
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
            output.insert(key.clone(), materialize_array_builtin_value(value));
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
            output.insert(key.clone(), materialize_array_builtin_value(value));
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

fn count_recursive_inner(
    array: &crate::PhpArray,
    active_storage: &mut Vec<u64>,
    active_references: &mut Vec<u64>,
) -> (usize, usize) {
    let identity = array.native_storage_id();
    if active_storage.contains(&identity) {
        return (0, 1);
    }
    active_storage.push(identity);
    let mut count = array.len();
    let mut recursion_warnings = 0_usize;
    for (_, value) in array.iter() {
        let child = match value {
            Value::Reference(reference) => {
                let identity = reference.gc_debug_id();
                if active_references.contains(&identity) {
                    recursion_warnings = recursion_warnings.saturating_add(1);
                    continue;
                }
                if reference
                    .native_array_storage_id()
                    .is_some_and(|identity| active_storage.contains(&identity))
                {
                    recursion_warnings = recursion_warnings.saturating_add(1);
                    continue;
                }
                active_references.push(identity);
                if let Value::Array(child) = reference.get() {
                    let (nested, warnings) =
                        count_recursive_inner(&child, active_storage, active_references);
                    count = count.saturating_add(nested);
                    recursion_warnings = recursion_warnings.saturating_add(warnings);
                }
                active_references.pop();
                continue;
            }
            Value::Array(child) => Some(child.clone()),
            _ => None,
        };
        if let Some(child) = child {
            let (nested, warnings) =
                count_recursive_inner(&child, active_storage, active_references);
            count = count.saturating_add(nested);
            recursion_warnings = recursion_warnings.saturating_add(warnings);
        }
    }
    let popped = active_storage.pop();
    debug_assert_eq!(popped, Some(identity));
    (count, recursion_warnings)
}

pub(in crate::builtins::modules) fn count_recursive(
    array: &crate::PhpArray,
    root_reference: Option<u64>,
) -> (usize, usize) {
    let mut active_references = root_reference.into_iter().collect::<Vec<_>>();
    count_recursive_inner(array, &mut Vec::new(), &mut active_references)
}

#[doc(hidden)]
pub fn baseline_count_recursive_value(value: &Value) -> Option<(usize, usize)> {
    let root_reference = match value {
        Value::Reference(reference) => Some(reference.gc_debug_id()),
        _ => None,
    };
    let Value::Array(array) = deref_value(value) else {
        return None;
    };
    Some(count_recursive(&array, root_reference))
}

pub(in crate::builtins::modules) fn array_entries(
    array: &crate::PhpArray,
) -> Vec<(ArrayKey, Value)> {
    array
        .iter()
        .map(|(key, value)| (key.clone(), materialize_array_builtin_value(value)))
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
    entries[start..end]
        .iter()
        .map(|(key, value)| (key.clone(), materialize_array_builtin_value(value)))
        .collect()
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
        Value::Array(array) => Ok(array
            .iter()
            .map(|(_, value)| materialize_array_builtin_value(value))
            .collect()),
        value => Ok(vec![string_arg(name, &value).map(Value::String)?]),
    }
}

pub(in crate::builtins::modules) fn merge_recursive_into(
    output: &mut crate::PhpArray,
    input: &crate::PhpArray,
) -> Result<(), BuiltinError> {
    for (key, value) in input.iter() {
        match key {
            ArrayKey::Int(_) => {
                output
                    .try_append(materialize_array_builtin_value(value))
                    .map_err(|error| {
                        BuiltinError::new("E_PHP_RUNTIME_ARRAY_APPEND_OVERFLOW", error.to_string())
                    })?;
            }
            ArrayKey::String(key) => {
                let out_key = ArrayKey::String(key.clone());
                if let Some(existing) = output.get(&out_key).map(materialize_array_builtin_value) {
                    let merged =
                        merge_recursive_values(existing, materialize_array_builtin_value(value))?;
                    output.insert(out_key, merged);
                } else {
                    output.insert(out_key, materialize_array_builtin_value(value));
                }
            }
        }
    }
    Ok(())
}

pub(in crate::builtins::modules) fn merge_recursive_values(
    left: Value,
    right: Value,
) -> Result<Value, BuiltinError> {
    match (deref_value(&left), deref_value(&right)) {
        (Value::Array(mut left), Value::Array(right)) => {
            merge_recursive_into(&mut left, &right)?;
            Ok(Value::Array(left))
        }
        (Value::Array(mut left), right) => {
            left.try_append(right).map_err(|error| {
                BuiltinError::new("E_PHP_RUNTIME_ARRAY_APPEND_OVERFLOW", error.to_string())
            })?;
            Ok(Value::Array(left))
        }
        (left, Value::Array(right)) => {
            let mut merged = crate::PhpArray::from_packed(vec![left]);
            merge_recursive_into(&mut merged, &right)?;
            Ok(Value::Array(merged))
        }
        (left, right) => Ok(Value::packed_array(vec![left, right])),
    }
}

pub(in crate::builtins::modules) fn replace_recursive_into(
    output: &mut crate::PhpArray,
    input: &crate::PhpArray,
) {
    for (key, value) in input.iter() {
        let replacement =
            if let Some(existing) = output.get(&key).map(materialize_array_builtin_value) {
                replace_recursive_values(existing, materialize_array_builtin_value(value))
            } else {
                materialize_array_builtin_value(value)
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

pub(in crate::builtins::modules) fn replace_map<K, V>(
    bytes: &[u8],
    replacements: &[(K, V)],
) -> Vec<u8>
where
    K: AsRef<[u8]>,
    V: AsRef<[u8]>,
{
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if let Some((needle, replacement)) = replacements.iter().find(|(needle, _)| {
            let needle = needle.as_ref();
            !needle.is_empty() && bytes[index..].starts_with(needle)
        }) {
            output.extend_from_slice(replacement.as_ref());
            index += needle.as_ref().len();
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

/// Scalar payload consumed by exact native formatting handlers without
/// constructing a runtime `Value`.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub enum NativePrintfScalar<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(&'a [u8]),
}

pub(in crate::builtins::modules) fn php_format(
    name: &str,
    format: &[u8],
    args: &[Value],
    context: &mut BuiltinContext<'_>,
    span: RuntimeSourceSpan,
) -> Result<Vec<u8>, BuiltinError> {
    php_format_with(name, format, args.len(), |spec, value_index| {
        format_printf_value(name, spec, &args[value_index], context, span.clone())
    })
}

fn php_format_with<E>(
    name: &str,
    format: &[u8],
    argument_count: usize,
    format_value: impl FnMut(&PrintfSpec, usize) -> Result<Vec<u8>, E>,
) -> Result<Vec<u8>, E>
where
    E: From<BuiltinError>,
{
    let mut output = Vec::new();
    php_format_visit_with(name, format, argument_count, format_value, |bytes| {
        output.extend_from_slice(bytes);
        Ok(())
    })?;
    Ok(output)
}

fn php_format_visit_with<E>(
    name: &str,
    format: &[u8],
    argument_count: usize,
    mut format_value: impl FnMut(&PrintfSpec, usize) -> Result<Vec<u8>, E>,
    mut emit: impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<usize, E>
where
    E: From<BuiltinError>,
{
    let mut output_length = 0_usize;
    let mut format_index = 0;
    let mut arg_index = 0;

    while format_index < format.len() {
        if format[format_index] != b'%' {
            let start = format_index;
            while format_index < format.len() && format[format_index] != b'%' {
                format_index += 1;
            }
            let literal = &format[start..format_index];
            emit(literal)?;
            output_length = output_length.checked_add(literal.len()).ok_or_else(|| {
                BuiltinError::new(
                    "E_PHP_RUNTIME_PRINTF_LENGTH",
                    format!("builtin {name} formatted output is too large"),
                )
                .into()
            })?;
            continue;
        }
        format_index += 1;
        if format_index >= format.len() {
            return Err(value_error(name, "incomplete format specifier").into());
        }
        if format[format_index] == b'%' {
            emit(b"%")?;
            output_length = output_length.checked_add(1).ok_or_else(|| {
                BuiltinError::new(
                    "E_PHP_RUNTIME_PRINTF_LENGTH",
                    format!("builtin {name} formatted output is too large"),
                )
                .into()
            })?;
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
        if value_index >= argument_count {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_PRINTF_ARGUMENTS",
                format!("builtin {name} has too few arguments for format string"),
            )
            .into());
        }
        let value = format_value(&spec, value_index)?;
        emit(&value)?;
        output_length = output_length.checked_add(value.len()).ok_or_else(|| {
            BuiltinError::new(
                "E_PHP_RUNTIME_PRINTF_LENGTH",
                format!("builtin {name} formatted output is too large"),
            )
            .into()
        })?;
    }

    Ok(output_length)
}

/// Visits one exact native formatting result without constructing a complete
/// argument vector or output vector.
///
/// The argument accessor is invoked only for the scalar currently consumed by
/// a format specifier. `None` denotes a shape requiring the baseline-native
/// tier. The emitter may count, write into a native arena reservation, or
/// append directly to the request output stack.
#[doc(hidden)]
pub fn visit_native_printf_scalars<'a>(
    name: &str,
    format: &[u8],
    argument_count: usize,
    mut argument: impl FnMut(usize) -> Option<NativePrintfScalar<'a>>,
    emit: impl FnMut(&[u8]) -> Option<()>,
) -> Option<usize> {
    php_format_visit_with(
        name,
        format,
        argument_count,
        |spec, value_index| {
            let value = argument(value_index).ok_or(NativePrintfBaseline)?;
            format_native_printf_value(name, spec, &value)
        },
        {
            let mut emit = emit;
            move |bytes| emit(bytes).ok_or(NativePrintfBaseline)
        },
    )
    .ok()
}

#[derive(Clone, Copy, Debug)]
struct NativePrintfBaseline;

impl From<BuiltinError> for NativePrintfBaseline {
    fn from(_: BuiltinError) -> Self {
        Self
    }
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
        b'c' => vec![printf_int_arg(name, value)?.rem_euclid(256) as u8],
        b'd' => format_signed_decimal(name, spec, printf_int_arg(name, value)?)?.into_bytes(),
        b'u' => (printf_int_arg(name, value)? as u64)
            .to_string()
            .into_bytes(),
        b'x' if spec.precision.is_some() => Vec::new(),
        b'X' if spec.precision.is_some() => Vec::new(),
        b'o' if spec.precision.is_some() => Vec::new(),
        b'b' if spec.precision.is_some() => Vec::new(),
        b'x' => format!("{:x}", printf_int_arg(name, value)? as u64).into_bytes(),
        b'X' => format!("{:X}", printf_int_arg(name, value)? as u64).into_bytes(),
        b'o' => format!("{:o}", printf_int_arg(name, value)? as u64).into_bytes(),
        b'b' => format!("{:b}", printf_int_arg(name, value)? as u64).into_bytes(),
        b'f' | b'F' => format_float_decimal(name, spec, float_arg(name, value)?)?.into_bytes(),
        b'e' | b'E' => format_float_scientific(name, spec, float_arg(name, value)?)?.into_bytes(),
        b'g' | b'G' => format_float_general(name, spec, float_arg(name, value)?)?.into_bytes(),
        b'%' => b"%".to_vec(),
        _ => unreachable!("parse_printf_spec validates specifier"),
    };
    Ok(apply_printf_padding(spec, bytes))
}

fn native_printf_int(value: &NativePrintfScalar<'_>) -> i64 {
    match value {
        NativePrintfScalar::Null | NativePrintfScalar::Bool(false) => 0,
        NativePrintfScalar::Bool(true) => 1,
        NativePrintfScalar::Int(value) => *value,
        NativePrintfScalar::Float(value) => php_float_to_int(*value),
        NativePrintfScalar::String(bytes) => {
            classify(bytes).value.map_or(0, NumericStringValue::to_i64)
        }
    }
}

fn native_printf_float(value: &NativePrintfScalar<'_>) -> f64 {
    match value {
        NativePrintfScalar::Null | NativePrintfScalar::Bool(false) => 0.0,
        NativePrintfScalar::Bool(true) => 1.0,
        NativePrintfScalar::Int(value) => *value as f64,
        NativePrintfScalar::Float(value) => *value,
        NativePrintfScalar::String(bytes) => classify(bytes)
            .value
            .map_or(0.0, NumericStringValue::as_f64),
    }
}

fn native_printf_string(value: &NativePrintfScalar<'_>) -> Vec<u8> {
    match value {
        NativePrintfScalar::Null | NativePrintfScalar::Bool(false) => Vec::new(),
        NativePrintfScalar::Bool(true) => b"1".to_vec(),
        NativePrintfScalar::Int(value) => value.to_string().into_bytes(),
        NativePrintfScalar::Float(value) => float_to_php_string(*value).into_bytes(),
        NativePrintfScalar::String(bytes) => bytes.to_vec(),
    }
}

fn format_native_printf_value(
    name: &str,
    spec: &PrintfSpec,
    value: &NativePrintfScalar<'_>,
) -> Result<Vec<u8>, NativePrintfBaseline> {
    if matches!(spec.specifier, b'f' | b'F' | b'e' | b'E' | b'g' | b'G')
        && let Some(text) = non_finite_float_text(native_printf_float(value))
    {
        return Ok(text.as_bytes().to_vec());
    }
    let bytes = match spec.specifier {
        b's' => {
            let mut bytes = native_printf_string(value);
            if let Some(precision) = spec.precision {
                bytes.truncate(precision);
            }
            bytes
        }
        b'c' => vec![native_printf_int(value).rem_euclid(256) as u8],
        b'd' => format_signed_decimal(name, spec, native_printf_int(value))?.into_bytes(),
        b'u' => (native_printf_int(value) as u64).to_string().into_bytes(),
        b'x' | b'X' | b'o' | b'b' if spec.precision.is_some() => Vec::new(),
        b'x' => format!("{:x}", native_printf_int(value) as u64).into_bytes(),
        b'X' => format!("{:X}", native_printf_int(value) as u64).into_bytes(),
        b'o' => format!("{:o}", native_printf_int(value) as u64).into_bytes(),
        b'b' => format!("{:b}", native_printf_int(value) as u64).into_bytes(),
        b'f' | b'F' => format_float_decimal(name, spec, native_printf_float(value))?.into_bytes(),
        b'e' | b'E' => {
            format_float_scientific(name, spec, native_printf_float(value))?.into_bytes()
        }
        b'g' | b'G' => format_float_general(name, spec, native_printf_float(value))?.into_bytes(),
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

fn string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from(value))
}

#[cfg(test)]
#[path = "core/tests.rs"]
mod tests;
