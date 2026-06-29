//! SimpleXML extension builtins for the bounded runtime slice.

use super::core::{expect_arity, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{Value, xml};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[BuiltinEntry::new(
    "simplexml_load_string",
    builtin_simplexml_load_string,
    BuiltinCompatibility::Php,
)];

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
