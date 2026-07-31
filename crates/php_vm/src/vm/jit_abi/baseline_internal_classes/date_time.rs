use super::*;

fn normalized_date_class(class_name: &str) -> Option<&'static str> {
    match normalize_class_name(class_name).as_str() {
        "datetime" => Some("DateTime"),
        "datetimeimmutable" => Some("DateTimeImmutable"),
        "datetimezone" => Some("DateTimeZone"),
        "dateinterval" => Some("DateInterval"),
        _ => None,
    }
}

fn object_from_value(value: Value) -> Result<php_runtime::api::ObjectRef, String> {
    match value {
        Value::Object(object) => Ok(object),
        _ => Err("E_PHP_VM_DATETIME_OBJECT: date helper did not create an object".to_owned()),
    }
}

fn encoded_string_argument(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    encoded: i64,
) -> Result<String, String> {
    let encoded = context.dereference_direct_encoding(encoded);
    if let Some(bytes) = context.native_string_bytes(encoded) {
        return Ok(String::from_utf8_lossy(bytes).into_owned());
    }
    let value = context.decode_baseline_value(encoded)?;
    let value = match value {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    string_argument(name, value)
}

fn encoded_object(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    encoded: i64,
) -> Result<php_runtime::api::ObjectRef, String> {
    if let Some(object) = context.native_query_object(encoded) {
        return Ok(object);
    }
    let value = context.decode_baseline_value(encoded)?;
    let value = match value {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    object_argument(name, &value)
}

fn object_timestamp(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
    object: &php_runtime::api::ObjectRef,
) -> Option<i64> {
    context
        .native_object_property_value(encoded, "__timestamp")
        .and_then(|value| context.native_encoded_int(value))
        .or_else(|| php_runtime::api::datetime::object_timestamp(object))
}

fn object_timezone(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
    object: &php_runtime::api::ObjectRef,
) -> Option<String> {
    context
        .native_object_property_value(encoded, "timezone")
        .and_then(|value| context.native_string_name_bytes(value))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .or_else(|| php_runtime::api::datetime::object_timezone(object))
}

fn timezone_from_encoded(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
) -> Result<String, String> {
    let object = encoded_object(context, "DateTimeZone", encoded)?;
    if normalize_class_name(&object.class_name()) != "datetimezone" {
        return Err(format!(
            "E_PHP_VM_DATETIMEZONE_ARG_TYPE: expected DateTimeZone, {} given",
            object.class_name()
        ));
    }
    object_timezone(context, encoded, &object)
        .ok_or_else(|| "E_PHP_VM_DATETIMEZONE_INVALID: object has no timezone name".to_owned())
}

fn interval_seconds(context: &mut NativeRequestColdState<'_>, encoded: i64) -> Result<i64, String> {
    let object = encoded_object(context, "DateInterval", encoded)?;
    if normalize_class_name(&object.class_name()) != "dateinterval" {
        return Err(format!(
            "E_PHP_VM_DATEINTERVAL_ARG_TYPE: expected DateInterval, {} given",
            object.class_name()
        ));
    }
    context
        .native_object_property_value(encoded, "__seconds")
        .and_then(|value| context.native_encoded_int(value))
        .or_else(|| match object.get_property("__seconds") {
            Some(Value::Int(seconds)) => Some(seconds),
            _ => None,
        })
        .ok_or_else(|| "E_PHP_VM_DATEINTERVAL_INVALID: object has no seconds".to_owned())
}

fn encode_object_value(
    context: &mut NativeRequestColdState<'_>,
    value: Value,
) -> Result<i64, String> {
    let object = object_from_value(value)?;
    context.encode_baseline_value(Value::Object(object))
}

fn encode_datetime(
    context: &mut NativeRequestColdState<'_>,
    immutable: bool,
    timestamp: i64,
    timezone: &str,
) -> Result<i64, String> {
    encode_object_value(
        context,
        if immutable {
            php_runtime::api::datetime::datetime_immutable_object(timestamp, timezone)
        } else {
            php_runtime::api::datetime::datetime_object(timestamp, timezone)
        },
    )
}

fn encode_string(context: &mut NativeRequestColdState<'_>, value: String) -> Result<i64, String> {
    context.encode_native_string_owner(PhpString::from_bytes(value.into_bytes()))
}

fn retain_receiver(
    context: &mut NativeRequestColdState<'_>,
    receiver: i64,
    object: &php_runtime::api::ObjectRef,
) -> Result<i64, String> {
    if let Some(receiver) = context.duplicate_authoritative_native_value(receiver)? {
        return Ok(receiver);
    }
    context.encode_baseline_value(Value::Object(object.clone()))
}

fn replace_internal_property(
    context: &mut NativeRequestColdState<'_>,
    receiver: i64,
    property: &str,
    value: i64,
) -> Result<(), String> {
    if context.replace_native_object_property_owned(receiver, property, value)? {
        return Ok(());
    }
    Err(format!(
        "E_PHP_VM_DATETIME_NATIVE_STATE: {property} is not an authoritative native property"
    ))
}

pub(in crate::vm::jit_abi) fn construct_native_date_time(
    context: &mut NativeRequestColdState<'_>,
    class_name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let display_name = normalized_date_class(class_name)?;
    let result = (|| match normalize_class_name(class_name).as_str() {
        "datetime" | "datetimeimmutable" => {
            expect_arity(
                &format!("{display_name}::__construct"),
                arguments.len(),
                0,
                2,
            )?;
            let text = arguments
                .first()
                .copied()
                .map(|value| encoded_string_argument(context, display_name, value))
                .transpose()?
                .unwrap_or_else(|| "now".to_owned());
            let timezone = arguments
                .get(1)
                .copied()
                .map(|value| timezone_from_encoded(context, value))
                .transpose()?
                .unwrap_or_else(|| context.default_timezone.clone());
            let timestamp = php_runtime::api::datetime::parse_datetime_text_in_timezone(
                &text,
                php_runtime::api::datetime::current_timestamp(),
                &timezone,
            )
            .ok_or_else(|| {
                format!("E_PHP_VM_DATETIME_PARSE: could not parse DateTime text {text:?}")
            })?;
            encode_datetime(
                context,
                display_name == "DateTimeImmutable",
                timestamp,
                &timezone,
            )
        }
        "datetimezone" => {
            expect_arity("DateTimeZone::__construct", arguments.len(), 1, 1)?;
            let timezone =
                encoded_string_argument(context, "DateTimeZone::__construct", arguments[0])?;
            let object =
                php_runtime::api::datetime::datetimezone_object(&timezone).ok_or_else(|| {
                    format!("E_PHP_VM_DATETIMEZONE_INVALID: timezone {timezone:?} is unsupported")
                })?;
            encode_object_value(context, object)
        }
        "dateinterval" => {
            expect_arity("DateInterval::__construct", arguments.len(), 1, 1)?;
            let spec = encoded_string_argument(context, "DateInterval::__construct", arguments[0])?;
            let seconds =
                php_runtime::api::datetime::parse_interval_spec(&spec).ok_or_else(|| {
                    format!("E_PHP_VM_DATEINTERVAL_PARSE: interval spec {spec:?} is unsupported")
                })?;
            encode_object_value(
                context,
                php_runtime::api::datetime::dateinterval_object(seconds),
            )
        }
        _ => unreachable!(),
    })();
    Some(result)
}

fn call_date_time_method(
    context: &mut NativeRequestColdState<'_>,
    receiver: i64,
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: &[i64],
) -> Result<i64, String> {
    let class_name = object.class_name();
    let immutable = normalize_class_name(&class_name) == "datetimeimmutable";
    match method.to_ascii_lowercase().as_str() {
        "format" => {
            expect_arity(&format!("{class_name}::format"), arguments.len(), 1, 1)?;
            let format =
                encoded_string_argument(context, &format!("{class_name}::format"), arguments[0])?;
            let timestamp = object_timestamp(context, receiver, object).unwrap_or(0);
            let timezone = object_timezone(context, receiver, object)
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
            encode_string(
                context,
                php_runtime::api::datetime::format_timestamp(timestamp, &timezone, &format),
            )
        }
        "gettimestamp" => {
            expect_arity(
                &format!("{class_name}::getTimestamp"),
                arguments.len(),
                0,
                0,
            )?;
            let timestamp = object_timestamp(context, receiver, object).unwrap_or(0);
            context.encode_native_int(timestamp)
        }
        "gettimezone" => {
            expect_arity(&format!("{class_name}::getTimezone"), arguments.len(), 0, 0)?;
            let timezone = object_timezone(context, receiver, object)
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
            match php_runtime::api::datetime::datetimezone_object(&timezone) {
                Some(object) => encode_object_value(context, object),
                None => context.encode_baseline_value(Value::Bool(false)),
            }
        }
        "getoffset" => {
            expect_arity(&format!("{class_name}::getOffset"), arguments.len(), 0, 0)?;
            let timezone = object_timezone(context, receiver, object)
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
            context.encode_native_int(php_runtime::api::datetime::timezone_offset_seconds(
                &timezone,
            ))
        }
        "settimezone" => {
            expect_arity(&format!("{class_name}::setTimezone"), arguments.len(), 1, 1)?;
            let timezone = timezone_from_encoded(context, arguments[0])?;
            let timezone = php_runtime::api::datetime::normalize_timezone_identifier(&timezone)
                .ok_or_else(|| {
                    format!("E_PHP_VM_DATETIMEZONE_INVALID: timezone {timezone:?} is unsupported")
                })?;
            if immutable {
                let timestamp = object_timestamp(context, receiver, object).unwrap_or(0);
                return encode_datetime(context, true, timestamp, &timezone);
            }
            let timezone_value =
                context.encode_native_string_owner(PhpString::from_bytes(timezone.into_bytes()))?;
            replace_internal_property(context, receiver, "timezone", timezone_value)?;
            retain_receiver(context, receiver, object)
        }
        "add" | "sub" => {
            expect_arity(&format!("{class_name}::{method}"), arguments.len(), 1, 1)?;
            let mut seconds = interval_seconds(context, arguments[0])?;
            if method.eq_ignore_ascii_case("sub") {
                seconds = seconds.saturating_neg();
            }
            let timestamp = object_timestamp(context, receiver, object)
                .unwrap_or(0)
                .saturating_add(seconds);
            if immutable {
                let timezone = object_timezone(context, receiver, object)
                    .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
                return encode_datetime(context, true, timestamp, &timezone);
            }
            let timestamp_value = context.encode_native_int(timestamp)?;
            replace_internal_property(context, receiver, "__timestamp", timestamp_value)?;
            retain_receiver(context, receiver, object)
        }
        "modify" => {
            expect_arity(&format!("{class_name}::modify"), arguments.len(), 1, 1)?;
            let modifier =
                encoded_string_argument(context, &format!("{class_name}::modify"), arguments[0])?;
            let Some(timestamp) = php_runtime::api::datetime::parse_datetime_text(
                &modifier,
                object_timestamp(context, receiver, object).unwrap_or(0),
            ) else {
                return context.encode_baseline_value(Value::Bool(false));
            };
            if immutable {
                let timezone = object_timezone(context, receiver, object)
                    .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
                return encode_datetime(context, true, timestamp, &timezone);
            }
            let timestamp_value = context.encode_native_int(timestamp)?;
            replace_internal_property(context, receiver, "__timestamp", timestamp_value)?;
            retain_receiver(context, receiver, object)
        }
        "diff" => {
            expect_arity(&format!("{class_name}::diff"), arguments.len(), 1, 1)?;
            let right = encoded_object(context, &format!("{class_name}::diff"), arguments[0])?;
            if !matches!(
                normalize_class_name(&right.class_name()).as_str(),
                "datetime" | "datetimeimmutable"
            ) {
                return Err(format!(
                    "E_PHP_VM_DATETIME_ARG_TYPE: {class_name}::diff expects DateTimeInterface"
                ));
            }
            let seconds = object_timestamp(context, arguments[0], &right)
                .unwrap_or(0)
                .saturating_sub(object_timestamp(context, receiver, object).unwrap_or(0));
            encode_object_value(
                context,
                php_runtime::api::datetime::dateinterval_object(seconds),
            )
        }
        method => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not implemented"
        )),
    }
}

