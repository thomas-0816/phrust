//! Date builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use crate::{Value, datetime, to_bool};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("date", builtin_date, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "date_default_timezone_get",
        builtin_date_default_timezone_get,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "date_default_timezone_set",
        builtin_date_default_timezone_set,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("strtotime", builtin_strtotime, BuiltinCompatibility::Php),
    BuiltinEntry::new("hrtime", builtin_hrtime, BuiltinCompatibility::Php),
    BuiltinEntry::new("time", builtin_time, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "timezone_identifiers_list",
        builtin_timezone_identifiers_list,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_date_default_timezone_get(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("date_default_timezone_get", &args, 0)?;
    Ok(Value::string(context.default_timezone()))
}
pub(in crate::builtins::modules) fn builtin_date(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("date", "one or two argument(s)"));
    }
    let format = string_arg("date", &args[0])?.to_string_lossy();
    let timestamp = args
        .get(1)
        .map(|value| int_arg("date", value))
        .transpose()?
        .unwrap_or_else(datetime::current_timestamp);
    Ok(Value::string(datetime::format_timestamp(
        timestamp,
        context.default_timezone(),
        &format,
    )))
}
pub(in crate::builtins::modules) fn builtin_time(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("time", &args, 0)?;
    Ok(Value::Int(datetime::current_timestamp()))
}
pub(in crate::builtins::modules) fn builtin_hrtime(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("hrtime", "zero or one argument(s)"));
    }
    let as_number = args
        .first()
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hrtime", message))?
        .unwrap_or(false);
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| value_error("hrtime", "system time is before UNIX epoch"))?;
    let seconds = i64::try_from(elapsed.as_secs())
        .map_err(|_| value_error("hrtime", "timestamp exceeds PHP integer range"))?;
    let nanos = i64::from(elapsed.subsec_nanos());
    if as_number {
        let total = seconds
            .checked_mul(1_000_000_000)
            .and_then(|value| value.checked_add(nanos))
            .ok_or_else(|| value_error("hrtime", "timestamp exceeds PHP integer range"))?;
        return Ok(Value::Int(total));
    }
    Ok(Value::packed_array(vec![
        Value::Int(seconds),
        Value::Int(nanos),
    ]))
}
pub(in crate::builtins::modules) fn builtin_strtotime(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("strtotime", "one or two argument(s)"));
    }
    let text = string_arg("strtotime", &args[0])?.to_string_lossy();
    let base = args
        .get(1)
        .map(|value| int_arg("strtotime", value))
        .transpose()?
        .unwrap_or_else(datetime::current_timestamp);
    Ok(datetime::parse_datetime_text(&text, base).map_or(Value::Bool(false), Value::Int))
}
pub(in crate::builtins::modules) fn builtin_date_default_timezone_set(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("date_default_timezone_set", &args, 1)?;
    let identifier = string_arg("date_default_timezone_set", &args[0])?.to_string_lossy();
    if !datetime::is_valid_timezone(&identifier) {
        return Ok(Value::Bool(false));
    }
    context.set_default_timezone(identifier);
    Ok(Value::Bool(true))
}
pub(in crate::builtins::modules) fn builtin_timezone_identifiers_list(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error(
            "timezone_identifiers_list",
            "zero to two argument(s)",
        ));
    }
    Ok(Value::packed_array(
        datetime::TIMEZONE_IDENTIFIERS
            .iter()
            .map(|identifier| Value::string(*identifier))
            .collect(),
    ))
}
