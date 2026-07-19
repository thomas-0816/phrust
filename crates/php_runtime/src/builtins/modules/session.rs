//! Session builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::convert::{to_bool, to_int};
use crate::{
    ArrayKey, PHP_SESSION_ACTIVE, PHP_SESSION_NONE, PhpDiagnosticLocation, PhpString,
    ReferenceCell, UnserializeOptions, Value, serialize_with_precision as php_serialize_value,
    unserialize as php_unserialize_value, unserialize_prefix,
};
use std::path::{Path, PathBuf};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "session_abort",
        builtin_session_abort,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_cache_expire",
        builtin_session_cache_expire,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_cache_limiter",
        builtin_session_cache_limiter,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_commit",
        builtin_session_write_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_destroy",
        builtin_session_destroy,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("session_gc", builtin_session_gc, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "session_decode",
        builtin_session_decode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_encode",
        builtin_session_encode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_create_id",
        builtin_session_create_id,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_get_cookie_params",
        builtin_session_get_cookie_params,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("session_id", builtin_session_id, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "session_module_name",
        builtin_session_module_name,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_name",
        builtin_session_name,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_regenerate_id",
        builtin_session_regenerate_id,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_register_shutdown",
        builtin_session_register_shutdown,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_reset",
        builtin_session_reset,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_save_path",
        builtin_session_save_path,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_set_cookie_params",
        builtin_session_set_cookie_params,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_set_save_handler",
        builtin_session_set_save_handler,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_start",
        builtin_session_start,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_status",
        builtin_session_status,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_unset",
        builtin_session_unset,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_write_close",
        builtin_session_write_close,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_session_cache_expire(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "session_cache_expire",
            "zero or one argument(s)",
        ));
    }
    let previous = context
        .ini_get("session.cache_expire")
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(180);
    if let Some(value) = args.first()
        && !matches!(deref_value(value), Value::Null)
        && let Some((started_file, started_line)) =
            session_active_start_location(context.session_state())
    {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_CACHE_EXPIRE_ACTIVE",
            format!(
                "session_cache_expire(): Session cache expiration cannot be changed when a session is active (started from {started_file} on line {started_line})"
            ),
            span,
        );
        return Ok(Value::Int(previous));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_cache_expire"));
    };
    if let Some(value) = args.first()
        && !matches!(deref_value(value), Value::Null)
    {
        let expires = int_arg("session_cache_expire", value)?;
        {
            state.replace_cache_expire(expires);
        }
        context.ini_set("session.cache_expire", expires.to_string());
    }
    Ok(Value::Int(previous))
}

pub(in crate::builtins::modules) fn builtin_session_cache_limiter(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "session_cache_limiter",
            "zero or one argument(s)",
        ));
    }
    let previous = context
        .ini_get("session.cache_limiter")
        .unwrap_or("nocache")
        .to_owned();
    if let Some(value) = args.first()
        && !matches!(deref_value(value), Value::Null)
        && let Some((started_file, started_line)) =
            session_active_start_location(context.session_state())
    {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_CACHE_LIMITER_ACTIVE",
            format!(
                "session_cache_limiter(): Session cache limiter cannot be changed when a session is active (started from {started_file} on line {started_line})"
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_cache_limiter"));
    };
    if let Some(value) = args.first()
        && !matches!(deref_value(value), Value::Null)
    {
        let limiter = string_arg("session_cache_limiter", value)?.to_string_lossy();
        {
            state.replace_cache_limiter(limiter.clone());
        }
        context.ini_set("session.cache_limiter", limiter);
    }
    Ok(Value::string(previous))
}

pub(in crate::builtins::modules) fn builtin_session_status(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_status", &args, 0)?;
    let status = context
        .session_state()
        .map_or(PHP_SESSION_NONE, |state| state.status());
    Ok(Value::Int(status))
}

pub(in crate::builtins::modules) fn builtin_session_abort(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_abort", &args, 0)?;
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_abort"));
    };
    let aborted = state.abort();
    if aborted {
        context.sync_session_global_from_state();
    }
    Ok(Value::Bool(aborted))
}

pub(in crate::builtins::modules) fn builtin_session_reset(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_reset", &args, 0)?;
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_reset"));
    };
    let reset = state.reset();
    if reset {
        context.sync_session_global_from_state();
    }
    Ok(Value::Bool(reset))
}

pub(in crate::builtins::modules) fn builtin_session_unset(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_unset", &args, 0)?;
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_unset"));
    };
    let unset = state.unset();
    if unset {
        context.sync_session_global_from_state();
    }
    Ok(Value::Bool(unset))
}

pub(in crate::builtins::modules) fn builtin_session_gc(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_gc", &args, 0)?;
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_gc"));
    };
    if state.status() != PHP_SESSION_ACTIVE {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_GC_INACTIVE",
            "session_gc(): Session cannot be garbage collected when there is no active session",
            span,
        );
        return Ok(Value::Bool(false));
    }
    Ok(Value::Int(0))
}

pub(in crate::builtins::modules) fn builtin_session_register_shutdown(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_register_shutdown", &args, 0)?;
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_session_encode(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_encode", &args, 0)?;
    let (status, session_exists, data) = {
        let Some(state) = context.session_state() else {
            return Err(session_context_error("session_encode"));
        };
        (
            state.status(),
            state.started() && !state.destroyed(),
            state.data(),
        )
    };
    if status != PHP_SESSION_ACTIVE {
        if !session_exists {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_ENCODE_INACTIVE",
                "session_encode(): Cannot encode non-existent session",
                span,
            );
        }
        return Ok(Value::Bool(false));
    }
    let handler = session_serialize_handler(context);
    let serialize_precision = session_serialize_precision(context);
    match handler.as_str() {
        "php" => session_encode_php(context, &data, span, serialize_precision),
        "php_binary" => session_encode_php_binary(context, &data, span, serialize_precision),
        "php_serialize" => php_serialize_value(&Value::Array(data), serialize_precision)
            .map(Value::String)
            .map_err(|error| session_serialization_error("session_encode", error.message())),
        _ => {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_SERIALIZER_UNKNOWN",
                format!(
                    "session_encode(): Cannot find session serialization handler \"{handler}\""
                ),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

fn session_encode_php(
    context: &mut BuiltinContext<'_>,
    data: &crate::PhpArray,
    span: RuntimeSourceSpan,
    serialize_precision: i32,
) -> BuiltinResult {
    let mut output = Vec::new();
    let mut serializer = SessionReferenceSerializer::new(serialize_precision);
    for (key, value) in data.iter() {
        let Some(name) = key.as_string() else {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_ENCODE_NUMERIC_KEY",
                format!(
                    "session_encode(): Skipping numeric key {}",
                    key.as_int().unwrap_or(0)
                ),
                span.clone(),
            );
            continue;
        };
        output.extend_from_slice(name.as_bytes());
        output.push(b'|');
        serializer.write_value(&mut output, value, 0)?;
    }
    if output.is_empty() {
        Ok(Value::Bool(false))
    } else {
        Ok(Value::String(PhpString::from_bytes(output)))
    }
}

fn session_encode_php_binary(
    context: &mut BuiltinContext<'_>,
    data: &crate::PhpArray,
    span: RuntimeSourceSpan,
    serialize_precision: i32,
) -> BuiltinResult {
    let mut output = Vec::new();
    let mut serializer = SessionReferenceSerializer::new(serialize_precision);
    for (key, value) in data.iter() {
        let Some(name) = key.as_string() else {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_ENCODE_NUMERIC_KEY",
                format!(
                    "session_encode(): Skipping numeric key {}",
                    key.as_int().unwrap_or(0)
                ),
                span.clone(),
            );
            continue;
        };
        if name.len() > u8::MAX as usize {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_ENCODE_KEY_TOO_LONG",
                "session_encode(): Skipping string key because it is too long",
                span.clone(),
            );
            continue;
        }
        output.push(name.len() as u8);
        output.extend_from_slice(name.as_bytes());
        serializer.write_value(&mut output, value, 0)?;
    }
    if output.is_empty() {
        Ok(Value::Bool(false))
    } else {
        Ok(Value::String(PhpString::from_bytes(output)))
    }
}

const SESSION_SERIALIZE_MAX_DEPTH: usize = 64;

struct SessionReferenceSerializer {
    references: Vec<(u64, usize)>,
    next_reference_index: usize,
    serialize_precision: i32,
}

impl SessionReferenceSerializer {
    fn new(serialize_precision: i32) -> Self {
        Self {
            references: Vec::new(),
            next_reference_index: 1,
            serialize_precision,
        }
    }

