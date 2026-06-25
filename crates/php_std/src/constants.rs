//! PHP 8.5.7 core and platform constants for standard-library.

use php_runtime::{FloatValue, PhpString, Value};

use crate::ConstantValue;

/// Target PHP version.
pub const PHP_VERSION: &str = "8.5.7";
/// Target PHP version ID.
pub const PHP_VERSION_ID: i64 = 80507;
/// Target PHP major version.
pub const PHP_MAJOR_VERSION: i64 = 8;
/// Target PHP minor version.
pub const PHP_MINOR_VERSION: i64 = 5;
/// Target PHP release version.
pub const PHP_RELEASE_VERSION: i64 = 7;
/// PHP integer size in bytes for the current build target.
pub const PHP_INT_SIZE: i64 = std::mem::size_of::<isize>() as i64;
/// PHP maximum integer for the current build target.
pub const PHP_INT_MAX: i64 = isize::MAX as i64;
/// PHP minimum integer for the current build target.
pub const PHP_INT_MIN: i64 = isize::MIN as i64;
/// PHP positive infinity constant.
pub const INF: FloatValue = FloatValue::from_f64(f64::INFINITY);
/// PHP quiet NaN constant.
pub const NAN: FloatValue = FloatValue::from_f64(f64::NAN);

/// Directory separator for the current build target.
#[cfg(windows)]
pub const DIRECTORY_SEPARATOR: &str = "\\";
/// Directory separator for the current build target.
#[cfg(not(windows))]
pub const DIRECTORY_SEPARATOR: &str = "/";

/// Path separator for the current build target.
#[cfg(windows)]
pub const PATH_SEPARATOR: &str = ";";
/// Path separator for the current build target.
#[cfg(not(windows))]
pub const PATH_SEPARATOR: &str = ":";

/// PHP end-of-line constant for this CLI engine.
pub const PHP_EOL: &str = "\n";

/// PHP OS string for the current build target.
#[cfg(target_os = "macos")]
pub const PHP_OS: &str = "Darwin";
/// PHP OS string for the current build target.
#[cfg(target_os = "linux")]
pub const PHP_OS: &str = "Linux";
/// PHP OS string for the current build target.
#[cfg(target_os = "windows")]
pub const PHP_OS: &str = "WINNT";
/// PHP OS string for other targets.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub const PHP_OS: &str = std::env::consts::OS;

/// PHP OS family string for the current build target.
#[cfg(target_os = "macos")]
pub const PHP_OS_FAMILY: &str = "Darwin";
/// PHP OS family string for the current build target.
#[cfg(target_os = "linux")]
pub const PHP_OS_FAMILY: &str = "Linux";
/// PHP OS family string for the current build target.
#[cfg(target_os = "windows")]
pub const PHP_OS_FAMILY: &str = "Windows";
/// PHP OS family string for other targets.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub const PHP_OS_FAMILY: &str = "Unknown";

/// PHP `E_ERROR`.
pub const E_ERROR: i64 = 1;
/// PHP `E_WARNING`.
pub const E_WARNING: i64 = 2;
/// PHP `E_PARSE`.
pub const E_PARSE: i64 = 4;
/// PHP `E_NOTICE`.
pub const E_NOTICE: i64 = 8;
/// PHP `E_CORE_ERROR`.
pub const E_CORE_ERROR: i64 = 16;
/// PHP `E_CORE_WARNING`.
pub const E_CORE_WARNING: i64 = 32;
/// PHP `E_COMPILE_ERROR`.
pub const E_COMPILE_ERROR: i64 = 64;
/// PHP `E_COMPILE_WARNING`.
pub const E_COMPILE_WARNING: i64 = 128;
/// PHP `E_USER_ERROR`.
pub const E_USER_ERROR: i64 = 256;
/// PHP `E_USER_WARNING`.
pub const E_USER_WARNING: i64 = 512;
/// PHP `E_USER_NOTICE`.
pub const E_USER_NOTICE: i64 = 1024;
/// PHP `E_STRICT`.
pub const E_STRICT: i64 = 2048;
/// PHP `E_RECOVERABLE_ERROR`.
pub const E_RECOVERABLE_ERROR: i64 = 4096;
/// PHP `E_DEPRECATED`.
pub const E_DEPRECATED: i64 = 8192;
/// PHP `E_USER_DEPRECATED`.
pub const E_USER_DEPRECATED: i64 = 16384;
/// PHP 8.x `E_ALL`.
pub const E_ALL: i64 = 32767;

/// `parse_url()` component selector for the URL scheme.
pub const PHP_URL_SCHEME: i64 = 0;
/// `parse_url()` component selector for the URL host.
pub const PHP_URL_HOST: i64 = 1;
/// `parse_url()` component selector for the URL port.
pub const PHP_URL_PORT: i64 = 2;
/// `parse_url()` component selector for the URL user.
pub const PHP_URL_USER: i64 = 3;
/// `parse_url()` component selector for the URL password.
pub const PHP_URL_PASS: i64 = 4;
/// `parse_url()` component selector for the URL path.
pub const PHP_URL_PATH: i64 = 5;
/// `parse_url()` component selector for the URL query.
pub const PHP_URL_QUERY: i64 = 6;
/// `parse_url()` component selector for the URL fragment.
pub const PHP_URL_FRAGMENT: i64 = 7;

