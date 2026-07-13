//! Request-local OPcache API facade.
//!
//! `opcache_compile_file()` needs the VM compiler to validate and cache PHP
//! source. The standalone runtime entry fails closed; the VM dispatch hook
//! records successful compiles in this request-local state.

use super::core::{arity_error, conversion_error, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::convert::to_bool;
use crate::{ArrayKey, PhpArray, PhpString, Value};
use std::path::{Path, PathBuf};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "opcache_compile_file",
        builtin_opcache_compile_file,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "opcache_get_configuration",
        builtin_opcache_get_configuration,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "opcache_get_status",
        builtin_opcache_get_status,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "opcache_invalidate",
        builtin_opcache_invalidate,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "opcache_is_script_cached",
        builtin_opcache_is_script_cached,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "opcache_is_script_cached_in_file_cache",
        builtin_opcache_is_script_cached_in_file_cache,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "opcache_jit_blacklist",
        builtin_opcache_jit_blacklist,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "opcache_reset",
        builtin_opcache_reset,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_opcache_compile_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("opcache_compile_file", "one argument"));
    }
    let filename = string_arg("opcache_compile_file", &args[0])?;
    let path = resolve_path(context.cwd(), &filename.to_string_lossy());
    if !path.is_file() {
        context.php_warning(
            "opcache_compile_file.file_not_found",
            format!(
                "opcache_compile_file(): Failed opening '{}' for inclusion",
                filename.to_string_lossy()
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    context.php_warning(
        "opcache_compile_file.compiler_unavailable",
        "opcache_compile_file(): compiler is not available in the runtime-only facade",
        span,
    );
    Ok(Value::Bool(false))
}

fn builtin_opcache_get_configuration(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("opcache_get_configuration", "zero arguments"));
    }

    let mut directives = PhpArray::new();
    directives.insert(string_key("opcache.enable"), Value::Bool(true));
    directives.insert(string_key("opcache.enable_cli"), Value::Bool(true));
    directives.insert(
        string_key("opcache.file_cache"),
        Value::string(context.ini_get("opcache.file_cache").unwrap_or("")),
    );
    directives.insert(string_key("opcache.jit"), Value::string(""));

    let mut version = PhpArray::new();
    version.insert(string_key("version"), Value::string("phrust-facade"));
    version.insert(
        string_key("opcache_product_name"),
        Value::string("phrust OPcache facade"),
    );

    let mut result = PhpArray::new();
    result.insert(string_key("directives"), Value::Array(directives));
    result.insert(string_key("version"), Value::Array(version));
    result.insert(string_key("blacklist"), Value::Array(PhpArray::new()));
    Ok(Value::Array(result))
}

fn builtin_opcache_get_status(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("opcache_get_status", "zero or one argument"));
    }
    let include_scripts = optional_bool("opcache_get_status", args.first())?.unwrap_or(true);

    let mut result = PhpArray::new();
    result.insert(string_key("opcache_enabled"), Value::Bool(true));
    result.insert(string_key("cache_full"), Value::Bool(false));
    result.insert(string_key("restart_pending"), Value::Bool(false));
    result.insert(string_key("restart_in_progress"), Value::Bool(false));
    result.insert(string_key("memory_usage"), Value::Array(memory_usage()));
    result.insert(
        string_key("interned_strings_usage"),
        Value::Array(interned_strings_usage()),
    );
    result.insert(
        string_key("opcache_statistics"),
        Value::Array(opcache_statistics(context)),
    );
    result.insert(string_key("jit"), Value::Array(jit_status()));
    if include_scripts {
        result.insert(string_key("scripts"), Value::Array(script_status(context)));
    }
    Ok(Value::Array(result))
}

fn builtin_opcache_invalidate(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("opcache_invalidate", "one or two arguments"));
    }
    let filename = string_arg("opcache_invalidate", &args[0])?;
    let force = optional_bool("opcache_invalidate", args.get(1))?.unwrap_or(false);
    let path = resolve_path(context.cwd(), &filename.to_string_lossy());
    let key = canonical_key(&path);
    let removed = context.opcache_state().invalidate_script(&key);
    Ok(Value::Bool(removed || force))
}

fn builtin_opcache_is_script_cached(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("opcache_is_script_cached", "one argument"));
    }
    let filename = string_arg("opcache_is_script_cached", &args[0])?;
    let path = resolve_path(context.cwd(), &filename.to_string_lossy());
    Ok(Value::Bool(
        context
            .opcache_state()
            .is_script_cached(&canonical_key(&path)),
    ))
}