    fn write_value(
        &mut self,
        output: &mut Vec<u8>,
        value: &Value,
        depth: usize,
    ) -> Result<(), BuiltinError> {
        if depth > SESSION_SERIALIZE_MAX_DEPTH {
            return Err(session_serialization_error(
                "session_encode",
                "serialization depth limit exceeded",
            ));
        }
        match value {
            Value::Reference(cell) => self.write_reference(output, cell, depth),
            Value::Array(array) => {
                output.extend_from_slice(format!("a:{}:{{", array.len()).as_bytes());
                for (key, element) in array.iter() {
                    self.write_key(output, &key);
                    self.write_value(output, element, depth + 1)?;
                }
                output.extend_from_slice(b"}");
                Ok(())
            }
            _ => {
                let serialized =
                    php_serialize_value(value, self.serialize_precision).map_err(|error| {
                        session_serialization_error("session_encode", error.message())
                    })?;
                output.extend_from_slice(serialized.as_bytes());
                Ok(())
            }
        }
    }

    fn write_reference(
        &mut self,
        output: &mut Vec<u8>,
        cell: &ReferenceCell,
        depth: usize,
    ) -> Result<(), BuiltinError> {
        let id = cell.gc_debug_id();
        if let Some((_, index)) = self
            .references
            .iter()
            .find(|(reference_id, _)| *reference_id == id)
        {
            output.extend_from_slice(format!("R:{index};").as_bytes());
            return Ok(());
        }

        let index = self.next_reference_index;
        self.next_reference_index += 1;
        self.references.push((id, index));
        let value = cell.get();
        self.write_value(output, &value, depth + 1)
    }

    fn write_key(&mut self, output: &mut Vec<u8>, key: &ArrayKey) {
        match key {
            ArrayKey::Int(value) => output.extend_from_slice(format!("i:{value};").as_bytes()),
            ArrayKey::String(value) => {
                output.extend_from_slice(format!("s:{}:\"", value.len()).as_bytes());
                output.extend_from_slice(value.as_bytes());
                output.extend_from_slice(b"\";");
            }
        }
    }
}

pub(in crate::builtins::modules) fn builtin_session_decode(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_decode", &args, 1)?;
    let Value::String(input) = deref_value(&args[0]) else {
        return Err(type_error("session_decode", "string", &args[0]));
    };
    let active = {
        let Some(state) = context.session_state() else {
            return Err(session_context_error("session_decode"));
        };
        state.status() == PHP_SESSION_ACTIVE
    };
    if !active {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_DECODE_INACTIVE",
            "session_decode(): Session data cannot be decoded when there is no active session",
            span,
        );
        return Ok(Value::Bool(false));
    }
    let decoded = match session_serialize_handler(context).as_str() {
        "php" => session_decode_php(&input),
        "php_binary" => session_decode_php_binary(&input),
        "php_serialize" => session_decode_php_serialize(&input),
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_SESSION_SERIALIZER_UNKNOWN",
            "unknown session serializer",
        )),
    };
    let decoded = match decoded {
        Ok(decoded) => decoded,
        Err(_) => {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_DECODE_FAILED",
                "session_decode(): Failed to decode session object. Session has been destroyed",
                span,
            );
            if let Some(state) = context.session_state() {
                state.destroy();
            }
            context.sync_session_global_from_state();
            return Ok(Value::Bool(false));
        }
    };

    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_decode"));
    };
    let mut data = state.data();
    for (key, value) in decoded.iter() {
        data.insert(key, value.clone());
    }
    state.set_data(data);
    context.sync_session_global_from_state();
    Ok(Value::Bool(true))
}

fn session_decode_php(input: &PhpString) -> Result<crate::PhpArray, BuiltinError> {
    let bytes = input.as_bytes();
    let mut offset = 0usize;
    let mut data = crate::PhpArray::new();
    let mut references = Vec::new();
    while offset < bytes.len() {
        let Some(relative_separator) = bytes[offset..].iter().position(|byte| *byte == b'|') else {
            return Err(session_decode_error());
        };
        let separator = offset + relative_separator;
        let key = ArrayKey::String(PhpString::from_bytes(bytes[offset..separator].to_vec()));
        let value_start = separator + 1;
        let consumed = session_decode_serialized_value(
            &mut data,
            &mut references,
            key,
            &bytes[value_start..],
        )?;
        offset = value_start + consumed;
    }
    Ok(data)
}

fn session_decode_php_binary(input: &PhpString) -> Result<crate::PhpArray, BuiltinError> {
    let bytes = input.as_bytes();
    let mut offset = 0usize;
    let mut data = crate::PhpArray::new();
    let mut references = Vec::new();
    while offset < bytes.len() {
        let key_len = bytes[offset] as usize;
        offset += 1;
        if offset + key_len > bytes.len() {
            return Err(session_decode_error());
        }
        let key = ArrayKey::String(PhpString::from_bytes(
            bytes[offset..offset + key_len].to_vec(),
        ));
        offset += key_len;
        let consumed =
            session_decode_serialized_value(&mut data, &mut references, key, &bytes[offset..])?;
        offset += consumed;
    }
    Ok(data)
}

struct SessionDecodeReference {
    key: ArrayKey,
    cell: ReferenceCell,
}

fn session_decode_serialized_value(
    data: &mut crate::PhpArray,
    references: &mut Vec<SessionDecodeReference>,
    key: ArrayKey,
    serialized: &[u8],
) -> Result<usize, BuiltinError> {
    if let Some((index, consumed)) = parse_session_reference_record(serialized)? {
        let Some(reference) = references.get(index.saturating_sub(1)) else {
            return Err(session_decode_error());
        };
        data.insert(
            reference.key.clone(),
            Value::Reference(reference.cell.clone()),
        );
        data.insert(key, Value::Reference(reference.cell.clone()));
        return Ok(consumed);
    }

    let serialized = PhpString::from_bytes(serialized.to_vec());
    let (value, consumed) = unserialize_prefix(&serialized, UnserializeOptions::default())
        .map_err(|_| session_decode_error())?;
    let cell = ReferenceCell::new(value.clone());
    references.push(SessionDecodeReference {
        key: key.clone(),
        cell,
    });
    data.insert(key, value);
    Ok(consumed)
}

fn parse_session_reference_record(bytes: &[u8]) -> Result<Option<(usize, usize)>, BuiltinError> {
    if !bytes.starts_with(b"R:") {
        return Ok(None);
    }
    let mut offset = 2usize;
    while offset < bytes.len() && bytes[offset].is_ascii_digit() {
        offset += 1;
    }
    if offset == 2 || bytes.get(offset) != Some(&b';') {
        return Err(session_decode_error());
    }
    let index = std::str::from_utf8(&bytes[2..offset])
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(session_decode_error)?;
    Ok(Some((index, offset + 1)))
}

fn session_decode_php_serialize(input: &PhpString) -> Result<crate::PhpArray, BuiltinError> {
    match php_unserialize_value(input, UnserializeOptions::default()) {
        Ok(Value::Array(data)) => Ok(data),
        _ => Err(session_decode_error()),
    }
}

fn session_serialization_error(function: &str, message: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_SERIALIZATION_ERROR",
        format!("builtin {function} failed: {message}"),
    )
}

fn session_decode_error() -> BuiltinError {
    BuiltinError::new("E_PHP_RUNTIME_SESSION_DECODE_FAILED", "decode failed")
}

fn session_serialize_handler(context: &BuiltinContext<'_>) -> String {
    context
        .ini_get("session.serialize_handler")
        .unwrap_or("php")
        .to_owned()
}

fn session_serialize_precision(context: &BuiltinContext<'_>) -> i32 {
    context
        .ini_get("serialize_precision")
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(-1)
}

fn session_save_handler(context: &BuiltinContext<'_>) -> String {
    context
        .ini_get("session.save_handler")
        .unwrap_or("files")
        .to_owned()
}

fn session_save_handler_is_supported(context: &BuiltinContext<'_>) -> bool {
    matches!(session_save_handler(context).as_str(), "files" | "user")
}

