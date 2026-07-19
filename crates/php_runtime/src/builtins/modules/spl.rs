//! Spl builtin registry slice.

use super::core::*;
use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "class_implements",
        builtin_spl_autoload_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iterator_apply",
        builtin_spl_autoload_requires_vm,
        BuiltinCompatibility::Php,
    ),
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
    _args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_SPL_AUTOLOAD_CONTEXT_REQUIRED",
        "SPL autoload builtins require VM autoload stack state",
    ))
}
pub(in crate::builtins::modules) fn builtin_spl_object_id(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("spl_object_id", &args, 1)?;
    let value = deref_value(&args[0]);
    let id = if let Value::Object(object) = &value {
        object.id()
    } else if let Some(payload) = value.as_closure() {
        payload.id
    } else {
        return Err(type_error("spl_object_id", "object", &args[0]));
    };
    Ok(Value::Int(id as i64))
}
pub(in crate::builtins::modules) fn builtin_spl_object_hash(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("spl_object_hash", &args, 1)?;
    let value = deref_value(&args[0]);
    let id = if let Value::Object(object) = &value {
        object.id()
    } else if let Some(payload) = value.as_closure() {
        payload.id
    } else {
        return Err(type_error("spl_object_hash", "object", &args[0]));
    };
    Ok(Value::string(format!("{id:032x}")))
}
