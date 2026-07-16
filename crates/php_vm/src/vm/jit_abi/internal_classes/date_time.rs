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

fn timezone_from_value(value: &Value) -> Result<String, String> {
    let object = object_argument("DateTimeZone", value)?;
    if normalize_class_name(&object.class_name()) != "datetimezone" {
        return Err(format!(
            "E_PHP_VM_DATETIMEZONE_ARG_TYPE: expected DateTimeZone, {} given",
            object.class_name()
        ));
    }
    php_runtime::api::datetime::object_timezone(&object)
        .ok_or_else(|| "E_PHP_VM_DATETIMEZONE_INVALID: object has no timezone name".to_owned())
}

fn interval_seconds(value: &Value) -> Result<i64, String> {
    let object = object_argument("DateInterval", value)?;
    if normalize_class_name(&object.class_name()) != "dateinterval" {
        return Err(format!(
            "E_PHP_VM_DATEINTERVAL_ARG_TYPE: expected DateInterval, {} given",
            object.class_name()
        ));
    }
    match object.get_property("__seconds") {
        Some(Value::Int(seconds)) => Ok(seconds),
        _ => Err("E_PHP_VM_DATEINTERVAL_INVALID: object has no seconds".to_owned()),
    }
}

pub(in crate::vm::jit_abi) fn construct_native_date_time(
    context: &mut NativeExecutionContext<'_>,
    class_name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let display_name = normalized_date_class(class_name)?;
    let result = decode_arguments(context, arguments)
        .and_then(
            |arguments| match normalize_class_name(class_name).as_str() {
                "datetime" | "datetimeimmutable" => {
                    expect_arity(
                        &format!("{display_name}::__construct"),
                        arguments.len(),
                        0,
                        2,
                    )?;
                    let text = arguments
                        .first()
                        .cloned()
                        .map(|value| string_argument(display_name, value))
                        .transpose()?
                        .unwrap_or_else(|| "now".to_owned());
                    let timezone = arguments
                        .get(1)
                        .map(timezone_from_value)
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
                    object_from_value(if display_name == "DateTimeImmutable" {
                        php_runtime::api::datetime::datetime_immutable_object(timestamp, &timezone)
                    } else {
                        php_runtime::api::datetime::datetime_object(timestamp, &timezone)
                    })
                }
                "datetimezone" => {
                    expect_arity("DateTimeZone::__construct", arguments.len(), 1, 1)?;
                    let timezone =
                        string_argument("DateTimeZone::__construct", arguments[0].clone())?;
                    php_runtime::api::datetime::datetimezone_object(&timezone)
                    .ok_or_else(|| {
                        format!(
                            "E_PHP_VM_DATETIMEZONE_INVALID: timezone {timezone:?} is unsupported"
                        )
                    })
                    .and_then(object_from_value)
                }
                "dateinterval" => {
                    expect_arity("DateInterval::__construct", arguments.len(), 1, 1)?;
                    let spec = string_argument("DateInterval::__construct", arguments[0].clone())?;
                    let seconds = php_runtime::api::datetime::parse_interval_spec(&spec)
                        .ok_or_else(|| {
                            format!(
                                "E_PHP_VM_DATEINTERVAL_PARSE: interval spec {spec:?} is unsupported"
                            )
                        })?;
                    object_from_value(php_runtime::api::datetime::dateinterval_object(seconds))
                }
                _ => unreachable!(),
            },
        )
        .and_then(|object| context.encode(Value::Object(object)));
    Some(result)
}

