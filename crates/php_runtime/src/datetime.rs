//! standard-library Date/Time helpers.

use crate::{ClassEntry, ClassFlags, ObjectRef, Value, display_class_name, normalize_class_name};
use std::time::{SystemTime, UNIX_EPOCH};

/// Deterministic standard-library timezone identifiers.
pub const TIMEZONE_IDENTIFIERS: &[&str] = &[
    "Africa/Johannesburg",
    "America/Chicago",
    "America/Los_Angeles",
    "America/New_York",
    "Asia/Tokyo",
    "Australia/Sydney",
    "Europe/Berlin",
    "Europe/London",
    "UTC",
];

/// Default timezone used when no request-local override is set.
pub const DEFAULT_TIMEZONE: &str = "UTC";

/// Returns true when the identifier is in the deterministic standard-library registry.
#[must_use]
pub fn is_valid_timezone(identifier: &str) -> bool {
    TIMEZONE_IDENTIFIERS.contains(&identifier)
}

/// Creates a `DateTime` runtime object.
#[must_use]
pub fn datetime_object(timestamp: i64, timezone: &str) -> Value {
    date_object("DateTime", timestamp, timezone)
}

/// Creates a `DateTimeImmutable` runtime object.
#[must_use]
pub fn datetime_immutable_object(timestamp: i64, timezone: &str) -> Value {
    date_object("DateTimeImmutable", timestamp, timezone)
}

/// Creates a `DateInterval` runtime object backed by signed seconds.
#[must_use]
pub fn dateinterval_object(seconds: i64) -> Value {
    let object =
        ObjectRef::new_with_display_name(&date_class("DateInterval", false), "DateInterval");
    let parts = interval_parts(seconds);
    object.set_property("__seconds", Value::Int(seconds));
    object.set_property("invert", Value::Int(i64::from(seconds < 0)));
    object.set_property("days", Value::Int(parts.total_days));
    object.set_property("y", Value::Int(parts.years));
    object.set_property("m", Value::Int(parts.months));
    object.set_property("d", Value::Int(parts.days));
    object.set_property("h", Value::Int(parts.hours));
    object.set_property("i", Value::Int(parts.minutes));
    object.set_property("s", Value::Int(parts.seconds));
    Value::Object(object)
}

/// Creates a `DateTimeZone` runtime object for a supported identifier.
#[must_use]
pub fn datetimezone_object(timezone: &str) -> Option<Value> {
    if !is_valid_timezone(timezone) {
        return None;
    }
    let object =
        ObjectRef::new_with_display_name(&date_class("DateTimeZone", false), "DateTimeZone");
    object.set_property("timezone", Value::string(timezone));
    Some(Value::Object(object))
}

/// Parses an ISO/common package-facing date string to a UTC timestamp.
pub fn parse_datetime_text(text: &str, base_timestamp: i64) -> Option<i64> {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("now") {
        return Some(base_timestamp);
    }
    if let Some(seconds) = parse_relative_modifier(trimmed) {
        return Some(base_timestamp.saturating_add(seconds));
    }
    parse_absolute_datetime(trimmed)
}

/// Parses DateTime constructor input using a default timezone for local absolute text.
pub fn parse_datetime_text_in_timezone(
    text: &str,
    base_timestamp: i64,
    timezone: &str,
) -> Option<i64> {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("now") {
        return Some(base_timestamp);
    }
    if let Some(seconds) = parse_relative_modifier(trimmed) {
        return Some(base_timestamp.saturating_add(seconds));
    }
    let timestamp = parse_absolute_datetime(trimmed)?;
    if absolute_text_has_explicit_timezone(trimmed) {
        Some(timestamp)
    } else {
        Some(timestamp.saturating_sub(timezone_offset_seconds(timezone)))
    }
}

