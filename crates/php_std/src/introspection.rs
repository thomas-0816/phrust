//! Extension and symbol introspection helpers.

use php_runtime::api::{PhpArray, PhpString, Value};

use crate::ExtensionRegistry;

/// Returns whether an extension is registered and enabled.
#[must_use]
pub fn extension_loaded(registry: &ExtensionRegistry, name: &str) -> bool {
    registry.extension_case_insensitive(name).is_some() && registry.is_extension_enabled(name)
}

/// Returns loaded extension names in deterministic order.
#[must_use]
pub fn get_loaded_extensions(registry: &ExtensionRegistry) -> Vec<&'static str> {
    registry.enabled_extension_names()
}

/// Returns loaded extension names as a PHP array value.
#[must_use]
pub fn get_loaded_extensions_value(registry: &ExtensionRegistry) -> Value {
    Value::Array(PhpArray::from_packed(
        get_loaded_extensions(registry)
            .into_iter()
            .map(|name| Value::String(PhpString::from(name)))
            .collect(),
    ))
}

/// Returns enabled PHP-visible function names for an extension.
#[must_use]
pub fn get_extension_funcs_value(registry: &ExtensionRegistry, extension: &str) -> Value {
    let Some(extension) = registry.extension_case_insensitive(extension) else {
        return Value::Bool(false);
    };
    if !registry.is_extension_enabled(extension.name()) {
        return Value::Bool(false);
    }
    Value::Array(PhpArray::from_packed(
        extension
            .functions()
            .iter()
            .filter(|function| function.visibility() == crate::SymbolVisibility::PhpVisible)
            .map(|function| Value::String(PhpString::from(function.name())))
            .collect(),
    ))
}

/// Returns whether a PHP-visible internal function is enabled.
#[must_use]
pub fn function_exists(registry: &ExtensionRegistry, name: &str) -> bool {
    registry.enabled_php_function(name).is_some()
}

/// Returns whether an internal class/interface/trait/enum is enabled.
#[must_use]
pub fn class_exists(registry: &ExtensionRegistry, name: &str) -> bool {
    registry.enabled_class(name).is_some()
}