fn call_date_time_method(
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: Vec<Value>,
) -> Result<Value, String> {
    let class_name = object.class_name();
    let immutable = normalize_class_name(&class_name) == "datetimeimmutable";
    match method.to_ascii_lowercase().as_str() {
        "format" => {
            expect_arity(&format!("{class_name}::format"), arguments.len(), 1, 1)?;
            let format = string_argument(&format!("{class_name}::format"), arguments[0].clone())?;
            let timestamp = php_runtime::api::datetime::object_timestamp(object).unwrap_or(0);
            let timezone = php_runtime::api::datetime::object_timezone(object)
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
            Ok(Value::string(php_runtime::api::datetime::format_timestamp(
                timestamp, &timezone, &format,
            )))
        }
        "gettimestamp" => {
            expect_arity(
                &format!("{class_name}::getTimestamp"),
                arguments.len(),
                0,
                0,
            )?;
            Ok(Value::Int(
                php_runtime::api::datetime::object_timestamp(object).unwrap_or(0),
            ))
        }
        "gettimezone" => {
            expect_arity(&format!("{class_name}::getTimezone"), arguments.len(), 0, 0)?;
            let timezone = php_runtime::api::datetime::object_timezone(object)
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
            Ok(php_runtime::api::datetime::datetimezone_object(&timezone)
                .unwrap_or(Value::Bool(false)))
        }
        "getoffset" => {
            expect_arity(&format!("{class_name}::getOffset"), arguments.len(), 0, 0)?;
            let timezone = php_runtime::api::datetime::object_timezone(object)
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
            Ok(Value::Int(
                php_runtime::api::datetime::timezone_offset_seconds(&timezone),
            ))
        }
        "settimezone" => {
            expect_arity(&format!("{class_name}::setTimezone"), arguments.len(), 1, 1)?;
            let timezone = timezone_from_value(&arguments[0])?;
            php_runtime::api::datetime::with_timezone(object, &timezone, immutable).ok_or_else(
                || format!("E_PHP_VM_DATETIMEZONE_INVALID: timezone {timezone:?} is unsupported"),
            )
        }
        "add" | "sub" => {
            expect_arity(&format!("{class_name}::{method}"), arguments.len(), 1, 1)?;
            let mut seconds = interval_seconds(&arguments[0])?;
            if method.eq_ignore_ascii_case("sub") {
                seconds = seconds.saturating_neg();
            }
            Ok(php_runtime::api::datetime::add_interval(
                object, seconds, immutable,
            ))
        }
        "modify" => {
            expect_arity(&format!("{class_name}::modify"), arguments.len(), 1, 1)?;
            let modifier = string_argument(&format!("{class_name}::modify"), arguments[0].clone())?;
            Ok(
                php_runtime::api::datetime::modify_object(object, &modifier, immutable)
                    .unwrap_or(Value::Bool(false)),
            )
        }
        "diff" => {
            expect_arity(&format!("{class_name}::diff"), arguments.len(), 1, 1)?;
            let right = object_argument(&format!("{class_name}::diff"), &arguments[0])?;
            if !matches!(
                normalize_class_name(&right.class_name()).as_str(),
                "datetime" | "datetimeimmutable"
            ) {
                return Err(format!(
                    "E_PHP_VM_DATETIME_ARG_TYPE: {class_name}::diff expects DateTimeInterface"
                ));
            }
            Ok(php_runtime::api::datetime::diff_objects(object, &right))
        }
        method => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not implemented"
        )),
    }
}

fn call_timezone_method(
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: Vec<Value>,
) -> Result<Value, String> {
    match method.to_ascii_lowercase().as_str() {
        "getname" => {
            expect_arity("DateTimeZone::getName", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::datetime::object_timezone(object)
                .map_or(Value::Bool(false), Value::string))
        }
        "getoffset" => {
            expect_arity("DateTimeZone::getOffset", arguments.len(), 1, 1)?;
            let datetime = object_argument("DateTimeZone::getOffset", &arguments[0])?;
            if !matches!(
                normalize_class_name(&datetime.class_name()).as_str(),
                "datetime" | "datetimeimmutable"
            ) {
                return Err("E_PHP_VM_DATETIMEZONE_ARG_TYPE: expected DateTimeInterface".to_owned());
            }
            let timezone = php_runtime::api::datetime::object_timezone(object)
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned());
            Ok(Value::Int(
                php_runtime::api::datetime::timezone_offset_seconds(&timezone),
            ))
        }
        method => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method DateTimeZone::{method} is not implemented"
        )),
    }
}

fn call_interval_method(
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: Vec<Value>,
) -> Result<Value, String> {
    match method.to_ascii_lowercase().as_str() {
        "format" => {
            expect_arity("DateInterval::format", arguments.len(), 1, 1)?;
            let format = string_argument("DateInterval::format", arguments[0].clone())?;
            Ok(Value::string(php_runtime::api::datetime::format_interval(
                interval_seconds(&Value::Object(object.clone()))?,
                &format,
            )))
        }
        method => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method DateInterval::{method} is not implemented"
        )),
    }
}

pub(in crate::vm::jit_abi) fn execute_native_date_time_instruction(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    match &instruction.kind {
        php_ir::InstructionKind::NewObject {
            display_class_name, ..
        } => construct_native_date_time(context, display_class_name, arguments),
        php_ir::InstructionKind::CallMethod { method, .. } => {
            let receiver = arguments.first().copied()?;
            let receiver = match context.decode(receiver) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let Value::Object(object) = receiver else {
                return None;
            };
            let class = normalize_class_name(&object.class_name());
            if !matches!(
                class.as_str(),
                "datetime" | "datetimeimmutable" | "datetimezone" | "dateinterval"
            ) {
                return None;
            }
            let result = decode_arguments(context, &arguments[1..])
                .and_then(|arguments| match class.as_str() {
                    "datetime" | "datetimeimmutable" => {
                        call_date_time_method(&object, method, arguments)
                    }
                    "datetimezone" => call_timezone_method(&object, method, arguments),
                    "dateinterval" => call_interval_method(&object, method, arguments),
                    _ => unreachable!(),
                })
                .and_then(|value| context.encode(value));
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