/// `sort()`/`array_multisort()` ascending order flag.
pub const SORT_ASC: i64 = 4;
/// `sort()`/`array_multisort()` descending order flag.
pub const SORT_DESC: i64 = 3;
/// Regular PHP comparison sort flag.
pub const SORT_REGULAR: i64 = 0;
/// Numeric comparison sort flag.
pub const SORT_NUMERIC: i64 = 1;
/// String comparison sort flag.
pub const SORT_STRING: i64 = 2;
/// Locale-aware string comparison sort flag.
pub const SORT_LOCALE_STRING: i64 = 5;
/// Natural string comparison sort flag.
pub const SORT_NATURAL: i64 = 6;
/// Case-insensitive string/natural sort modifier.
pub const SORT_FLAG_CASE: i64 = 8;

/// Lowercase key conversion flag.
pub const CASE_LOWER: i64 = 0;
/// Uppercase key conversion flag.
pub const CASE_UPPER: i64 = 1;
/// Non-recursive count mode.
pub const COUNT_NORMAL: i64 = 0;
/// Recursive count mode.
pub const COUNT_RECURSIVE: i64 = 1;
/// `array_filter()` callback receives value and key.
pub const ARRAY_FILTER_USE_BOTH: i64 = 1;
/// `array_filter()` callback receives key.
pub const ARRAY_FILTER_USE_KEY: i64 = 2;

/// Converts registry constant metadata into a runtime value.
#[must_use]
pub fn constant_to_value(value: ConstantValue) -> Value {
    match value {
        ConstantValue::Bool(value) => Value::Bool(value),
        ConstantValue::Int(value) => Value::Int(value),
        ConstantValue::Float(value) => Value::Float(value),
        ConstantValue::String(value) => Value::String(PhpString::from(value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExtensionRegistry;

    #[test]
    fn version_constants_match_foundation_target() {
        assert_eq!(PHP_VERSION, "8.5.7");
        assert_eq!(PHP_VERSION_ID, 80507);
        assert_eq!(PHP_MAJOR_VERSION, 8);
        assert_eq!(PHP_MINOR_VERSION, 5);
        assert_eq!(PHP_RELEASE_VERSION, 7);
        assert_eq!(PHP_INT_SIZE, std::mem::size_of::<isize>() as i64);
        assert_eq!(PHP_INT_MAX, isize::MAX as i64);
        assert_eq!(PHP_INT_MIN, isize::MIN as i64);
        assert!(INF.to_f64().is_infinite());
        assert!(NAN.to_f64().is_nan());
    }

    #[test]
    fn core_constants_are_registered_with_values() {
        let registry = ExtensionRegistry::standard_library();
        let version_id = registry
            .enabled_constant("PHP_VERSION_ID")
            .expect("PHP_VERSION_ID");
        assert_eq!(version_id.value(), Some(ConstantValue::Int(80507)));

        let separator = registry
            .enabled_constant("DIRECTORY_SEPARATOR")
            .expect("DIRECTORY_SEPARATOR");
        assert_eq!(
            constant_to_value(separator.value().expect("separator value")),
            Value::String(PhpString::from(DIRECTORY_SEPARATOR))
        );

        assert_eq!(
            registry
                .enabled_constant("PHP_INT_MAX")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(PHP_INT_MAX))
        );
        assert_eq!(
            registry
                .enabled_constant("PHP_INT_SIZE")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(PHP_INT_SIZE))
        );
        assert!(matches!(
            registry
                .enabled_constant("INF")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Float(value)) if value.to_f64().is_infinite()
        ));
        assert!(matches!(
            registry
                .enabled_constant("NAN")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Float(value)) if value.to_f64().is_nan()
        ));
    }

    #[test]
    fn standard_array_constants_match_php_src_values() {
        assert_eq!(SORT_REGULAR, 0);
        assert_eq!(SORT_NUMERIC, 1);
        assert_eq!(SORT_STRING, 2);
        assert_eq!(SORT_DESC, 3);
        assert_eq!(SORT_ASC, 4);
        assert_eq!(SORT_LOCALE_STRING, 5);
        assert_eq!(SORT_NATURAL, 6);
        assert_eq!(SORT_FLAG_CASE, 8);
        assert_eq!(CASE_LOWER, 0);
        assert_eq!(CASE_UPPER, 1);
        assert_eq!(COUNT_NORMAL, 0);
        assert_eq!(COUNT_RECURSIVE, 1);
        assert_eq!(ARRAY_FILTER_USE_BOTH, 1);
        assert_eq!(ARRAY_FILTER_USE_KEY, 2);
    }

    #[test]
    fn json_constants_are_enabled_with_json_extension() {
        let mut registry = ExtensionRegistry::standard_library();
        assert_eq!(
            registry
                .enabled_constant("JSON_ERROR_NONE")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(0))
        );

        registry.disable_extension("json").expect("disable json");
        assert!(registry.enabled_constant("JSON_ERROR_NONE").is_none());
        registry.enable_extension("json").expect("re-enable json");
        assert_eq!(
            registry
                .enabled_constant("JSON_ERROR_NONE")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(0))
        );
    }
}