/// Returns whether an enabled internal constant exists.
#[must_use]
pub fn defined(registry: &ExtensionRegistry, name: &str) -> bool {
    registry.enabled_constant(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClassDescriptor, ClassKind, ExtensionDescriptor};

    #[test]
    fn extension_loaded_is_case_insensitive_and_configurable() {
        let mut registry = ExtensionRegistry::standard_library().clone();

        assert!(extension_loaded(&registry, "CORE"));
        assert!(extension_loaded(&registry, "curl"));
        assert!(extension_loaded(&registry, "DATE"));
        assert!(extension_loaded(&registry, "ffi"));
        assert!(extension_loaded(&registry, "JSON"));
        assert!(extension_loaded(&registry, "PCRE"));
        assert!(extension_loaded(&registry, "mbstring"));
        assert!(extension_loaded(&registry, "mysqli"));
        assert!(extension_loaded(&registry, "openssl"));
        assert!(extension_loaded(&registry, "pcntl"));
        assert!(extension_loaded(&registry, "phar"));
        assert!(extension_loaded(&registry, "pdo"));
        assert!(extension_loaded(&registry, "pdo_mysql"));
        assert!(extension_loaded(&registry, "pdo_pgsql"));
        assert!(extension_loaded(&registry, "pdo_sqlite"));
        assert!(extension_loaded(&registry, "pgsql"));
        assert!(extension_loaded(&registry, "readline"));
        assert!(extension_loaded(&registry, "reflection"));
        assert!(extension_loaded(&registry, "session"));
        assert!(extension_loaded(&registry, "shmop"));
        assert!(extension_loaded(&registry, "sqlite3"));
        assert!(extension_loaded(&registry, "sysvmsg"));
        assert!(extension_loaded(&registry, "sysvsem"));
        assert!(extension_loaded(&registry, "sysvshm"));

        registry.disable_extension("json").expect("disable json");
        assert!(!extension_loaded(&registry, "Json"));
        registry.disable_extension("core").expect("disable core");
        assert!(!extension_loaded(&registry, "core"));
    }

    #[test]
    fn get_extension_funcs_returns_enabled_php_functions() {
        let registry = ExtensionRegistry::standard_library();
        let Value::Array(core_functions) = get_extension_funcs_value(registry, "CORE") else {
            panic!("core extension functions should return an array");
        };
        let names = core_functions
            .packed_elements()
            .expect("function list should be packed")
            .into_iter()
            .filter_map(|value| value.as_php_string().map(PhpString::to_string_lossy))
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == "zend_version"));
        assert!(names.iter().any(|name| name == "get_defined_vars"));
        assert_eq!(
            get_extension_funcs_value(registry, "missing"),
            Value::Bool(false)
        );
    }

    #[test]
    fn loaded_extensions_are_stable_and_array_convertible() {
        let registry = ExtensionRegistry::standard_library();

        assert_eq!(
            get_loaded_extensions(registry),
            [
                "apcu",
                "bcmath",
                "calendar",
                "core",
                "ctype",
                "curl",
                "date",
                "dom",
                "exif",
                "ffi",
                "fileinfo",
                "filter",
                "ftp",
                "gd",
                "gettext",
                "gmp",
                "hash",
                "iconv",
                "igbinary",
                "imagick",
                "imap",
                "json",
                "ldap",
                "mbstring",
                "memcached",
                "msgpack",
                "mysqli",
                "opcache",
                "openssl",
                "pcntl",
                "pcre",
                "pdo",
                "pdo_mysql",
                "pdo_pgsql",
                "pdo_sqlite",
                "pgsql",
                "phar",
                "posix",
                "random",
                "readline",
                "redis",
                "reflection",
                "session",
                "shmop",
                "simplexml",
                "soap",
                "sockets",
                "sodium",
                "spl",
                "sqlite3",
                "ssh2",
                "standard",
                "sysvmsg",
                "sysvsem",
                "sysvshm",
                "tokenizer",
                "xml",
                "xmlreader",
                "xmlwriter",
                "xsl",
                "zip",
                "zlib"
            ]
        );
        let Value::Array(array) = get_loaded_extensions_value(registry) else {
            panic!("expected array");
        };
        assert_eq!(array.len(), 62);
    }

    #[test]
    fn function_names_are_case_insensitive_and_test_helpers_hidden() {
        let mut registry = ExtensionRegistry::standard_library().clone();
        registry.enable_extension("test").expect("enable test");

        assert!(function_exists(&registry, "STRLEN"));
        assert!(function_exists(&registry, "curl_exec"));
        assert!(function_exists(&registry, "mb_strlen"));
        assert!(function_exists(&registry, "mysqli_more_results"));
        assert!(function_exists(&registry, "mysqli_query"));
        assert!(function_exists(&registry, "openssl_digest"));
        assert!(function_exists(&registry, "pdo_drivers"));
        assert!(function_exists(&registry, "readline_info"));
        assert!(function_exists(&registry, "shmop_open"));
        assert!(function_exists(&registry, "msg_get_queue"));
        assert!(function_exists(&registry, "sem_get"));
        assert!(function_exists(&registry, "shm_attach"));
        assert!(function_exists(&registry, "imagecreatefromstring"));
        assert!(function_exists(&registry, "gettext"));
        assert!(!function_exists(&registry, "__php_std_test_probe"));
        assert!(!function_exists(&registry, "composer_missing_function"));
    }

    #[test]
    fn class_names_are_case_insensitive_and_constants_exact() {
        let registry = ExtensionRegistry::from_extensions([ExtensionDescriptor::new("core")
            .with_class(ClassDescriptor::new(
                "RuntimeException",
                "core",
                ClassKind::Class,
            ))
            .with_constant(crate::ConstantDescriptor::new("PHP_VERSION_ID", "core"))]);

        assert!(class_exists(&registry, "runtimeexception"));
        assert!(defined(&registry, "PHP_VERSION_ID"));
        assert!(!defined(&registry, "php_version_id"));
    }
}
