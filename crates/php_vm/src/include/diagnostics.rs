//! Typed include diagnostics shared by loader and cache components.

use crate::error::VmError;
use std::path::Path;

pub(super) fn include_read_error(path: &Path, error: std::io::Error) -> VmError {
    include_error(
        "E_PHP_VM_INCLUDE_READ",
        format!("{}: {error}", path.display()),
    )
    .with_context("canonical_path", path.display())
    .with_context("reason", &error)
}

pub(super) fn include_metadata_error(path: &Path, error: std::io::Error) -> VmError {
    include_error(
        "E_PHP_VM_INCLUDE_METADATA",
        format!("{}: {error}", path.display()),
    )
    .with_context("path", path.display())
}

pub(super) fn include_error_suggestion(code: &str) -> &'static str {
    match code {
        "E_PHP_VM_INCLUDE_DISABLED" => {
            "configure an allowed include root before executing include or require"
        }
        "E_PHP_VM_INCLUDE_UNSUPPORTED_SCHEME" => {
            "use a local path or phar URI supported by the include loader"
        }
        "E_PHP_VM_INCLUDE_MISSING" => {
            "check the requested path, current working directory, and include_path entries"
        }
        "E_PHP_VM_INCLUDE_OUTSIDE_ROOT" => {
            "add the canonical parent directory to the allowed include roots"
        }
        "E_PHP_VM_INCLUDE_COMPILE_ERROR" => {
            "inspect the included file compile diagnostic and source span"
        }
        _ => "inspect the include path and loader configuration",
    }
}

pub(super) fn include_error(code: &'static str, message: impl Into<String>) -> VmError {
    VmError::fatal(code, "include", message)
}

pub(super) fn include_cache_lock_error(cache: &'static str, operation: &'static str) -> VmError {
    VmError::internal(
        "E_PHP_VM_INCLUDE_CACHE_POISONED",
        "include",
        format!("{cache} include cache lock poisoned during {operation}"),
    )
    .with_context("cache", cache)
    .with_context("operation", operation)
}
