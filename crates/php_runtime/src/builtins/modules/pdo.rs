//! PDO builtin registry slice.

use super::core::expect_arity;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use crate::{PhpArray, Value};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[BuiltinEntry::new(
    "pdo_drivers",
    builtin_pdo_drivers,
    BuiltinCompatibility::Php,
)];

pub(in crate::builtins::modules) fn builtin_pdo_drivers(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("pdo_drivers", &args, 0)?;
    Ok(Value::Array(PhpArray::from_packed(vec![
        Value::string("mysql"),
        Value::string("pgsql"),
        Value::string("sqlite"),
    ])))
}