fn builtin_opcache_is_script_cached_in_file_cache(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error(
            "opcache_is_script_cached_in_file_cache",
            "one argument",
        ));
    }
    let _ = string_arg("opcache_is_script_cached_in_file_cache", &args[0])?;
    Ok(Value::Bool(false))
}

fn builtin_opcache_jit_blacklist(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("opcache_jit_blacklist", "one argument"));
    }
    Ok(Value::Null)
}

fn builtin_opcache_reset(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("opcache_reset", "zero arguments"));
    }
    context.opcache_state().reset();
    Ok(Value::Bool(true))
}

fn memory_usage() -> PhpArray {
    let mut usage = PhpArray::new();
    usage.insert(string_key("used_memory"), Value::Int(0));
    usage.insert(string_key("free_memory"), Value::Int(0));
    usage.insert(string_key("wasted_memory"), Value::Int(0));
    usage.insert(
        string_key("current_wasted_percentage"),
        Value::Float(0.0.into()),
    );
    usage
}

fn interned_strings_usage() -> PhpArray {
    let mut usage = PhpArray::new();
    usage.insert(string_key("buffer_size"), Value::Int(0));
    usage.insert(string_key("used_memory"), Value::Int(0));
    usage.insert(string_key("free_memory"), Value::Int(0));
    usage.insert(string_key("number_of_strings"), Value::Int(0));
    usage
}

fn opcache_statistics(context: &mut BuiltinContext<'_>) -> PhpArray {
    let state = context.opcache_state();
    let compile_attempts = state.compile_attempts() as i64;
    let cached_scripts = state.compiled_scripts().count() as i64;
    let mut stats = PhpArray::new();
    stats.insert(string_key("num_cached_scripts"), Value::Int(cached_scripts));
    stats.insert(string_key("num_cached_keys"), Value::Int(cached_scripts));
    stats.insert(string_key("max_cached_keys"), Value::Int(cached_scripts));
    stats.insert(string_key("hits"), Value::Int(0));
    stats.insert(string_key("start_time"), Value::Int(0));
    stats.insert(string_key("last_restart_time"), Value::Int(0));
    stats.insert(string_key("oom_restarts"), Value::Int(0));
    stats.insert(string_key("hash_restarts"), Value::Int(0));
    stats.insert(
        string_key("manual_restarts"),
        Value::Int(state.resets() as i64),
    );
    stats.insert(string_key("misses"), Value::Int(compile_attempts));
    stats.insert(string_key("blacklist_misses"), Value::Int(0));
    stats.insert(string_key("blacklist_miss_ratio"), Value::Float(0.0.into()));
    stats.insert(
        string_key("opcache_hit_rate"),
        Value::Float(if compile_attempts == 0 { 0.0 } else { 100.0 }.into()),
    );
    stats.insert(
        string_key("invalidations"),
        Value::Int(state.invalidations() as i64),
    );
    stats
}

fn script_status(context: &mut BuiltinContext<'_>) -> PhpArray {
    let mut scripts = PhpArray::new();
    for path in context.opcache_state().compiled_scripts() {
        let mut entry = PhpArray::new();
        entry.insert(string_key("full_path"), Value::string(path));
        entry.insert(string_key("hits"), Value::Int(0));
        entry.insert(string_key("memory_consumption"), Value::Int(0));
        entry.insert(string_key("last_used"), Value::string(""));
        entry.insert(string_key("last_used_timestamp"), Value::Int(0));
        entry.insert(string_key("timestamp"), Value::Int(0));
        scripts.insert(string_key(path), Value::Array(entry));
    }
    scripts
}

fn jit_status() -> PhpArray {
    let mut jit = PhpArray::new();
    jit.insert(string_key("enabled"), Value::Bool(false));
    jit.insert(string_key("on"), Value::Bool(false));
    jit.insert(string_key("kind"), Value::Int(0));
    jit.insert(string_key("opt_level"), Value::Int(0));
    jit.insert(string_key("opt_flags"), Value::Int(0));
    jit.insert(string_key("buffer_size"), Value::Int(0));
    jit.insert(string_key("buffer_free"), Value::Int(0));
    jit
}

fn resolve_path(cwd: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn canonical_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn optional_bool(
    function: &'static str,
    value: Option<&Value>,
) -> Result<Option<bool>, BuiltinError> {
    value
        .map(|value| to_bool(value).map_err(|message| conversion_error(function, message)))
        .transpose()
}

fn string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_test_str(value))
}
