use super::*;

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

pub(super) fn pdo_mysql_class_constant(class_name: &str, constant: &str) -> Option<Value> {
    let value = if class_name.eq_ignore_ascii_case("PDO") {
        MYSQL_ATTRIBUTES
            .iter()
            .find(|(legacy, _, _)| legacy.eq_ignore_ascii_case(constant))
            .map(|(_, _, value)| *value)
    } else if class_name.eq_ignore_ascii_case("Pdo\\Mysql") {
        MYSQL_ATTRIBUTES
            .iter()
            .find(|(_, modern, _)| modern.eq_ignore_ascii_case(constant))
            .map(|(_, _, value)| *value)
    } else {
        None
    }?;
    Some(Value::Int(value))
}

pub(in crate::vm::jit_abi) fn pdo_mysql_deprecated_constant(
    class_name: &str,
    constant: &str,
) -> Option<(&'static str, &'static str)> {
    if !class_name.eq_ignore_ascii_case("PDO") {
        return None;
    }
    MYSQL_ATTRIBUTES
        .iter()
        .find(|(legacy, _, _)| legacy.eq_ignore_ascii_case(constant))
        .map(|(legacy, modern, _)| (*legacy, *modern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_modern_and_deprecated_pdo_mysql_constants() {
        assert_eq!(
            pdo_mysql_class_constant("Pdo\\Mysql", "ATTR_USE_BUFFERED_QUERY"),
            Some(Value::Int(1000))
        );
        assert_eq!(
            pdo_mysql_class_constant("PDO", "MYSQL_ATTR_MULTI_STATEMENTS"),
            Some(Value::Int(1012))
        );
        assert_eq!(
            pdo_mysql_deprecated_constant("PDO", "MYSQL_ATTR_SSL_CA"),
            Some(("MYSQL_ATTR_SSL_CA", "ATTR_SSL_CA"))
        );
    }
}
