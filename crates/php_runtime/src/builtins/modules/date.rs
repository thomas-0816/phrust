//! Date builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{Value, datetime, normalize_class_name, to_bool};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("checkdate", builtin_checkdate, BuiltinCompatibility::Php),
    BuiltinEntry::new("date", builtin_date, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "date_create",
        builtin_date_create,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "date_create_immutable_from_format",
        builtin_date_create_immutable_from_format,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "date_format",
        builtin_date_format,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("date_diff", builtin_date_diff, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "date_interval_format",
        builtin_date_interval_format,
        BuiltinCompatibility::Php,
    ),
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
    BuiltinEntry::new("gmdate", builtin_gmdate, BuiltinCompatibility::Php),
    BuiltinEntry::new("microtime", builtin_microtime, BuiltinCompatibility::Php),
    BuiltinEntry::new("strtotime", builtin_strtotime, BuiltinCompatibility::Php),
    BuiltinEntry::new("hrtime", builtin_hrtime, BuiltinCompatibility::Php),
    BuiltinEntry::new("time", builtin_time, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "timezone_identifiers_list",
        builtin_timezone_identifiers_list,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "timezone_name_get",
        builtin_timezone_name_get,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "timezone_open",
        builtin_timezone_open,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_checkdate(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("checkdate", &args, 3)?;
    let month = int_arg("checkdate", &args[0])?;
    let day = int_arg("checkdate", &args[1])?;
    let year = int_arg("checkdate", &args[2])?;
    Ok(Value::Bool(is_valid_gregorian_date(month, day, year)))
}

pub(in crate::builtins::modules) fn builtin_date_default_timezone_get(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("date_default_timezone_get", &args, 0)?;
    Ok(Value::string(context.default_timezone()))
}

fn is_valid_gregorian_date(month: i64, day: i64, year: i64) -> bool {
    if !(1..=32767).contains(&year) || !(1..=12).contains(&month) {
        return false;
    }
    (1..=days_in_month(month, year)).contains(&day)
}

fn days_in_month(month: i64, year: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
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
pub(in crate::builtins::modules) fn builtin_gmdate(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gmdate", "one or two argument(s)"));
    }
    let format = string_arg("gmdate", &args[0])?.to_string_lossy();
    let timestamp = args
        .get(1)
        .map(|value| int_arg("gmdate", value))
        .transpose()?
        .unwrap_or_else(datetime::current_timestamp);
    Ok(Value::string(datetime::format_timestamp(
        timestamp, "GMT", &format,
    )))
}
pub(in crate::builtins::modules) fn builtin_date_create(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error("date_create", "zero, one, or two argument(s)"));
    }
    let text = args
        .first()
        .map(nullable_string_arg)
        .transpose()?
        .unwrap_or_else(|| "now".to_owned());
    let timezone = match args.get(1) {
        Some(value) if !matches!(deref_value(value), Value::Null) => {
            date_create_timezone_name("date_create", value)?
        }
        _ => context.default_timezone().to_owned(),
    };
    let timestamp =
        datetime::parse_datetime_text_in_timezone(&text, datetime::current_timestamp(), &timezone);
    Ok(timestamp.map_or(Value::Bool(false), |timestamp| {
        datetime::datetime_object(timestamp, &timezone)
    }))
}

pub(in crate::builtins::modules) fn builtin_date_create_immutable_from_format(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error(
            "date_create_immutable_from_format",
            "two or three argument(s)",
        ));
    }
    let format = string_arg("date_create_immutable_from_format", &args[0])?.to_string_lossy();
    let text = string_arg("date_create_immutable_from_format", &args[1])?.to_string_lossy();
    let timezone = match args.get(2) {
        Some(value) if !matches!(deref_value(value), Value::Null) => {
            date_create_timezone_name("date_create_immutable_from_format", value)?
        }
        _ => context.default_timezone().to_owned(),
    };
    Ok(parse_datetime_from_format(&format, &text, &timezone)
        .map_or(Value::Bool(false), |timestamp| {
            datetime::datetime_immutable_object(timestamp, &timezone)
        }))
}
pub(in crate::builtins::modules) fn builtin_time(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("time", &args, 0)?;
    Ok(Value::Int(datetime::current_timestamp()))
}
pub(in crate::builtins::modules) fn builtin_microtime(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("microtime", "zero or one argument(s)"));
    }
    let as_float = args
        .first()
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("microtime", message))?
        .unwrap_or(false);
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| value_error("microtime", "system time is before UNIX epoch"))?;
    let seconds = elapsed.as_secs();
    let micros = elapsed.subsec_micros();
    if as_float {
        return Ok(Value::float(
            seconds as f64 + f64::from(micros) / 1_000_000.0,
        ));
    }
    Ok(Value::string(format!("0.{micros:06} {seconds}")))
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