fn session_files_save_path_is_unreadable(
    context: &mut BuiltinContext<'_>,
    span: RuntimeSourceSpan,
) -> bool {
    if session_save_handler(context) != "files" {
        return false;
    }
    let raw_path = context
        .ini_get("session.save_path")
        .map(str::to_owned)
        .or_else(|| {
            context
                .session_state()
                .map(|state| state.save_path().to_owned())
        })
        .unwrap_or_default();
    if raw_path.is_empty()
        && let Some(open_basedir) = context.ini_get("open_basedir").map(str::to_owned)
        && !open_basedir.trim().is_empty()
    {
        let display_path = context.cwd().to_string_lossy();
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_FILES_OPEN_BASEDIR",
            format!(
                "session_start(): open_basedir restriction in effect. File({display_path}) is not within the allowed path(s): ({open_basedir})"
            ),
            span.clone(),
        );
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_FILES_STORAGE_FAILED",
            "session_start(): Failed to initialize storage module: files (path: )",
            span,
        );
        return true;
    }
    let Some(path) = session_files_save_path_directory(&raw_path) else {
        return false;
    };
    if let Some(open_basedir) = context.ini_get("open_basedir").map(str::to_owned)
        && !open_basedir.trim().is_empty()
        && !open_basedir_allows_path(&path, &open_basedir, context.cwd())
    {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_FILES_OPEN_BASEDIR",
            format!(
                "session_start(): open_basedir restriction in effect. File({path}) is not within the allowed path(s): ({open_basedir})"
            ),
            span.clone(),
        );
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_FILES_STORAGE_FAILED",
            format!("session_start(): Failed to initialize storage module: files (path: {path})"),
            span,
        );
        return true;
    }
    if Path::new(&path)
        .metadata()
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return false;
    }

    let session_id = context
        .session_state()
        .map(|state| state.id().to_owned())
        .unwrap_or_default();
    let file_name = format!("sess_{session_id}");
    let open_path = if path.ends_with(std::path::MAIN_SEPARATOR) {
        format!("{path}{file_name}")
    } else {
        format!("{path}{}{file_name}", std::path::MAIN_SEPARATOR)
    };
    context.php_warning(
        "E_PHP_RUNTIME_SESSION_FILES_OPEN_FAILED",
        format!("session_start(): open({open_path}, O_RDWR) failed: No such file or directory (2)"),
        span.clone(),
    );
    context.php_warning(
        "E_PHP_RUNTIME_SESSION_FILES_READ_FAILED",
        format!("session_start(): Failed to read session data: files (path: {path})"),
        span,
    );
    true
}

fn session_files_save_path_directory(raw_path: &str) -> Option<String> {
    if raw_path.is_empty() {
        return None;
    }
    let path = raw_path
        .split(';')
        .next_back()
        .unwrap_or(raw_path)
        .trim()
        .to_owned();
    (!path.is_empty()).then_some(path)
}

fn open_basedir_allows_path(path: &str, open_basedir: &str, cwd: &Path) -> bool {
    let candidate = canonicalize_open_basedir_path(path, cwd);
    open_basedir
        .split(open_basedir_separator())
        .filter_map(|entry| {
            let entry = entry.trim();
            (!entry.is_empty()).then(|| canonicalize_open_basedir_path(entry, cwd))
        })
        .any(|allowed| candidate == allowed || candidate.starts_with(&allowed))
}

fn canonicalize_open_basedir_path(path: &str, cwd: &Path) -> PathBuf {
    let path = Path::new(path);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    std::fs::canonicalize(&absolute).unwrap_or_else(|_| normalize_open_basedir_path(&absolute))
}

fn normalize_open_basedir_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn open_basedir_separator() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

fn session_serialize_handler_is_supported(context: &BuiltinContext<'_>) -> bool {
    matches!(
        session_serialize_handler(context).as_str(),
        "php" | "php_binary" | "php_serialize"
    )
}

enum ActiveSessionStart {
    Automatic,
    Location(String, u32),
}

fn session_active_start_context(
    state: Option<&mut crate::SessionState>,
) -> Option<ActiveSessionStart> {
    state.and_then(|state| {
        if state.status() == crate::PHP_SESSION_ACTIVE {
            if state.started_automatically() {
                Some(ActiveSessionStart::Automatic)
            } else {
                let (file, line) = state
                    .started_location()
                    .map(|(file, line)| (file.to_owned(), line))
                    .unwrap_or_else(|| ("<unknown>".to_owned(), 0));
                Some(ActiveSessionStart::Location(file, line))
            }
        } else {
            None
        }
    })
}

fn session_active_start_location(state: Option<&mut crate::SessionState>) -> Option<(String, u32)> {
    state.and_then(|state| {
        if state.status() == crate::PHP_SESSION_ACTIVE {
            Some(
                state
                    .started_location()
                    .map(|(file, line)| (file.to_owned(), line))
                    .unwrap_or_else(|| ("<unknown>".to_owned(), 0)),
            )
        } else {
            None
        }
    })
}

pub(in crate::builtins::modules) fn builtin_session_get_cookie_params(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_get_cookie_params", &args, 0)?;
    let mut params = crate::PhpArray::new();
    params.insert(
        string_array_key("lifetime"),
        Value::Int(session_cookie_lifetime(context)),
    );
    params.insert(
        string_array_key("path"),
        Value::string(session_ini_string(context, "session.cookie_path", "/")),
    );
    params.insert(
        string_array_key("domain"),
        Value::string(session_ini_string(context, "session.cookie_domain", "")),
    );
    params.insert(
        string_array_key("secure"),
        Value::Bool(session_ini_bool(context, "session.cookie_secure")),
    );
    params.insert(
        string_array_key("partitioned"),
        Value::Bool(session_ini_bool(context, "session.cookie_partitioned")),
    );
    params.insert(
        string_array_key("httponly"),
        Value::Bool(session_ini_bool(context, "session.cookie_httponly")),
    );
    params.insert(
        string_array_key("samesite"),
        Value::string(session_ini_string(context, "session.cookie_samesite", "")),
    );
    Ok(Value::Array(params))
}

pub(in crate::builtins::modules) fn builtin_session_set_cookie_params(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 5 {
        return Err(arity_error(
            "session_set_cookie_params",
            "one to five argument(s)",
        ));
    }
    if let Some((started_file, started_line)) = context.session_state().and_then(|state| {
        if state.status() == crate::PHP_SESSION_ACTIVE {
            Some(
                state
                    .started_location()
                    .map(|(file, line)| (file.to_owned(), line))
                    .unwrap_or_else(|| ("<unknown>".to_owned(), 0)),
            )
        } else {
            None
        }
    }) {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_COOKIE_PARAMS_ACTIVE",
            format!(
                "session_set_cookie_params(): Session cookie parameters cannot be changed when a session is active (started from {started_file} on line {started_line})"
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }

    if let Value::Array(options) = deref_value(&args[0]) {
        return session_set_cookie_params_from_options(context, &args, &options, span);
    }

    context.ini_set(
        "session.cookie_lifetime",
        int_arg("session_set_cookie_params", &args[0])?.to_string(),
    );
    if let Some(path) = args.get(1)
        && !matches!(deref_value(path), Value::Null)
    {
        context.ini_set(
            "session.cookie_path",
            string_arg("session_set_cookie_params", path)?.to_string_lossy(),
        );
    }
    if let Some(domain) = args.get(2)
        && !matches!(deref_value(domain), Value::Null)
    {
        context.ini_set(
            "session.cookie_domain",
            string_arg("session_set_cookie_params", domain)?.to_string_lossy(),
        );
    }
    if let Some(secure) = args.get(3)
        && !matches!(deref_value(secure), Value::Null)
    {
        context.ini_set(
            "session.cookie_secure",
            if to_bool(secure)
                .map_err(|message| conversion_error("session_set_cookie_params", message))?
            {
                "1"
            } else {
                "0"
            },
        );
    }
    if let Some(httponly) = args.get(4)
        && !matches!(deref_value(httponly), Value::Null)
    {
        context.ini_set(
            "session.cookie_httponly",
            if to_bool(httponly)
                .map_err(|message| conversion_error("session_set_cookie_params", message))?
            {
                "1"
            } else {
                "0"
            },
        );
    }
    Ok(Value::Bool(true))
}

fn session_set_cookie_params_from_options(
    context: &mut BuiltinContext<'_>,
    args: &[Value],
    options: &crate::PhpArray,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    for (index, number, name) in [
        (1usize, 2usize, "$path"),
        (2, 3, "$domain"),
        (3, 4, "$secure"),
        (4, 5, "$httponly"),
    ] {
        if args
            .get(index)
            .is_some_and(|value| !matches!(deref_value(value), Value::Null))
        {
            return Err(session_cookie_params_value_error(format!(
                "session_set_cookie_params(): Argument #{number} ({name}) must be null when argument #1 ($lifetime_or_options) is an array"
            )));
        }
    }

    let mut valid_options = 0usize;
    let mut updates = Vec::new();
    for (key, value) in options.iter() {
        let Some(name) = key.as_string().map(crate::PhpString::to_string_lossy) else {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_COOKIE_PARAMS_NUMERIC_KEY",
                "session_set_cookie_params(): Argument #1 ($lifetime_or_options) cannot contain numeric keys",
                span.clone(),
            );
            continue;
        };
        match name.to_ascii_lowercase().as_str() {
            "lifetime" => {
                valid_options += 1;
                let lifetime = string_arg("session_set_cookie_params", value)?.to_string_lossy();
                if session_cookie_lifetime_is_negative(&lifetime) {
                    context.php_warning(
                        "E_PHP_RUNTIME_SESSION_COOKIE_LIFETIME_NEGATIVE",
                        "session_set_cookie_params(): CookieLifetime cannot be negative",
                        span,
                    );
                    return Ok(Value::Bool(false));
                }
                updates.push(("session.cookie_lifetime", lifetime));
            }
            "path" => {
                valid_options += 1;
                updates.push((
                    "session.cookie_path",
                    string_arg("session_set_cookie_params", value)?.to_string_lossy(),
                ));
            }
            "domain" => {
                valid_options += 1;
                updates.push((
                    "session.cookie_domain",
                    string_arg("session_set_cookie_params", value)?.to_string_lossy(),
                ));
            }
            "secure" => {
                valid_options += 1;
                updates.push((
                    "session.cookie_secure",
                    session_cookie_bool_option(value)?.to_owned(),
                ));
            }
            "partitioned" => {
                valid_options += 1;
                updates.push((
                    "session.cookie_partitioned",
                    session_cookie_bool_option(value)?.to_owned(),
                ));
            }
            "httponly" => {
                valid_options += 1;
                updates.push((
                    "session.cookie_httponly",
                    session_cookie_bool_option(value)?.to_owned(),
                ));
            }
            "samesite" => {
                valid_options += 1;
                updates.push((
                    "session.cookie_samesite",
                    string_arg("session_set_cookie_params", value)?.to_string_lossy(),
                ));
            }
            _ => {
                context.php_warning(
                    "E_PHP_RUNTIME_SESSION_COOKIE_PARAMS_UNKNOWN_KEY",
                    format!(
                        "session_set_cookie_params(): Argument #1 ($lifetime_or_options) contains an unrecognized key \"{name}\""
                    ),
                    span.clone(),
                );
            }
        }
    }

    if valid_options == 0 {
        return Err(session_cookie_params_value_error(
            "session_set_cookie_params(): Argument #1 ($lifetime_or_options) must contain at least 1 valid key",
        ));
    }

    for (name, value) in updates {
        context.ini_set(name, value);
    }
    Ok(Value::Bool(true))
}

