//! Spl builtin registry slice.

use super::core::*;
use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "iterator_count",
        builtin_spl_autoload_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iterator_to_array",
        builtin_spl_autoload_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "spl_autoload_call",
        builtin_spl_autoload_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "spl_autoload_functions",
        builtin_spl_autoload_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "spl_autoload_register",
        builtin_spl_autoload_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "spl_autoload_unregister",
        builtin_spl_autoload_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "spl_object_hash",
        builtin_spl_object_hash,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "spl_object_id",
        builtin_spl_object_id,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_spl_autoload_requires_vm(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_SPL_AUTOLOAD_CONTEXT_REQUIRED",
        "SPL autoload builtins require VM autoload stack state",
    ))
}
pub(in crate::builtins::modules) fn builtin_spl_object_id(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("spl_object_id", &args, 1)?;
    let Value::Object(object) = deref_value(&args[0]) else {
        return Err(type_error("spl_object_id", "object", &args[0]));
    };
    Ok(Value::Int(object.id() as i64))
}
pub(in crate::builtins::modules) fn builtin_spl_object_hash(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("spl_object_hash", &args, 1)?;
    let Value::Object(object) = deref_value(&args[0]) else {
        return Err(type_error("spl_object_hash", "object", &args[0]));
    };
    Ok(Value::string(format!("{:032x}", object.id())))
}