/// Parses a DateInterval MVP specification.
pub fn parse_interval_spec(spec: &str) -> Option<i64> {
    let bytes = spec.as_bytes();
    if bytes.first().copied() != Some(b'P') {
        return None;
    }
    let mut index = 1usize;
    let mut in_time = false;
    let mut total = 0i64;
    while index < bytes.len() {
        if bytes[index] == b'T' {
            in_time = true;
            index += 1;
            continue;
        }
        let start = index;
        while bytes
            .get(index)
            .copied()
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            index += 1;
        }
        if index == start || index >= bytes.len() {
            return None;
        }
        let value = std::str::from_utf8(&bytes[start..index])
            .ok()?
            .parse::<i64>()
            .ok()?;
        let unit = bytes[index];
        index += 1;
        let multiplier = match (in_time, unit) {
            (false, b'D') => 86_400,
            (false, b'M') => 30 * 86_400,
            (false, b'Y') => 365 * 86_400,
            (true, b'H') => 3_600,
            (true, b'M') => 60,
            (true, b'S') => 1,
            _ => return None,
        };
        total = total.saturating_add(value.saturating_mul(multiplier));
    }
    Some(total)
}

/// Formats a DateInterval-like seconds payload with a small PHP interval format subset.
#[must_use]
pub fn format_interval(seconds: i64, format: &str) -> String {
    let parts = interval_parts(seconds);
    let mut output = String::new();
    let mut percent = false;
    for marker in format.chars() {
        if percent {
            match marker {
                '%' => output.push('%'),
                'R' => output.push(if seconds < 0 { '-' } else { '+' }),
                'r' => {
                    if seconds < 0 {
                        output.push('-');
                    }
                }
                'y' => output.push_str(&parts.years.to_string()),
                'm' => output.push_str(&parts.months.to_string()),
                'd' => output.push_str(&parts.days.to_string()),
                'a' => output.push_str(&parts.total_days.to_string()),
                'h' => output.push_str(&parts.hours.to_string()),
                'i' => output.push_str(&parts.minutes.to_string()),
                's' => output.push_str(&parts.seconds.to_string()),
                other => {
                    output.push('%');
                    output.push(other);
                }
            }
            percent = false;
            continue;
        }
        if marker == '%' {
            percent = true;
        } else {
            output.push(marker);
        }
    }
    if percent {
        output.push('%');
    }
    output
}

/// Returns the current Unix timestamp.
#[must_use]
pub fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// Formats a timestamp with a PHP-date-format MVP.
#[must_use]
pub fn format_timestamp(timestamp: i64, timezone: &str, format: &str) -> String {
    let offset = timezone_offset_seconds(timezone);
    let parts = timestamp_to_parts(timestamp.saturating_add(offset));
    let mut output = String::new();
    let mut escaped = false;
    for marker in format.chars() {
        if escaped {
            output.push(marker);
            escaped = false;
            continue;
        }
        if marker == '\\' {
            escaped = true;
            continue;
        }
        match marker {
            'Y' => output.push_str(&format!("{:04}", parts.year)),
            'y' => output.push_str(&format!("{:02}", parts.year.rem_euclid(100))),
            'm' => output.push_str(&format!("{:02}", parts.month)),
            'n' => output.push_str(&parts.month.to_string()),
            'd' => output.push_str(&format!("{:02}", parts.day)),
            'j' => output.push_str(&parts.day.to_string()),
            'H' => output.push_str(&format!("{:02}", parts.hour)),
            'G' => output.push_str(&parts.hour.to_string()),
            'i' => output.push_str(&format!("{:02}", parts.minute)),
            's' => output.push_str(&format!("{:02}", parts.second)),
            'U' => output.push_str(&timestamp.to_string()),
            'c' => output.push_str(&format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}",
                parts.year,
                parts.month,
                parts.day,
                parts.hour,
                parts.minute,
                parts.second,
                timezone_offset_text(offset)
            )),
            'O' => output.push_str(&timezone_offset_text(offset).replace(':', "")),
            'P' => output.push_str(timezone_offset_text(offset)),
            'T' => output.push_str(timezone_abbreviation(timezone)),
            _ => output.push(marker),
        }
    }
    output
}

/// Reads a timestamp from a runtime DateTime-like object.
#[must_use]
pub fn object_timestamp(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__timestamp") {
        Some(Value::Int(value)) => Some(value),
        _ => None,
    }
}

/// Reads a timezone from a runtime DateTime-like object.
#[must_use]
pub fn object_timezone(object: &ObjectRef) -> Option<String> {
    match object.get_property("timezone") {
        Some(Value::String(value)) => Some(value.to_string_lossy()),
        _ => None,
    }
}

/// Returns a DateTime-like object with a changed timestamp.
#[must_use]
pub fn with_timestamp(object: &ObjectRef, timestamp: i64, immutable: bool) -> Value {
    if immutable {
        date_object(
            &object.display_name(),
            timestamp,
            &object_timezone_or_utc(object),
        )
    } else {
        object.set_property("__timestamp", Value::Int(timestamp));
        Value::Object(object.clone())
    }
}