fn session_cookie_lifetime_is_negative(value: &str) -> bool {
    value.trim_start().starts_with('-')
}

fn session_cookie_bool_option(value: &Value) -> Result<&'static str, BuiltinError> {
    if to_bool(value).map_err(|message| conversion_error("session_set_cookie_params", message))? {
        Ok("1")
    } else {
        Ok("0")
    }
}

fn session_cookie_params_value_error(message: impl Into<String>) -> BuiltinError {
    BuiltinError::new("E_PHP_RUNTIME_BUILTIN_VALUE", message.into())
}

fn session_cookie_lifetime(context: &BuiltinContext<'_>) -> i64 {
    context
        .ini_get("session.cookie_lifetime")
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0)
}

fn session_ini_string(context: &BuiltinContext<'_>, name: &str, default: &str) -> String {
    context.ini_get(name).unwrap_or(default).to_owned()
}

fn session_ini_bool(context: &BuiltinContext<'_>, name: &str) -> bool {
    context.ini_get(name).is_some_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off" | "no"
        )
    })
}

pub(in crate::builtins::modules) fn builtin_session_name(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("session_name", "zero or one argument(s)"));
    }
    let new_name = args
        .first()
        .filter(|name| !matches!(deref_value(name), Value::Null))
        .map(|name| string_arg("session_name", name).map(|value| value.to_string_lossy()))
        .transpose()?;
    let ini_name = context
        .ini_get("session.name")
        .unwrap_or("PHPSESSID")
        .to_owned();
    let previous = {
        let Some(state) = context.session_state() else {
            return Err(session_context_error("session_name"));
        };
        if state.name() == "PHPSESSID" && ini_name != "PHPSESSID" {
            state.replace_name(ini_name);
        }
        state.name().to_owned()
    };
    if let Some(name) = new_name {
        if let Some((started_file, started_line)) =
            session_active_start_location(context.session_state())
        {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_NAME_ACTIVE",
                format!(
                    "session_name(): Session name cannot be changed when a session is active (started from {started_file} on line {started_line})"
                ),
                span,
            );
            return Ok(Value::Bool(false));
        }
        if name.as_bytes().contains(&0) {
            return Err(argument_value_error(
                "session_name",
                "#1 ($name)",
                "must not contain any null bytes",
            ));
        }
        if !session_name_is_valid(&name) {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_NAME_INVALID",
                format!(
                    "session_name(): session.name \"{name}\" must not be numeric, empty, contain null bytes or any of the following characters \"=,;.[ \\t\\r\\n\\013\\014\""
                ),
                span,
            );
            return Ok(Value::string(previous));
        }
        context.ini_set("session.name", name.clone());
        let Some(state) = context.session_state() else {
            return Err(session_context_error("session_name"));
        };
        state.replace_name(name);
    }
    Ok(Value::string(previous))
}

fn session_name_is_valid(name: &str) -> bool {
    !name.is_empty()
        && !name.as_bytes().contains(&0)
        && !session_name_is_numeric(name)
        && !name.bytes().any(|byte| {
            matches!(
                byte,
                b'=' | b',' | b';' | b'.' | b'[' | b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c
            )
        })
}

fn session_name_is_numeric(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed.parse::<f64>().is_ok()
}

pub(in crate::builtins::modules) fn builtin_session_module_name(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "session_module_name",
            "zero or one argument(s)",
        ));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_module_name"));
    };
    let previous = state.module_name().to_owned();
    if let Some(name) = args.first()
        && !matches!(deref_value(name), Value::Null)
    {
        let module = string_arg("session_module_name", name)?.to_string_lossy();
        if let Some((started_file, started_line)) =
            session_active_start_location(context.session_state())
        {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_MODULE_NAME_ACTIVE",
                format!(
                    "session_module_name(): Session save handler module cannot be changed when a session is active (started from {started_file} on line {started_line})"
                ),
                span,
            );
            return Ok(Value::Bool(false));
        }
        if module == "user" {
            return Err(argument_value_error(
                "session_module_name",
                "#1 ($module)",
                "cannot be \"user\"",
            ));
        }
        if module != "files" {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_MODULE_NAME_UNKNOWN",
                format!(
                    "session_module_name(): Session handler module \"{module}\" cannot be found"
                ),
                span,
            );
            return Ok(Value::Bool(false));
        }
        context.ini_set("session.save_handler", module.clone());
        let Some(state) = context.session_state() else {
            return Err(session_context_error("session_module_name"));
        };
        state.replace_module_name(module);
    }
    Ok(Value::string(previous))
}

pub(in crate::builtins::modules) fn builtin_session_save_path(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("session_save_path", "zero or one argument(s)"));
    }
    let previous = context
        .ini_get("session.save_path")
        .map(str::to_owned)
        .or_else(|| {
            context
                .session_state()
                .map(|state| state.save_path().to_owned())
        })
        .unwrap_or_default();
    if let Some(path) = args.first()
        && !matches!(deref_value(path), Value::Null)
        && let Some((started_file, started_line)) =
            session_active_start_location(context.session_state())
    {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_SAVE_PATH_ACTIVE",
            format!(
                "session_save_path(): Session save path cannot be changed when a session is active (started from {started_file} on line {started_line})"
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_save_path"));
    };
    if let Some(path) = args.first()
        && !matches!(deref_value(path), Value::Null)
    {
        let path = string_arg("session_save_path", path)?.to_string_lossy();
        {
            state.replace_save_path(path.clone());
        }
        context.ini_set("session.save_path", path);
    }
    Ok(Value::string(previous))
}