fn nullable_string_arg(value: &Value) -> Result<String, BuiltinError> {
    if matches!(deref_value(value), Value::Null) {
        return Ok("now".to_owned());
    }
    Ok(string_arg("date_create", value)?.to_string_lossy())
}

fn date_create_timezone_name(function: &str, value: &Value) -> Result<String, BuiltinError> {
    let Value::Object(object) = deref_value(value) else {
        return Err(type_error(function, "?DateTimeZone", value));
    };
    if normalize_class_name(&object.class_name()) != "datetimezone" {
        return Err(type_error(function, "?DateTimeZone", value));
    }
    datetime::object_timezone(&object)
        .ok_or_else(|| value_error(function, "DateTimeZone object has no timezone name"))
}

fn parse_datetime_from_format(format: &str, text: &str, timezone: &str) -> Option<i64> {
    let format = format.strip_prefix('!').unwrap_or(format);
    match format {
        "Y-m-d H:i:s" => datetime::parse_datetime_text_in_timezone(text, 0, timezone),
        "Y-m-d" => {
            let text = format!("{text} 00:00:00");
            datetime::parse_datetime_text_in_timezone(&text, 0, timezone)
        }
        "U" => text.trim().parse::<i64>().ok(),
        "U.u" => text
            .trim()
            .split_once('.')
            .map_or(text.trim(), |(seconds, _)| seconds)
            .parse::<i64>()
            .ok(),
        _ => None,
    }
}
pub(in crate::builtins::modules) fn builtin_date_format(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("date_format", &args, 2)?;
    let Value::Object(object) = deref_value(&args[0]) else {
        return Err(type_error("date_format", "DateTimeInterface", &args[0]));
    };
    let format = string_arg("date_format", &args[1])?.to_string_lossy();
    let timestamp = datetime::object_timestamp(&object)
        .ok_or_else(|| value_error("date_format", "object is not a DateTimeInterface MVP"))?;
    let timezone = datetime::object_timezone(&object).unwrap_or_else(|| "UTC".to_string());
    Ok(Value::string(datetime::format_timestamp(
        timestamp, &timezone, &format,
    )))
}
pub(in crate::builtins::modules) fn builtin_date_diff(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("date_diff", &args, 2)?;
    let Value::Object(left) = deref_value(&args[0]) else {
        return Err(type_error("date_diff", "DateTimeInterface", &args[0]));
    };
    let Value::Object(right) = deref_value(&args[1]) else {
        return Err(type_error("date_diff", "DateTimeInterface", &args[1]));
    };
    if datetime::object_timestamp(&left).is_none() {
        return Err(value_error(
            "date_diff",
            "first object is not a DateTimeInterface MVP",
        ));
    }
    if datetime::object_timestamp(&right).is_none() {
        return Err(value_error(
            "date_diff",
            "second object is not a DateTimeInterface MVP",
        ));
    }
    Ok(datetime::diff_objects(&left, &right))
}
pub(in crate::builtins::modules) fn builtin_timezone_open(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("timezone_open", &args, 1)?;
    let timezone = string_arg("timezone_open", &args[0])?.to_string_lossy();
    Ok(datetime::datetimezone_object(&timezone).unwrap_or(Value::Bool(false)))
}
pub(in crate::builtins::modules) fn builtin_timezone_name_get(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("timezone_name_get", &args, 1)?;
    let Value::Object(object) = deref_value(&args[0]) else {
        return Err(type_error("timezone_name_get", "DateTimeZone", &args[0]));
    };
    Ok(datetime::object_timezone(&object).map_or(Value::Bool(false), Value::string))
}
pub(in crate::builtins::modules) fn builtin_date_interval_format(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("date_interval_format", &args, 2)?;
    let Value::Object(object) = deref_value(&args[0]) else {
        return Err(type_error("date_interval_format", "DateInterval", &args[0]));
    };
    let seconds = match object.get_property("__seconds") {
        Some(Value::Int(value)) => value,
        _ => {
            return Err(value_error(
                "date_interval_format",
                "object is not a DateInterval MVP",
            ));
        }
    };
    let format = string_arg("date_interval_format", &args[1])?.to_string_lossy();
    Ok(Value::string(datetime::format_interval(seconds, &format)))
}
pub(in crate::builtins::modules) fn builtin_date_default_timezone_set(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("date_default_timezone_set", &args, 1)?;
    let identifier = string_arg("date_default_timezone_set", &args[0])?.to_string_lossy();
    let Some(identifier) = datetime::normalize_timezone_identifier(&identifier) else {
        return Ok(Value::Bool(false));
    };
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

#[cfg(test)]
mod tests {
    use super::{
        BuiltinContext, RuntimeSourceSpan, builtin_date_create,
        builtin_date_create_immutable_from_format, builtin_date_diff,
    };
    use crate::{OutputBuffer, Value, datetime};

    #[test]
    fn date_create_returns_datetime_object_with_explicit_timezone() {
        let timezone = datetime::datetimezone_object("Europe/Berlin").expect("timezone object");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let result = builtin_date_create(
            &mut context,
            vec![Value::string("2024-02-03 04:05:06"), timezone],
            RuntimeSourceSpan::default(),
        )
        .expect("date_create succeeds");
        let Value::Object(datetime) = result else {
            panic!("expected DateTime object");
        };

        assert_eq!(datetime.display_name(), "DateTime");
        assert_eq!(
            crate::datetime::object_timezone(&datetime),
            Some("Europe/Berlin".to_owned())
        );
        assert!(crate::datetime::object_timestamp(&datetime).is_some());
    }

    #[test]
    fn date_create_immutable_from_format_returns_datetimeimmutable_for_wp_datetime() {
        let timezone = datetime::datetimezone_object("UTC").expect("timezone object");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let result = builtin_date_create_immutable_from_format(
            &mut context,
            vec![
                Value::string("Y-m-d H:i:s"),
                Value::string("2024-02-03 04:05:06"),
                timezone,
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("date_create_immutable_from_format succeeds");
        let Value::Object(datetime) = result else {
            panic!("expected DateTimeImmutable object");
        };

        assert_eq!(datetime.display_name(), "DateTimeImmutable");
        assert_eq!(
            crate::datetime::object_timezone(&datetime),
            Some("UTC".to_owned())
        );
        assert_eq!(
            crate::datetime::object_timestamp(&datetime),
            Some(1_706_933_106)
        );
    }

    #[test]
    fn date_diff_returns_datetimeinterval_for_datetimeinterface_objects() {
        let Value::Object(left) = datetime::datetime_object(1_603_238_400, "UTC") else {
            panic!("expected DateTime object");
        };
        let Value::Object(right) = datetime::datetime_immutable_object(1_603_929_600, "UTC") else {
            panic!("expected DateTimeImmutable object");
        };
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let result = builtin_date_diff(
            &mut context,
            vec![Value::Object(left), Value::Object(right)],
            RuntimeSourceSpan::default(),
        )
        .expect("date_diff succeeds");
        let Value::Object(interval) = result else {
            panic!("expected DateInterval object");
        };

        assert_eq!(interval.get_property("days"), Some(Value::Int(8)));
        assert_eq!(interval.get_property("d"), Some(Value::Int(8)));
        assert_eq!(interval.get_property("invert"), Some(Value::Int(0)));
    }
}
