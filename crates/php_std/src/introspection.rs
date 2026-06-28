//! Extension and symbol introspection helpers.

use php_runtime::{PhpArray, PhpString, Value};

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
        let mut registry = ExtensionRegistry::standard_library();

        assert!(extension_loaded(&registry, "CORE"));
        assert!(extension_loaded(&registry, "curl"));
        assert!(extension_loaded(&registry, "DATE"));
        assert!(extension_loaded(&registry, "JSON"));
        assert!(extension_loaded(&registry, "PCRE"));
        assert!(extension_loaded(&registry, "mbstring"));
        assert!(extension_loaded(&registry, "mysqli"));
        assert!(extension_loaded(&registry, "openssl"));
        assert!(extension_loaded(&registry, "phar"));
        assert!(extension_loaded(&registry, "pdo"));
        assert!(extension_loaded(&registry, "pdo_sqlite"));
        assert!(extension_loaded(&registry, "reflection"));
        assert!(extension_loaded(&registry, "session"));
        assert!(extension_loaded(&registry, "sqlite3"));

        registry.disable_extension("json").expect("disable json");
        assert!(!extension_loaded(&registry, "Json"));
        registry.disable_extension("core").expect("disable core");
        assert!(!extension_loaded(&registry, "core"));
    }

    #[test]
    fn loaded_extensions_are_stable_and_array_convertible() {
        let registry = ExtensionRegistry::standard_library();

        assert_eq!(
            get_loaded_extensions(&registry),
            [
                "core",
                "curl",
                "date",
                "exif",
                "fileinfo",
                "filter",
                "hash",
                "iconv",
                "json",
                "mbstring",
                "mysqli",
                "openssl",
                "pcre",
                "pdo",
                "pdo_sqlite",
                "phar",
                "random",
                "reflection",
                "session",
                "spl",
                "sqlite3",
                "standard",
                "tokenizer",
                "zip",
                "zlib"
            ]
        );
        let Value::Array(array) = get_loaded_extensions_value(&registry) else {
            panic!("expected array");
        };
        assert_eq!(array.len(), 25);
    }

    #[test]
    fn function_names_are_case_insensitive_and_test_helpers_hidden() {
        let mut registry = ExtensionRegistry::standard_library();
        registry.enable_extension("test").expect("enable test");

        assert!(function_exists(&registry, "STRLEN"));
        assert!(function_exists(&registry, "curl_exec"));
        assert!(function_exists(&registry, "mb_strlen"));
        assert!(function_exists(&registry, "mysqli_query"));
        assert!(function_exists(&registry, "openssl_digest"));
        assert!(function_exists(&registry, "pdo_drivers"));
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