fn call_timezone_method(
    context: &mut NativeRequestColdState<'_>,
    receiver: i64,
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: &[i64],
) -> Result<i64, String> {
    match method.to_ascii_lowercase().as_str() {
        "getname" => {
            expect_arity("DateTimeZone::getName", arguments.len(), 0, 0)?;
            match object_timezone(context, receiver, object) {
                Some(timezone) => encode_string(context, timezone),
                None => context.encode_baseline_value(Value::Bool(false)),
            }
        }
        "getoffset" => {
            expect_arity("DateTimeZone::getOffset", arguments.len(), 1, 1)?;
            let datetime = encoded_object(context, "DateTimeZone::getOffset", arguments[0])?;
            if !matches!(
                normalize_class_name(&datetime.class_name()).as_str(),
                "datetime" | "datetimeimmutable"
            ) {
                return Err("E_PHP_VM_DATETIMEZONE_ARG_TYPE: expected DateTimeInterface".to_owned());
            }
            let timezone = object_timezone(context, receiver, object)
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
            context.encode_native_int(php_runtime::api::datetime::timezone_offset_seconds(
                &timezone,
            ))
        }
        method => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method DateTimeZone::{method} is not implemented"
        )),
    }
}

fn call_interval_method(
    context: &mut NativeRequestColdState<'_>,
    receiver: i64,
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: &[i64],
) -> Result<i64, String> {
    match method.to_ascii_lowercase().as_str() {
        "format" => {
            expect_arity("DateInterval::format", arguments.len(), 1, 1)?;
            let format = encoded_string_argument(context, "DateInterval::format", arguments[0])?;
            let seconds = interval_seconds(context, receiver).or_else(|_| {
                match object.get_property("__seconds") {
                    Some(Value::Int(seconds)) => Ok(seconds),
                    _ => Err("E_PHP_VM_DATEINTERVAL_INVALID: object has no seconds".to_owned()),
                }
            })?;
            encode_string(
                context,
                php_runtime::api::datetime::format_interval(seconds, &format),
            )
        }
        method => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method DateInterval::{method} is not implemented"
        )),
    }
}