/// Returns a DateTime-like object with a changed timezone.
#[must_use]
pub fn with_timezone(object: &ObjectRef, timezone: &str, immutable: bool) -> Option<Value> {
    if !is_valid_timezone(timezone) {
        return None;
    }
    if immutable {
        Some(date_object(
            &object.display_name(),
            object_timestamp(object).unwrap_or(0),
            timezone,
        ))
    } else {
        object.set_property("timezone", Value::string(timezone));
        Some(Value::Object(object.clone()))
    }
}

/// Applies a DateInterval-like seconds delta.
#[must_use]
pub fn add_interval(object: &ObjectRef, seconds: i64, immutable: bool) -> Value {
    let timestamp = object_timestamp(object)
        .unwrap_or(0)
        .saturating_add(seconds);
    with_timestamp(object, timestamp, immutable)
}

/// Parses and applies a simple modify expression.
pub fn modify_object(object: &ObjectRef, modifier: &str, immutable: bool) -> Option<Value> {
    let base = object_timestamp(object).unwrap_or(0);
    parse_datetime_text(modifier, base)
        .map(|timestamp| with_timestamp(object, timestamp, immutable))
}

/// Computes a DateInterval object representing `right - left`.
#[must_use]
pub fn diff_objects(left: &ObjectRef, right: &ObjectRef) -> Value {
    dateinterval_object(
        object_timestamp(right)
            .unwrap_or(0)
            .saturating_sub(object_timestamp(left).unwrap_or(0)),
    )
}

fn date_object(class_name: &str, timestamp: i64, timezone: &str) -> Value {
    let timezone = if is_valid_timezone(timezone) {
        timezone
    } else {
        DEFAULT_TIMEZONE
    };
    let object = ObjectRef::new_with_display_name(
        &date_class(class_name, false),
        display_class_name(class_name),
    );
    object.set_property("__timestamp", Value::Int(timestamp));
    object.set_property("timezone", Value::string(timezone));
    Value::Object(object)
}

fn date_class(name: &str, is_interface: bool) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: ClassFlags {
            is_interface,
            ..ClassFlags::default()
        },
    }
}

fn parse_absolute_datetime(text: &str) -> Option<i64> {
    if let Some(timestamp) = text.strip_prefix('@') {
        return timestamp.parse::<i64>().ok();
    }
    if let Ok(timestamp) = text.parse::<i64>() {
        return Some(timestamp);
    }

    let normalized = strip_supported_timezone_suffix(text)
        .trim_end_matches('Z')
        .replace('T', " ");
    let (date, time) = normalized
        .split_once(' ')
        .map_or((normalized.as_str(), "00:00:00"), |(date, time)| {
            (date, time)
        });
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u8>().ok()?;
    let day = date_parts.next()?.parse::<u8>().ok()?;
    if date_parts.next().is_some() {
        return None;
    }
    let time = time
        .split_once(['+', '-'])
        .map_or(time, |(clock, _)| clock)
        .trim();
    let mut time_parts = time.split(':');
    let hour = time_parts.next().unwrap_or("0").parse::<u8>().ok()?;
    let minute = time_parts.next().unwrap_or("0").parse::<u8>().ok()?;
    let second = time_parts.next().unwrap_or("0").parse::<u8>().ok()?;
    Some(parts_to_timestamp(year, month, day, hour, minute, second))
}

fn absolute_text_has_explicit_timezone(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.starts_with('@') {
        return true;
    }
    if trimmed.ends_with('Z') {
        return true;
    }
    if has_supported_timezone_suffix(trimmed) {
        return true;
    }
    let Some((_, time)) = trimmed.split_once([' ', 'T']) else {
        return false;
    };
    time.contains('+') || time.contains('-')
}