pub(in crate::builtins::modules) fn builtin_session_set_save_handler(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 9 {
        return Err(arity_error(
            "session_set_save_handler",
            "at least one and at most nine arguments",
        ));
    }
    if let Some((started_file, started_line)) =
        session_active_start_location(context.session_state())
    {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_SET_SAVE_HANDLER_ACTIVE",
            format!(
                "session_set_save_handler(): Session save handler cannot be changed when a session is active (started from {started_file} on line {started_line})"
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    match deref_value(&args[0]) {
        Value::Object(_) => {
            context.ini_set("session.save_handler", "user");
            if let Some(state) = context.session_state() {
                state.replace_module_name("user");
            }
            Ok(Value::Bool(true))
        }
        _ => {
            if args.len() < 6 {
                return Err(argument_type_error(
                    "session_set_save_handler",
                    "#1 ($open)",
                    "SessionHandlerInterface",
                    &args[0],
                ));
            }
            context.php_deprecation(
                "E_PHP_RUNTIME_SESSION_SET_SAVE_HANDLER_CALLBACKS",
                "session_set_save_handler(): Providing individual callbacks instead of an object implementing SessionHandlerInterface is deprecated",
                span,
            );
            for (index, name) in ["open", "close", "read", "write", "destroy", "gc"]
                .iter()
                .enumerate()
            {
                validate_session_callback_arg(index, name, &args[index])?;
            }
            context.ini_set("session.save_handler", "user");
            if let Some(state) = context.session_state() {
                state.replace_module_name("user");
            }
            Ok(Value::Bool(true))
        }
    }
}

fn validate_session_callback_arg(
    index: usize,
    name: &str,
    value: &Value,
) -> Result<(), BuiltinError> {
    match deref_value(value) {
        Value::String(callback) => {
            let callback = callback.to_string_lossy();
            if callback.eq_ignore_ascii_case("echo") {
                return Err(BuiltinError::new(
                    "E_PHP_RUNTIME_BUILTIN_TYPE",
                    format!(
                        "session_set_save_handler(): Argument #{} (${name}) must be a valid callback, function \"echo\" not found or invalid function name",
                        index + 1
                    ),
                ));
            }
            Ok(())
        }
        Value::Array(_) | Value::Callable(_) | Value::Object(_) => Ok(()),
        _ => Err(argument_type_error(
            "session_set_save_handler",
            &format!("#{} (${name})", index + 1),
            "callable",
            value,
        )),
    }
}

pub(in crate::builtins::modules) fn builtin_session_id(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("session_id", "zero or one argument(s)"));
    }
    let new_id = args
        .first()
        .filter(|id| !matches!(deref_value(id), Value::Null))
        .map(|id| string_arg("session_id", id).map(|value| value.to_string_lossy()))
        .transpose()?;
    if new_id.is_some()
        && let Some((started_file, started_line)) =
            session_active_start_location(context.session_state())
    {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_ID_ACTIVE",
            format!(
                "session_id(): Session ID cannot be changed when a session is active (started from {started_file} on line {started_line})"
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_id"));
    };
    let previous = state.id().to_owned();
    if let Some(id) = new_id {
        state.replace_id(id);
    }
    Ok(Value::string(previous))
}

pub(in crate::builtins::modules) fn builtin_session_create_id(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("session_create_id", "zero or one argument(s)"));
    }
    let prefix = args
        .first()
        .filter(|prefix| !matches!(deref_value(prefix), Value::Null))
        .map(|prefix| string_arg("session_create_id", prefix).map(|value| value.to_string_lossy()))
        .transpose()?
        .unwrap_or_default();
    if prefix.as_bytes().contains(&0) {
        return Err(argument_value_error(
            "session_create_id",
            "#1 ($prefix)",
            "must not contain any null bytes",
        ));
    }
    if prefix.len() > 256 {
        return Err(argument_value_error(
            "session_create_id",
            "#1 ($prefix)",
            "cannot be longer than 256 characters",
        ));
    }
    if !session_create_id_prefix_is_valid(&prefix) {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_CREATE_ID_PREFIX",
            "session_create_id(): Prefix cannot contain special characters. Only the A-Z, a-z, 0-9, \"-\", and \",\" characters are allowed",
            span,
        );
        return Ok(Value::Bool(false));
    }
    let id_length = session_sid_length(context);
    context
        .prepare_new_session_id()
        .map_err(|message| session_store_error("session_create_id", message))?;
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_create_id"));
    };
    Ok(Value::string(
        state.create_id_with_prefix(&prefix, id_length),
    ))
}

fn session_create_id_prefix_is_valid(prefix: &str) -> bool {
    prefix
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b','))
}

pub(in crate::builtins::modules) fn builtin_session_regenerate_id(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "session_regenerate_id",
            "zero or one argument(s)",
        ));
    }
    if let Some(delete_old_session) = args.first() {
        to_bool(delete_old_session)
            .map_err(|message| conversion_error("session_regenerate_id", message))?;
    }
    let id_length = session_sid_length(context);
    let session_is_active = context
        .session_state()
        .is_some_and(|state| state.status() == PHP_SESSION_ACTIVE);
    if session_is_active {
        context
            .prepare_new_session_id()
            .map_err(|message| session_store_error("session_regenerate_id", message))?;
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_regenerate_id"));
    };
    if !state.regenerate_id_with_length(id_length) {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_REGENERATE_ID_INACTIVE",
            "session_regenerate_id(): Session ID cannot be regenerated when there is no active session",
            span,
        );
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_session_start(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("session_start", "zero or one argument(s)"));
    }
    if let Some(options) = args.first()
        && !matches!(deref_value(options), Value::Array(_))
    {
        return Err(type_error("session_start", "array", options));
    }
    if let Some(active_start) = session_active_start_context(context.session_state()) {
        let message = match active_start {
            ActiveSessionStart::Automatic => {
                "session_start(): Ignoring session_start() because a session is already active (session started automatically)".to_owned()
            }
            ActiveSessionStart::Location(started_file, started_line) => {
                format!(
                    "session_start(): Ignoring session_start() because a session is already active (started from {started_file} on line {started_line})"
                )
            }
        };
        context.php_notice("E_PHP_RUNTIME_SESSION_START_ACTIVE", message, span);
        return Ok(Value::Bool(true));
    }
    let options = session_start_options(context, args.first(), span.clone())?;
    if !session_save_handler_is_supported(context) {
        let handler = session_save_handler(context);
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_SAVE_HANDLER_UNKNOWN",
            format!(
                "session_start(): Cannot find session save handler \"{handler}\" - session startup failed"
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    if !session_serialize_handler_is_supported(context) {
        let handler = session_serialize_handler(context);
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_SERIALIZER_UNKNOWN",
            format!(
                "session_start(): Cannot find session serialization handler \"{handler}\" - session startup failed"
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    if session_files_save_path_is_unreadable(context, span.clone()) {
        return Ok(Value::Bool(false));
    }
    let needs_lazy_load = {
        let Some(state) = context.session_state() else {
            return Err(session_context_error("session_start"));
        };
        state.needs_lazy_load()
    };
    if needs_lazy_load {
        context
            .load_pending_session_data()
            .map_err(|message| session_store_error("session_start", message))?;
    }
    let id_length = session_sid_length(context);
    let strict_mode = session_ini_bool(context, "session.use_strict_mode");
    let needs_new_id = context
        .session_state()
        .is_some_and(|state| state.id().is_empty() || strict_mode);
    if needs_new_id {
        context
            .prepare_new_session_id()
            .map_err(|message| session_store_error("session_start", message))?;
    }
    {
        let Some(state) = context.session_state() else {
            return Err(session_context_error("session_start"));
        };
        if state.destroyed() || (!state.started() && !needs_lazy_load && state.id().is_empty()) {
            state.set_data(crate::PhpArray::new());
        }
        let location = PhpDiagnosticLocation::from_span(&span);
        state.start_with_policy(id_length, strict_mode);
        state.record_start_location(location.file, location.line);
        if options.read_and_close {
            state.write_close();
        }
    }
    context.sync_session_global_from_state();
    Ok(Value::Bool(true))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SessionStartOptions {
    read_and_close: bool,
}

fn session_start_options(
    context: &mut BuiltinContext<'_>,
    options: Option<&Value>,
    span: RuntimeSourceSpan,
) -> Result<SessionStartOptions, BuiltinError> {
    let Some(options) = options else {
        return Ok(SessionStartOptions::default());
    };
    let Value::Array(options) = deref_value(options) else {
        return Err(type_error("session_start", "array", options));
    };
    let mut parsed = SessionStartOptions::default();
    for (key, value) in options.iter() {
        let ArrayKey::String(name) = key else {
            return Err(argument_value_error(
                "session_start",
                "#1 ($options)",
                "must be of type array with keys as string",
            ));
        };
        let name = name.to_string_lossy();
        match deref_value(value) {
            Value::String(_) | Value::Int(_) | Value::Bool(_) => {}
            _ => {
                return Err(BuiltinError::new(
                    "E_PHP_RUNTIME_BUILTIN_TYPE",
                    format!(
                        "session_start(): Option \"{name}\" must be of type string|int|bool, {} given",
                        php_argument_type_name(value)
                    ),
                ));
            }
        }
        if name.eq_ignore_ascii_case("read_and_close") {
            parsed.read_and_close = session_start_read_and_close_value(value)?;
        } else if name.eq_ignore_ascii_case("gc_probability") {
            let probability = session_start_int_option(&name, value)?;
            if probability < 0 {
                context.php_warning(
                    "E_PHP_RUNTIME_SESSION_GC_PROBABILITY_INVALID",
                    "session_start(): session.gc_probability must be greater than or equal to 0",
                    span.clone(),
                );
                context.php_warning(
                    "E_PHP_RUNTIME_SESSION_START_OPTION_FAILED",
                    "session_start(): Setting option \"gc_probability\" failed",
                    span.clone(),
                );
            } else {
                context.ini_set("session.gc_probability", probability.to_string());
            }
        } else if name.eq_ignore_ascii_case("gc_divisor") {
            let divisor = session_start_int_option(&name, value)?;
            if divisor <= 0 {
                context.php_warning(
                    "E_PHP_RUNTIME_SESSION_GC_DIVISOR_INVALID",
                    "session_start(): session.gc_divisor must be greater than 0",
                    span.clone(),
                );
                context.php_warning(
                    "E_PHP_RUNTIME_SESSION_START_OPTION_FAILED",
                    "session_start(): Setting option \"gc_divisor\" failed",
                    span.clone(),
                );
            } else {
                context.ini_set("session.gc_divisor", divisor.to_string());
            }
        } else {
            context.php_warning(
                "E_PHP_RUNTIME_SESSION_START_OPTION_FAILED",
                format!("session_start(): Setting option \"{name}\" failed"),
                span.clone(),
            );
        }
    }
    Ok(parsed)
}

fn session_start_int_option(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    to_int(value).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("session_start(): Option \"{name}\" value must be int-compatible: {message}"),
        )
    })
}

fn session_start_read_and_close_value(value: &Value) -> Result<bool, BuiltinError> {
    match deref_value(value) {
        Value::Bool(value) => Ok(value),
        Value::Int(value) => Ok(value > 0),
        Value::String(value) => {
            let value = value.to_string_lossy();
            let trimmed = value.trim();
            let Some(first) = trimmed.as_bytes().first() else {
                return Ok(false);
            };
            if !first.is_ascii_digit() && *first != b'+' && *first != b'-' {
                return Err(BuiltinError::new(
                    "E_PHP_RUNTIME_BUILTIN_TYPE",
                    format!(
                        "session_start(): Option \"read_and_close\" value must be of type compatible with int, \"{value}\" given"
                    ),
                ));
            }
            Ok(trimmed.parse::<i64>().unwrap_or(0) > 0)
        }
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!(
                "session_start(): Option \"read_and_close\" must be of type string|int|bool, {} given",
                php_argument_type_name(value)
            ),
        )),
    }
}

