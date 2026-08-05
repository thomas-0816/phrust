//! Immutable metadata for extension-provided internal classes.

use super::*;
use php_runtime::api::Value;

const MYSQL_ATTRIBUTES: &[(&str, &str, i64)] = &[
    (
        "MYSQL_ATTR_USE_BUFFERED_QUERY",
        "ATTR_USE_BUFFERED_QUERY",
        1000,
    ),
    ("MYSQL_ATTR_LOCAL_INFILE", "ATTR_LOCAL_INFILE", 1001),
    (
        "MYSQL_ATTR_LOCAL_INFILE_DIRECTORY",
        "ATTR_LOCAL_INFILE_DIRECTORY",
        1014,
    ),
    ("MYSQL_ATTR_INIT_COMMAND", "ATTR_INIT_COMMAND", 1002),
    ("MYSQL_ATTR_COMPRESS", "ATTR_COMPRESS", 1003),
    ("MYSQL_ATTR_DIRECT_QUERY", "ATTR_DIRECT_QUERY", 20),
    ("MYSQL_ATTR_FOUND_ROWS", "ATTR_FOUND_ROWS", 1004),
    ("MYSQL_ATTR_IGNORE_SPACE", "ATTR_IGNORE_SPACE", 1005),
    ("MYSQL_ATTR_SSL_KEY", "ATTR_SSL_KEY", 1006),
    ("MYSQL_ATTR_SSL_CERT", "ATTR_SSL_CERT", 1007),
    ("MYSQL_ATTR_SSL_CA", "ATTR_SSL_CA", 1008),
    ("MYSQL_ATTR_SSL_CAPATH", "ATTR_SSL_CAPATH", 1009),
    ("MYSQL_ATTR_SSL_CIPHER", "ATTR_SSL_CIPHER", 1010),
    (
        "MYSQL_ATTR_SSL_VERIFY_SERVER_CERT",
        "ATTR_SSL_VERIFY_SERVER_CERT",
        1013,
    ),
    (
        "MYSQL_ATTR_SERVER_PUBLIC_KEY",
        "ATTR_SERVER_PUBLIC_KEY",
        1011,
    ),
    ("MYSQL_ATTR_MULTI_STATEMENTS", "ATTR_MULTI_STATEMENTS", 1012),
];

pub(super) fn pdo_mysql_deprecated_constant(
    class_name: &str,
    constant: &str,
) -> Option<(&'static str, &'static str)> {
    class_name.eq_ignore_ascii_case("PDO").then_some(())?;
    MYSQL_ATTRIBUTES
        .iter()
        .find(|(legacy, _, _)| legacy.eq_ignore_ascii_case(constant))
        .map(|(legacy, modern, _)| (*legacy, *modern))
}

fn pdo_mysql_class_constant(class_name: &str, constant: &str) -> Option<Value> {
    let value = if class_name.eq_ignore_ascii_case("PDO") {
        MYSQL_ATTRIBUTES
            .iter()
            .find(|(legacy, _, _)| legacy.eq_ignore_ascii_case(constant))
    } else if class_name.eq_ignore_ascii_case("Pdo\\Mysql") {
        MYSQL_ATTRIBUTES
            .iter()
            .find(|(_, modern, _)| modern.eq_ignore_ascii_case(constant))
    } else {
        None
    }?;
    Some(Value::Int(value.2))
}

fn date_time_class_constant(class_name: &str, constant: &str) -> Option<Value> {
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

fn spl_iterator_class_constant(class_name: &str, constant: &str) -> Option<Value> {
    match (
        normalize_class_name(class_name).as_str(),
        constant.to_ascii_lowercase().as_str(),
    ) {
        ("regexiterator" | "recursiveregexiterator", "get_match") => Some(Value::Int(1)),
        ("filesystemiterator" | "recursivedirectoryiterator", "skip_dots") => {
            Some(Value::Int(4096))
        }
        ("filesystemiterator" | "recursivedirectoryiterator", "unix_paths") => {
            Some(Value::Int(8192))
        }
        _ => None,
    }
}

fn xml_reader_class_constant(class_name: &str, constant: &str) -> Option<Value> {
    class_name.eq_ignore_ascii_case("XMLReader").then_some(())?;
    let value = match constant.to_ascii_uppercase().as_str() {
        "NONE" => php_runtime::api::xml::XML_READER_NONE,
        "ELEMENT" => php_runtime::api::xml::XML_READER_ELEMENT,
        "ATTRIBUTE" => php_runtime::api::xml::XML_READER_ATTRIBUTE,
        "TEXT" => php_runtime::api::xml::XML_READER_TEXT,
        "END_ELEMENT" => php_runtime::api::xml::XML_READER_END_ELEMENT,
        _ => return None,
    };
    Some(Value::Int(value))
}

pub(super) fn native_internal_class_constant(class_name: &str, constant: &str) -> Option<Value> {
    date_time_class_constant(class_name, constant)
        .or_else(|| pdo_mysql_class_constant(class_name, constant))
        .or_else(|| spl_iterator_class_constant(class_name, constant))
        .or_else(|| xml_reader_class_constant(class_name, constant))
}

pub(super) fn native_internal_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    let object_class = normalize_class_name(object_class);
    if !matches!(
        object_class.as_str(),
        "datetime" | "datetimeimmutable" | "datetimezone" | "dateinterval"
    ) {
        return None;
    }
    let target_class = normalize_class_name(target_class);
    Some(match target_class.as_str() {
        "datetimeinterface" => matches!(object_class.as_str(), "datetime" | "datetimeimmutable"),
        "datetime" | "datetimeimmutable" | "datetimezone" | "dateinterval" => {
            object_class == target_class
        }
        _ => false,
    })
}