fn strip_supported_timezone_suffix(text: &str) -> &str {
    let trimmed = text.trim();
    for suffix in [" GMT", " UTC"] {
        if trimmed.len() > suffix.len()
            && trimmed[trimmed.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
        {
            return trimmed[..trimmed.len() - suffix.len()].trim_end();
        }
    }
    trimmed
}

fn has_supported_timezone_suffix(text: &str) -> bool {
    let trimmed = text.trim();
    [" GMT", " UTC"].iter().any(|suffix| {
        trimmed.len() > suffix.len()
            && trimmed[trimmed.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
    })
}

fn parse_relative_modifier(text: &str) -> Option<i64> {
    match text.to_ascii_lowercase().as_str() {
        "tomorrow" => return Some(86_400),
        "yesterday" => return Some(-86_400),
        "next day" => return Some(86_400),
        _ => {}
    }
    let mut parts = text.split_whitespace();
    let amount = parts.next()?;
    let unit = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let sign = if let Some(stripped) = amount.strip_prefix('+') {
        (1, stripped)
    } else if let Some(stripped) = amount.strip_prefix('-') {
        (-1, stripped)
    } else {
        return None;
    };
    let value = sign.1.parse::<i64>().ok()?.saturating_mul(sign.0);
    let multiplier = match unit.trim_end_matches('s').to_ascii_lowercase().as_str() {
        "second" => 1,
        "minute" => 60,
        "hour" => 3_600,
        "day" => 86_400,
        "week" => 7 * 86_400,
        "month" => 30 * 86_400,
        "year" => 365 * 86_400,
        _ => return None,
    };
    Some(value.saturating_mul(multiplier))
}

#[derive(Clone, Copy)]
struct IntervalParts {
    total_days: i64,
    years: i64,
    months: i64,
    days: i64,
    hours: i64,
    minutes: i64,
    seconds: i64,
}

fn interval_parts(seconds: i64) -> IntervalParts {
    let mut remaining = seconds.abs();
    let total_days = remaining / 86_400;
    let years = remaining / (365 * 86_400);
    remaining %= 365 * 86_400;
    let months = remaining / (30 * 86_400);
    remaining %= 30 * 86_400;
    let days = remaining / 86_400;
    remaining %= 86_400;
    let hours = remaining / 3_600;
    remaining %= 3_600;
    let minutes = remaining / 60;
    let seconds = remaining % 60;
    IntervalParts {
        total_days,
        years,
        months,
        days,
        hours,
        minutes,
        seconds,
    }
}

fn object_timezone_or_utc(object: &ObjectRef) -> String {
    object_timezone(object).unwrap_or_else(|| DEFAULT_TIMEZONE.to_string())
}

fn timezone_offset_seconds(timezone: &str) -> i64 {
    match timezone {
        "Europe/Berlin" => 3_600,
        "Europe/London" | "UTC" | "GMT" => 0,
        "Africa/Johannesburg" => 7_200,
        "America/New_York" => -18_000,
        "America/Chicago" => -21_600,
        "America/Los_Angeles" => -28_800,
        "Asia/Tokyo" => 32_400,
        "Australia/Sydney" => 36_000,
        _ => 0,
    }
}

fn timezone_offset_text(offset: i64) -> &'static str {
    match offset {
        3_600 => "+01:00",
        7_200 => "+02:00",
        -18_000 => "-05:00",
        -21_600 => "-06:00",
        -28_800 => "-08:00",
        32_400 => "+09:00",
        36_000 => "+10:00",
        _ => "+00:00",
    }
}

fn timezone_abbreviation(timezone: &str) -> &'static str {
    match timezone {
        "Europe/Berlin" => "CET",
        "GMT" => "GMT",
        "Europe/London" | "UTC" => "UTC",
        "Africa/Johannesburg" => "SAST",
        "America/New_York" => "EST",
        "America/Chicago" => "CST",
        "America/Los_Angeles" => "PST",
        "Asia/Tokyo" => "JST",
        "Australia/Sydney" => "AEST",
        _ => "UTC",
    }
}

#[derive(Clone, Copy)]
struct DateParts {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

fn timestamp_to_parts(timestamp: i64) -> DateParts {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    DateParts {
        year,
        month,
        day,
        hour: (seconds / 3_600) as u8,
        minute: ((seconds % 3_600) / 60) as u8,
        second: (seconds % 60) as u8,
    }
}

fn parts_to_timestamp(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> i64 {
    days_from_civil(year, month, day)
        .saturating_mul(86_400)
        .saturating_add((hour as i64) * 3_600)
        .saturating_add((minute as i64) * 60)
        .saturating_add(second as i64)
}

fn civil_from_days(days: i64) -> (i32, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(m <= 2);
    (year as i32, m as u8, d as u8)
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = i64::from(month);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