pub(in crate::builtins::modules) fn builtin_session_destroy(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_destroy", &args, 0)?;
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_destroy"));
    };
    let destroyed = state.destroy();
    if !destroyed {
        context.php_warning(
            "E_PHP_RUNTIME_SESSION_DESTROY_INACTIVE",
            "session_destroy(): Trying to destroy uninitialized session",
            span,
        );
    }
    Ok(Value::Bool(destroyed))
}

pub(in crate::builtins::modules) fn builtin_session_write_close(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_write_close", &args, 0)?;
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_write_close"));
    };
    Ok(Value::Bool(state.write_close()))
}

fn session_context_error(function: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_SESSION_CONTEXT_REQUIRED",
        format!("{function}() requires VM request-local session state"),
    )
}

fn session_store_error(function: &str, message: String) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_SESSION_STORE_UNAVAILABLE",
        format!("{function}() could not load session data: {message}"),
    )
}

fn session_sid_length(context: &BuiltinContext<'_>) -> usize {
    context
        .ini_get("session.sid_length")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| (22..=256).contains(value))
        .unwrap_or(32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArrayKey, IniRegistry, OutputBuffer, PHP_SESSION_ACTIVE, PhpArray, PhpString,
        ReferenceCell, SessionIdGenerateCallback, SessionLoadCallback, SessionState,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn context_with_session<'a>(
        output: &'a mut OutputBuffer,
        state: &'a mut SessionState,
        global: ReferenceCell,
    ) -> BuiltinContext<'a> {
        let mut context = BuiltinContext::new(output);
        context.set_session_state(state, global);
        context
    }

    #[test]
    fn session_builtins_track_cli_state() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global.clone());

        assert_eq!(
            builtin_session_status(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("status"),
            Value::Int(PHP_SESSION_NONE)
        );
        assert_eq!(
            builtin_session_name(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("name"),
            Value::string("PHPSESSID")
        );
        assert_eq!(
            builtin_session_cache_expire(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("cache expire"),
            Value::Int(180)
        );
        assert_eq!(
            builtin_session_cache_limiter(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("cache limiter"),
            Value::string("nocache")
        );
        assert_eq!(
            builtin_session_module_name(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("module name"),
            Value::string("files")
        );
        assert_eq!(
            builtin_session_save_path(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("save path"),
            Value::string("")
        );
        assert_eq!(
            builtin_session_id(
                &mut context,
                vec![Value::string("local")],
                RuntimeSourceSpan::default()
            )
            .expect("id"),
            Value::string("")
        );
        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(global.get(), Value::Array(crate::PhpArray::new()));
        assert_eq!(
            builtin_session_write_close(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("write close"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_status(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("status after write close"),
            Value::Int(PHP_SESSION_NONE)
        );
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default()).expect("id"),
            Value::string("local")
        );
        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("restart"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_destroy(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("destroy"),
            Value::Bool(true)
        );
    }

    #[test]
    fn session_cache_settings_are_backed_by_request_ini() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);
        let mut ini = IniRegistry::default();
        assert_eq!(
            ini.set("session.cache_expire", "360"),
            Some("180".to_owned())
        );
        assert_eq!(
            ini.set("session.cache_limiter", "private_no_expire"),
            Some("nocache".to_owned())
        );
        context.set_ini_registry(ini);

        assert_eq!(
            builtin_session_cache_expire(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("cache expire"),
            Value::Int(360)
        );
        assert_eq!(
            builtin_session_cache_expire(
                &mut context,
                vec![Value::Int(180)],
                RuntimeSourceSpan::default(),
            )
            .expect("replace cache expire"),
            Value::Int(360)
        );
        assert_eq!(context.ini_get("session.cache_expire"), Some("180"));
        assert_eq!(
            builtin_session_cache_limiter(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("cache limiter"),
            Value::string("private_no_expire")
        );
        assert_eq!(
            builtin_session_cache_limiter(
                &mut context,
                vec![Value::string("public")],
                RuntimeSourceSpan::default(),
            )
            .expect("replace cache limiter"),
            Value::string("private_no_expire")
        );
        assert_eq!(context.ini_get("session.cache_limiter"), Some("public"));

        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_cache_expire(
                &mut context,
                vec![Value::Int(60)],
                RuntimeSourceSpan::default(),
            )
            .expect("active cache expire"),
            Value::Int(180)
        );
        assert_eq!(
            builtin_session_cache_limiter(
                &mut context,
                vec![Value::string("nocache")],
                RuntimeSourceSpan::default(),
            )
            .expect("active cache limiter"),
            Value::Bool(false)
        );
        assert_eq!(context.ini_get("session.cache_expire"), Some("180"));
        assert_eq!(context.ini_get("session.cache_limiter"), Some("public"));
        let diagnostics = context.take_diagnostics();
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains(
            "session_cache_expire(): Session cache expiration cannot be changed when a session is active"
        )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains(
            "session_cache_limiter(): Session cache limiter cannot be changed when a session is active"
        )));
    }

    #[test]
    fn session_save_path_is_backed_by_request_ini_and_rejects_active_changes() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);
        let mut ini = IniRegistry::default();
        let test_root =
            std::env::temp_dir().join(format!("phrust-session-save-path-{}", std::process::id()));
        let initial_dir = test_root.join("initial");
        let replacement_dir = test_root.join("replacement");
        std::fs::create_dir_all(&initial_dir).expect("create initial session save path");
        std::fs::create_dir_all(&replacement_dir).expect("create replacement session save path");
        let initial_path = initial_dir.to_string_lossy().into_owned();
        let replacement_path = replacement_dir.to_string_lossy().into_owned();
        assert_eq!(
            ini.set("session.save_path", &initial_path),
            Some("".to_owned())
        );
        context.set_ini_registry(ini);

        assert_eq!(
            builtin_session_save_path(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("save path from ini"),
            Value::string(initial_path.as_str())
        );
        assert_eq!(
            builtin_session_save_path(
                &mut context,
                vec![Value::string(replacement_path.as_str())],
                RuntimeSourceSpan::default(),
            )
            .expect("replace save path"),
            Value::string(initial_path.as_str())
        );
        assert_eq!(
            context.ini_get("session.save_path"),
            Some(replacement_path.as_str())
        );
        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_save_path(
                &mut context,
                vec![Value::string("/tmp/rejected-sessions")],
                RuntimeSourceSpan::default(),
            )
            .expect("active save path"),
            Value::Bool(false)
        );
        assert_eq!(
            context.ini_get("session.save_path"),
            Some(replacement_path.as_str())
        );
        assert!(context.take_diagnostics().iter().any(|diagnostic| {
            diagnostic.message().contains(
                "session_save_path(): Session save path cannot be changed when a session is active",
            )
        }));
    }

    #[test]
    fn session_name_is_backed_by_request_ini_and_rejects_invalid_names() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);
        let mut ini = IniRegistry::default();
        assert_eq!(
            ini.set("session.name", "APPSESSID"),
            Some("PHPSESSID".to_owned())
        );
        context.set_ini_registry(ini);

        assert_eq!(
            builtin_session_name(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("name from ini"),
            Value::string("APPSESSID")
        );
        assert_eq!(
            builtin_session_name(
                &mut context,
                vec![Value::string("\t")],
                RuntimeSourceSpan::default(),
            )
            .expect("invalid tab name returns previous"),
            Value::string("APPSESSID")
        );
        assert_eq!(context.ini_get("session.name"), Some("APPSESSID"));
        assert!(context.take_diagnostics().iter().any(|diagnostic| {
            diagnostic
                .message()
                .contains("session.name \"\t\" must not be numeric, empty, contain null bytes")
        }));

        let error = builtin_session_name(
            &mut context,
            vec![Value::string("AB\0CD")],
            RuntimeSourceSpan::default(),
        )
        .expect_err("null-byte session name should be a value error");
        assert_eq!(
            error.message(),
            "session_name(): Argument #1 ($name) must not contain any null bytes"
        );
        assert_eq!(context.ini_get("session.name"), Some("APPSESSID"));

        assert_eq!(
            builtin_session_name(
                &mut context,
                vec![Value::string("NEXTSESSID")],
                RuntimeSourceSpan::default(),
            )
            .expect("valid name returns previous"),
            Value::string("APPSESSID")
        );
        assert_eq!(context.ini_get("session.name"), Some("NEXTSESSID"));
        assert_eq!(
            builtin_session_name(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("updated name"),
            Value::string("NEXTSESSID")
        );

        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_name(
                &mut context,
                vec![Value::string("TOO_LATE")],
                RuntimeSourceSpan::default(),
            )
            .expect("active name mutation"),
            Value::Bool(false)
        );
        assert_eq!(context.ini_get("session.name"), Some("NEXTSESSID"));
        assert!(context.take_diagnostics().iter().any(|diagnostic| {
            diagnostic
                .message()
                .contains("Session name cannot be changed when a session is active")
        }));
    }

    #[test]
    fn session_create_id_generates_prefixed_ids_and_validates_prefixes() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);

        assert_eq!(
            builtin_session_create_id(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("create default id"),
            Value::string("phrustcli00000001000000000000000")
        );
        assert_eq!(
            builtin_session_create_id(
                &mut context,
                vec![Value::string("XYZ")],
                RuntimeSourceSpan::default(),
            )
            .expect("create prefixed id"),
            Value::string("XYZphrustcli00000002000000000000000")
        );
        assert_eq!(
            builtin_session_create_id(
                &mut context,
                vec![Value::string("_")],
                RuntimeSourceSpan::default(),
            )
            .expect("invalid prefix"),
            Value::Bool(false)
        );
        assert!(context.take_diagnostics().iter().any(|diagnostic| {
            diagnostic
                .message()
                .contains("Prefix cannot contain special characters")
        }));

        let long_prefix = "A".repeat(257);
        let error = builtin_session_create_id(
            &mut context,
            vec![Value::string(long_prefix)],
            RuntimeSourceSpan::default(),
        )
        .expect_err("long prefix should be a value error");
        assert_eq!(
            error.message(),
            "session_create_id(): Argument #1 ($prefix) cannot be longer than 256 characters"
        );

        let error = builtin_session_create_id(
            &mut context,
            vec![Value::string("AB\0CD")],
            RuntimeSourceSpan::default(),
        )
        .expect_err("null-byte prefix should be a value error");
        assert_eq!(
            error.message(),
            "session_create_id(): Argument #1 ($prefix) must not contain any null bytes"
        );
    }

    #[test]
    fn session_start_options_support_read_and_close_and_validate_shape() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);

        let mut options = PhpArray::new();
        options.insert(string_array_key("read_and_close"), Value::Bool(true));
        assert_eq!(
            builtin_session_start(
                &mut context,
                vec![Value::Array(options)],
                RuntimeSourceSpan::default(),
            )
            .expect("read and close"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_status(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("closed status"),
            Value::Int(PHP_SESSION_NONE)
        );

        let mut options = PhpArray::new();
        options.insert(string_array_key("read_and_close"), Value::Int(-1));
        assert_eq!(
            builtin_session_start(
                &mut context,
                vec![Value::Array(options)],
                RuntimeSourceSpan::default(),
            )
            .expect("negative read and close"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_status(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("active status"),
            Value::Int(PHP_SESSION_ACTIVE)
        );

        let mut options = PhpArray::new();
        options.insert(string_array_key("read_and_close"), Value::string("true"));
        assert_eq!(
            builtin_session_start(
                &mut context,
                vec![Value::Array(options)],
                RuntimeSourceSpan::default(),
            )
            .expect("active session ignores invalid option"),
            Value::Bool(true)
        );
        assert!(context.take_diagnostics().iter().any(|diagnostic| {
            diagnostic
                .message()
                .contains("Ignoring session_start() because a session is already active")
        }));
        assert_eq!(
            builtin_session_write_close(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("close"),
            Value::Bool(true)
        );

        let mut options = PhpArray::new();
        options.insert(string_array_key("read_and_close"), Value::string("false"));
        let error = builtin_session_start(
            &mut context,
            vec![Value::Array(options)],
            RuntimeSourceSpan::default(),
        )
        .expect_err("non-numeric string should be a type error");
        assert_eq!(
            error.message(),
            "session_start(): Option \"read_and_close\" value must be of type compatible with int, \"false\" given"
        );

        let mut options = PhpArray::new();
        options.insert(ArrayKey::Int(0), Value::string("ignored"));
        let error = builtin_session_start(
            &mut context,
            vec![Value::Array(options)],
            RuntimeSourceSpan::default(),
        )
        .expect_err("numeric key should be a value error");
        assert_eq!(
            error.message(),
            "session_start(): Argument #1 ($options) must be of type array with keys as string"
        );

        let mut options = PhpArray::new();
        options.insert(string_array_key("option"), Value::Bool(false));
        assert_eq!(
            builtin_session_start(
                &mut context,
                vec![Value::Array(options)],
                RuntimeSourceSpan::default(),
            )
            .expect("unsupported option warns and starts"),
            Value::Bool(true)
        );
        assert!(context.take_diagnostics().iter().any(|diagnostic| {
            diagnostic
                .message()
                .contains("session_start(): Setting option \"option\" failed")
        }));
    }

    #[test]
    fn session_start_strict_mode_replaces_preselected_ids() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);
        let mut ini = IniRegistry::default();
        assert_eq!(
            ini.set("session.use_strict_mode", "true"),
            Some("0".to_owned())
        );
        context.set_ini_registry(ini);

        assert_eq!(
            builtin_session_id(
                &mut context,
                vec![Value::string("XYZphrustcli00000001000000000000000")],
                RuntimeSourceSpan::default(),
            )
            .expect("set id"),
            Value::string("")
        );
        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start strict"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("strict id"),
            Value::string("phrustcli00000001000000000000000")
        );
    }

    #[test]
    fn session_start_and_regenerate_use_request_sid_length() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);
        let mut ini = IniRegistry::default();
        assert_eq!(ini.set("session.sid_length", "22"), Some("32".to_owned()));
        context.set_ini_registry(ini);

        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("started id"),
            Value::string("phrustcli0000000100000")
        );
        assert_eq!(
            builtin_session_regenerate_id(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("regenerate"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("regenerated id"),
            Value::string("phrustcli0000000200000")
        );
        assert_eq!(
            builtin_session_id(
                &mut context,
                vec![Value::string("cannot-change")],
                RuntimeSourceSpan::default(),
            )
            .expect("active id mutation"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("unchanged active id"),
            Value::string("phrustcli0000000200000")
        );
        assert!(context.take_diagnostics().iter().any(|diagnostic| {
            diagnostic
                .message()
                .contains("Session ID cannot be changed when a session is active")
        }));
    }

    #[test]
    fn session_get_cookie_params_reads_request_ini_values() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);
        let mut ini = IniRegistry::default();
        assert_eq!(
            ini.set("session.cookie_lifetime", "3600"),
            Some("0".to_owned())
        );
        assert_eq!(
            ini.set("session.cookie_path", "/admin"),
            Some("/".to_owned())
        );
        assert_eq!(
            ini.set("session.cookie_domain", ".example.test"),
            Some("".to_owned())
        );
        assert_eq!(ini.set("session.cookie_secure", "1"), Some("0".to_owned()));
        assert_eq!(
            ini.set("session.cookie_partitioned", "1"),
            Some("0".to_owned())
        );
        assert_eq!(
            ini.set("session.cookie_httponly", "1"),
            Some("0".to_owned())
        );
        assert_eq!(
            ini.set("session.cookie_samesite", "Lax"),
            Some("".to_owned())
        );
        context.set_ini_registry(ini);

        let mut expected = PhpArray::new();
        expected.insert(string_array_key("lifetime"), Value::Int(3600));
        expected.insert(string_array_key("path"), Value::string("/admin"));
        expected.insert(string_array_key("domain"), Value::string(".example.test"));
        expected.insert(string_array_key("secure"), Value::Bool(true));
        expected.insert(string_array_key("partitioned"), Value::Bool(true));
        expected.insert(string_array_key("httponly"), Value::Bool(true));
        expected.insert(string_array_key("samesite"), Value::string("Lax"));

        assert_eq!(
            builtin_session_get_cookie_params(
                &mut context,
                Vec::new(),
                RuntimeSourceSpan::default()
            )
            .expect("cookie params"),
            Value::Array(expected)
        );
    }

    #[test]
    fn session_set_cookie_params_updates_ini_until_session_is_active() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);

        assert_eq!(
            builtin_session_set_cookie_params(
                &mut context,
                vec![Value::Int(3600), Value::string("/foo")],
                RuntimeSourceSpan::default(),
            )
            .expect("set before active"),
            Value::Bool(true)
        );
        assert_eq!(context.ini_get("session.cookie_lifetime"), Some("3600"));
        assert_eq!(context.ini_get("session.cookie_path"), Some("/foo"));

        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_set_cookie_params(
                &mut context,
                vec![Value::Int(1800), Value::string("/bar")],
                RuntimeSourceSpan::default(),
            )
            .expect("set while active"),
            Value::Bool(false)
        );
        assert_eq!(context.ini_get("session.cookie_lifetime"), Some("3600"));
        assert_eq!(context.ini_get("session.cookie_path"), Some("/foo"));
    }

    #[test]
    fn session_set_cookie_params_accepts_array_options_and_rejects_invalid_shapes() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);

        let error = builtin_session_set_cookie_params(
            &mut context,
            vec![Value::Array(PhpArray::new())],
            RuntimeSourceSpan::default(),
        )
        .expect_err("empty option array should be a value error");
        assert_eq!(
            error.message(),
            "session_set_cookie_params(): Argument #1 ($lifetime_or_options) must contain at least 1 valid key"
        );

        let mut options = PhpArray::new();
        options.insert(string_array_key("Secure"), Value::Bool(true));
        options.insert(string_array_key("partitioned"), Value::Bool(true));
        options.insert(string_array_key("samesite"), Value::string("please"));
        options.insert(string_array_key("lifetime"), Value::string("42"));
        options.insert(ArrayKey::Int(0), Value::string("ignored"));
        assert_eq!(
            builtin_session_set_cookie_params(
                &mut context,
                vec![Value::Array(options)],
                RuntimeSourceSpan::default(),
            )
            .expect("array options"),
            Value::Bool(true)
        );
        assert_eq!(context.ini_get("session.cookie_secure"), Some("1"));
        assert_eq!(context.ini_get("session.cookie_partitioned"), Some("1"));
        assert_eq!(context.ini_get("session.cookie_samesite"), Some("please"));
        assert_eq!(context.ini_get("session.cookie_lifetime"), Some("42"));
        assert!(context.take_diagnostics().iter().any(|diagnostic| {
            diagnostic.message()
                == "session_set_cookie_params(): Argument #1 ($lifetime_or_options) cannot contain numeric keys"
        }));

        let mut options = PhpArray::new();
        options.insert(string_array_key("path"), Value::string("newpath/"));
        let error = builtin_session_set_cookie_params(
            &mut context,
            vec![Value::Array(options), Value::string("extra")],
            RuntimeSourceSpan::default(),
        )
        .expect_err("non-null path after option array should be a value error");
        assert_eq!(
            error.message(),
            "session_set_cookie_params(): Argument #2 ($path) must be null when argument #1 ($lifetime_or_options) is an array"
        );

        let mut options = PhpArray::new();
        options.insert(string_array_key("lifetime"), Value::Int(-10));
        assert_eq!(
            builtin_session_set_cookie_params(
                &mut context,
                vec![Value::Array(options)],
                RuntimeSourceSpan::default(),
            )
            .expect("negative lifetime warning"),
            Value::Bool(false)
        );
        assert_eq!(context.ini_get("session.cookie_lifetime"), Some("42"));
        let diagnostics = context.take_diagnostics();
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.message()
                == "session_set_cookie_params(): CookieLifetime cannot be negative"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn session_builtins_use_seeded_web_state() {
        let mut seeded = PhpArray::new();
        seeded.insert(ArrayKey::String(PhpString::from("n")), Value::Int(7));
        let mut output = OutputBuffer::new();
        let mut state = SessionState::seeded(
            "APPSESSID".to_string(),
            "incoming123".to_string(),
            seeded.clone(),
            Some("generated456".to_string()),
        );
        let global = ReferenceCell::new(Value::Array(seeded));
        let mut context = context_with_session(&mut output, &mut state, global.clone());

        assert_eq!(
            builtin_session_name(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("name"),
            Value::string("APPSESSID")
        );
        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default()).expect("id"),
            Value::string("incoming123")
        );
        assert_eq!(
            global.get(),
            Value::Array({
                let mut array = PhpArray::new();
                array.insert(ArrayKey::String(PhpString::from("n")), Value::Int(7));
                array
            })
        );
        assert!(!state.newly_created());
    }

    #[test]
    fn session_start_loads_lazy_seeded_web_state() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::seeded_lazy(
            "APPSESSID".to_string(),
            "incoming123".to_string(),
            Some("generated456".to_string()),
        );
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let loader = SessionLoadCallback::new(|id| {
            assert_eq!(id, "incoming123");
            let mut data = PhpArray::new();
            data.insert(ArrayKey::String(PhpString::from("n")), Value::Int(7));
            Ok(data)
        });
        let mut context = context_with_session(&mut output, &mut state, global.clone());
        context.set_session_loader(Some(&loader));

        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(
            global.get(),
            Value::Array({
                let mut array = PhpArray::new();
                array.insert(ArrayKey::String(PhpString::from("n")), Value::Int(7));
                array
            })
        );
        assert!(!state.newly_created());
    }

    #[test]
    fn web_session_id_generation_is_deferred_until_session_start() {
        let generated = Arc::new(AtomicUsize::new(0));
        let generated_for_callback = Arc::clone(&generated);
        let generator = SessionIdGenerateCallback::new(move || {
            let sequence = generated_for_callback.fetch_add(1, Ordering::Relaxed) + 1;
            Ok(format!("secure-generated-id-{sequence}"))
        });
        let mut output = OutputBuffer::new();
        let mut state = SessionState::seeded_lazy("APPSESSID", "", None);
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global);
        context.set_session_id_generator(Some(&generator));

        assert_eq!(generated.load(Ordering::Relaxed), 0);
        assert_eq!(
            builtin_session_status(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("status"),
            Value::Int(PHP_SESSION_NONE)
        );
        assert_eq!(generated.load(Ordering::Relaxed), 0);
        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(generated.load(Ordering::Relaxed), 1);
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default()).expect("id"),
            Value::string("secure-generated-id-1")
        );
        assert_eq!(
            builtin_session_create_id(
                &mut context,
                vec![Value::string("prefix-")],
                RuntimeSourceSpan::default(),
            )
            .expect("create id"),
            Value::string("prefix-secure-generated-id-2")
        );
        assert_eq!(generated.load(Ordering::Relaxed), 2);
        assert_eq!(
            builtin_session_regenerate_id(&mut context, Vec::new(), RuntimeSourceSpan::default(),)
                .expect("regenerate id"),
            Value::Bool(true)
        );
        assert_eq!(generated.load(Ordering::Relaxed), 3);
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default()).expect("id"),
            Value::string("secure-generated-id-3")
        );
    }
}
