//! SimpleXML extension builtins for the bounded runtime slice.

use super::core::{expect_arity, resolve_runtime_path, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{Value, xml};
use std::fs;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "simplexml_load_string",
        builtin_simplexml_load_string,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "simplexml_load_file",
        builtin_simplexml_load_file,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_simplexml_load_string(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("simplexml_load_string", &args, 1)?;
    let input = string_arg("simplexml_load_string", &args[0])?;
    let input = std::str::from_utf8(input.as_bytes()).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_SIMPLEXML_UTF8",
            "simplexml_load_string(): input must be valid UTF-8",
        )
    })?;
    xml::simplexml_load_string(input).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_SIMPLEXML_PARSE",
            format!("simplexml_load_string(): {message}"),
        )
    })
}

fn builtin_simplexml_load_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("simplexml_load_file", &args, 1)?;
    let path = string_arg("simplexml_load_file", &args[0])?.to_string_lossy();
    let resolved = resolve_runtime_path(context, &path);
    if !context.filesystem_capabilities().allows_path(&resolved) {
        return Ok(Value::Bool(false));
    }
    let bytes = match fs::read(&resolved) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(Value::Bool(false)),
    };
    let input = std::str::from_utf8(&bytes).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_SIMPLEXML_UTF8",
            "simplexml_load_file(): input must be valid UTF-8",
        )
    })?;
    xml::simplexml_load_string(input).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_SIMPLEXML_PARSE",
            format!("simplexml_load_file(): {message}"),
        )
    })
}