pub(in crate::vm::jit_abi) fn execute_native_date_time_instruction(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    match &instruction.kind {
        php_ir::InstructionKind::NewObject {
            display_class_name, ..
        } => construct_native_date_time(context, display_class_name, arguments),
        php_ir::InstructionKind::CallMethod { method, .. } => {
            let receiver = arguments.first().copied()?;
            let object = if let Some(object) = context.native_query_object(receiver) {
                object
            } else {
                let receiver = match context.decode_baseline_value(receiver) {
                    Ok(Value::Reference(reference)) => reference.get(),
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let Value::Object(object) = receiver else {
                    return None;
                };
                object
            };
            let class = normalize_class_name(&object.class_name());
            if !matches!(
                class.as_str(),
                "datetime" | "datetimeimmutable" | "datetimezone" | "dateinterval"
            ) {
                return None;
            }
            let result = match class.as_str() {
                "datetime" | "datetimeimmutable" => {
                    call_date_time_method(context, receiver, &object, method, &arguments[1..])
                }
                "datetimezone" => {
                    call_timezone_method(context, receiver, &object, method, &arguments[1..])
                }
                "dateinterval" => {
                    call_interval_method(context, receiver, &object, method, &arguments[1..])
                }
                _ => unreachable!(),
            };
            Some(result)
        }
        _ => None,
    }
}

pub(in crate::vm::jit_abi) fn date_time_instanceof(
    object_class: &str,
    target_class: &str,
) -> Option<bool> {
    normalized_date_class(object_class)?;
    let object_class = normalize_class_name(object_class);
    let target_class = normalize_class_name(target_class);
    Some(match target_class.as_str() {
        "datetimeinterface" => matches!(object_class.as_str(), "datetime" | "datetimeimmutable"),
        "datetime" | "datetimeimmutable" | "datetimezone" | "dateinterval" => {
            object_class == target_class
        }
        _ => false,
    })
}

pub(in crate::vm::jit_abi) fn date_time_class_constant(
    class_name: &str,
    constant: &str,
) -> Option<Value> {
    let class = normalize_class_name(class_name);
    if matches!(
        class.as_str(),
        "datetimeinterface" | "datetime" | "datetimeimmutable"
    ) {
        let value = match constant.to_ascii_uppercase().as_str() {
            "ATOM" => php_std::constants::DATE_ATOM,
            "COOKIE" => php_std::constants::DATE_COOKIE,
            "ISO8601" => php_std::constants::DATE_ISO8601,
            "ISO8601_EXPANDED" => php_std::constants::DATE_ISO8601_EXPANDED,
            "RFC822" => php_std::constants::DATE_RFC822,
            "RFC850" => php_std::constants::DATE_RFC850,
            "RFC1036" => php_std::constants::DATE_RFC1036,
            "RFC1123" => php_std::constants::DATE_RFC1123,
            "RFC7231" => php_std::constants::DATE_RFC7231,
            "RFC2822" => php_std::constants::DATE_RFC2822,
            "RFC3339" => php_std::constants::DATE_RFC3339,
            "RFC3339_EXTENDED" => php_std::constants::DATE_RFC3339_EXTENDED,
            "RSS" => php_std::constants::DATE_RSS,
            "W3C" => php_std::constants::DATE_W3C,
            _ => return None,
        };
        return Some(Value::string(value));
    }
    if class == "datetimezone" {
        let value = match constant.to_ascii_uppercase().as_str() {
            "AFRICA" => 1,
            "AMERICA" => 2,
            "ANTARCTICA" => 4,
            "ARCTIC" => 8,
            "ASIA" => 16,
            "ATLANTIC" => 32,
            "AUSTRALIA" => 64,
            "EUROPE" => 128,
            "INDIAN" => 256,
            "PACIFIC" => 512,
            "UTC" => 1024,
            "ALL" => 2047,
            "ALL_WITH_BC" => 4095,
            "PER_COUNTRY" => 4096,
            _ => return None,
        };
        return Some(Value::Int(value));
    }
    None
}
