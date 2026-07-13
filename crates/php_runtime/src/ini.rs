//! Deterministic standard-library INI/config registry.

use std::collections::HashMap;
use std::sync::Arc;
/// One supported INI entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IniEntrySnapshot {
    /// Extension that owns this INI option.
    pub extension: &'static str,
    /// Canonical INI option name.
    pub name: &'static str,
    /// Engine default value.
    pub global_value: String,
    /// Current per-request value.
    pub local_value: String,
    /// PHP-style access mask. The standard-library MVP treats supported entries as all.
    pub access: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IniEntry {
    extension: &'static str,
    name: &'static str,
    global_value: &'static str,
    local_value: String,
    access: i64,
}

/// Small, deterministic registry for Composer-typical INI checks.
#[derive(Clone, Debug, Eq, PartialEq)]

pub struct IniRegistry {
    /// Shared entry table: cloning a registry is a refcount bump, and the
    /// first mutation after a clone copies the table (ini_set is rare while
    /// registries are cloned into every builtin invocation context).
    entries: Arc<Vec<IniEntry>>,
}

impl Default for IniRegistry {
    fn default() -> Self {
        // One shared pristine table for the whole process: a default
        // registry is built per builtin invocation context, and
        // materializing all 57 owned local values there put ~60 heap
        // allocations on every builtin dispatch. Mutation already
        // copies-on-write through `Arc::make_mut`.
        static DEFAULT_TABLE: std::sync::OnceLock<Arc<Vec<IniEntry>>> = std::sync::OnceLock::new();
        Self {
            entries: Arc::clone(DEFAULT_TABLE.get_or_init(|| {
                Arc::new(
                    default_entries()
                        .into_iter()
                        .map(|(extension, name, value, access)| IniEntry {
                            extension,
                            name,
                            global_value: value,
                            local_value: value.to_owned(),
                            access,
                        })
                        .collect(),
                )
            })),
        }
    }
}

impl IniRegistry {
    /// Returns a stable snapshot for supported options.
    #[must_use]
    pub fn entries(&self) -> Vec<IniEntrySnapshot> {
        self.entries
            .iter()
            .map(|entry| IniEntrySnapshot {
                extension: entry.extension,
                name: entry.name,
                global_value: entry.global_value.to_owned(),
                local_value: entry.local_value.clone(),
                access: entry.access,
            })
            .collect()
    }

    /// Returns a stable snapshot for options owned by an extension.
    #[must_use]
    pub fn entries_for_extension(&self, extension: &str) -> Vec<IniEntrySnapshot> {
        self.entries()
            .into_iter()
            .filter(|entry| entry.extension.eq_ignore_ascii_case(extension))
            .collect()
    }

    /// Reads the current per-request value.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.lookup(name).map(|entry| entry.local_value.as_str())
    }

    /// Reads the engine default value.
    #[must_use]
    pub fn cfg_var(&self, name: &str) -> Option<&str> {
        self.lookup(name).map(|entry| entry.global_value)
    }

    /// Overrides a supported option and returns its previous local value.
    pub fn set(&mut self, name: &str, value: impl Into<String>) -> Option<String> {
        let entry = self.lookup_mut(name)?;
        if entry.access & 1 == 0 {
            return None;
        }
        let next = normalize_ini_value(entry.name, value.into());
        let previous = std::mem::replace(&mut entry.local_value, next);
        Some(previous)
    }

    /// Applies a startup/configuration override and returns the previous local
    /// value. This intentionally bypasses runtime mutability checks used by
    /// `ini_set()`.
    pub fn set_startup(&mut self, name: &str, value: impl Into<String>) -> Option<String> {
        let entry = self.lookup_mut(name)?;
        let next = normalize_ini_value(entry.name, value.into());
        let previous = std::mem::replace(&mut entry.local_value, next);
        Some(previous)
    }

    fn lookup(&self, name: &str) -> Option<&IniEntry> {
        self.entries.get(entry_index(name)?)
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut IniEntry> {
        let index = entry_index(name)?;
        Arc::make_mut(&mut self.entries).get_mut(index)
    }
}

/// Position of a supported option in the (fixed, shared) entry table.
///
/// Every registry is built from `default_entries` in the same order and
/// `set` only mutates values in place, so one process-wide index replaces
/// the per-lookup case-insensitive linear scan (which ran twice per
/// builtin dispatch for the diagnostic display options alone).
fn entry_index(name: &str) -> Option<usize> {
    static NAME_INDEX: std::sync::OnceLock<HashMap<&'static str, usize>> =
        std::sync::OnceLock::new();
    let index = NAME_INDEX.get_or_init(|| {
        default_entries()
            .into_iter()
            .enumerate()
            .map(|(position, (_, name, _, _))| (name, position))
            .collect()
    });
    if let Some(position) = index.get(name) {
        return Some(*position);
    }
    if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return index.get(name.to_ascii_lowercase().as_str()).copied();
    }
    None
}

fn normalize_ini_value(name: &str, value: String) -> String {
    if is_boolean_ini_entry(name) {
        if parse_php_ini_bool(&value) {
            "1".to_owned()
        } else {
            "0".to_owned()
        }
    } else {
        value
    }
}

fn is_boolean_ini_entry(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "display_errors"
            | "file_uploads"
            | "ignore_user_abort"
            | "pcre.jit"
            | "session.auto_start"
            | "session.cookie_secure"
            | "session.cookie_partitioned"
            | "session.cookie_httponly"
            | "session.use_cookies"
            | "session.use_only_cookies"
            | "session.use_strict_mode"
            | "session.use_trans_sid"
            | "session.lazy_write"
    )
}

fn parse_php_ini_bool(value: &str) -> bool {
    if matches_ignore_ascii_case(value, "true")
        || matches_ignore_ascii_case(value, "yes")
        || matches_ignore_ascii_case(value, "on")
    {
        return true;
    }
    php_atoi_prefix(value) != 0
}

fn matches_ignore_ascii_case(value: &str, expected: &str) -> bool {
    value.len() == expected.len() && value.eq_ignore_ascii_case(expected)
}

fn php_atoi_prefix(value: &str) -> i64 {
    let trimmed = value.trim_start();
    let bytes = trimmed.as_bytes();
    let mut index = 0usize;
    let mut sign = 1i64;
    if let Some(first) = bytes.first() {
        if *first == b'-' {
            sign = -1;
            index = 1;
        } else if *first == b'+' {
            index = 1;
        }
    }

    let mut parsed = 0i64;
    let mut consumed = false;
    while let Some(byte) = bytes.get(index) {
        if !byte.is_ascii_digit() {
            break;
        }
        parsed = parsed
            .saturating_mul(10)
            .saturating_add(i64::from(byte - b'0'));
        consumed = true;
        index += 1;
    }
    if consumed { parsed * sign } else { 0 }
}

fn default_entries() -> [(&'static str, &'static str, &'static str, i64); 57] {
    [
        ("standard", "arg_separator.input", "&", 7),
        ("standard", "arg_separator.output", "&", 7),
        ("date", "date.timezone", "UTC", 7),
        ("standard", "default_charset", "UTF-8", 7),
        ("core", "display_errors", "1", 7),
        ("core", "error_reporting", "-1", 7),
        ("ffi", "ffi.enable", "preload", 4),
        ("ffi", "ffi.preload", "", 4),
        ("filter", "filter.default", "unsafe_raw", 7),
        ("filter", "filter.default_flags", "", 7),
        ("standard", "file_uploads", "1", 7),
        ("iconv", "iconv.input_encoding", "", 7),
        ("iconv", "iconv.internal_encoding", "", 7),
        ("iconv", "iconv.output_encoding", "", 7),
        ("standard", "ignore_user_abort", "0", 7),
        ("standard", "include_path", ".", 7),
        ("standard", "input_encoding", "", 7),
        ("standard", "internal_encoding", "", 7),
        ("standard", "max_file_uploads", "20", 7),
        ("standard", "max_input_nesting_level", "64", 7),
        ("standard", "max_input_vars", "1000", 7),
        ("core", "memory_limit", "128M", 7),
        ("core", "open_basedir", "", 7),
        ("standard", "output_encoding", "", 7),
        ("standard", "post_max_size", "8M", 7),
        ("pcre", "pcre.backtrack_limit", "1000000", 7),
        ("pcre", "pcre.jit", "1", 7),
        ("pcre", "pcre.recursion_limit", "100000", 7),
        ("core", "precision", "14", 7),
        ("core", "serialize_precision", "-1", 7),
        ("session", "session.save_path", "", 7),
        ("session", "session.name", "PHPSESSID", 7),
        ("session", "session.save_handler", "files", 7),
        ("session", "session.auto_start", "0", 4),
        ("session", "session.gc_probability", "1", 7),
        ("session", "session.gc_divisor", "100", 7),
        ("session", "session.gc_maxlifetime", "1440", 7),
        ("session", "session.serialize_handler", "php", 7),
        ("session", "session.sid_length", "32", 7),
        ("session", "session.sid_bits_per_character", "4", 7),
        ("session", "session.use_strict_mode", "0", 7),
        ("session", "session.cookie_lifetime", "0", 7),
        ("session", "session.cookie_path", "/", 7),
        ("session", "session.cookie_domain", "", 7),
        ("session", "session.cookie_secure", "0", 7),
        ("session", "session.cookie_partitioned", "0", 7),
        ("session", "session.cookie_httponly", "0", 7),
        ("session", "session.cookie_samesite", "", 7),
        ("session", "session.use_cookies", "1", 7),
        ("session", "session.use_only_cookies", "1", 7),
        ("session", "session.referer_check", "", 7),
        ("session", "session.cache_expire", "180", 7),
        ("session", "session.cache_limiter", "nocache", 7),
        ("session", "session.use_trans_sid", "0", 7),
        ("session", "session.lazy_write", "1", 7),
        ("standard", "upload_max_filesize", "2M", 7),
        ("standard", "upload_tmp_dir", "", 7),
    ]
}

#[cfg(test)]
mod tests {
    use super::IniRegistry;

    #[test]
    fn ini_registry_reads_and_overrides_supported_values() {
        let mut registry = IniRegistry::default();

        assert_eq!(registry.get("INCLUDE_PATH"), Some("."));
        assert_eq!(registry.cfg_var("include_path"), Some("."));
        assert_eq!(registry.set("include_path", "lib"), Some(".".to_owned()));
        assert_eq!(registry.get("include_path"), Some("lib"));
        assert_eq!(registry.cfg_var("include_path"), Some("."));
        assert_eq!(registry.get("file_uploads"), Some("1"));
        assert_eq!(registry.get("arg_separator.output"), Some("&"));
        assert_eq!(registry.get("upload_tmp_dir"), Some(""));
        assert_eq!(registry.get("upload_max_filesize"), Some("2M"));
        assert_eq!(registry.get("open_basedir"), Some(""));
        assert_eq!(registry.get("post_max_size"), Some("8M"));
        assert_eq!(registry.get("max_file_uploads"), Some("20"));
        assert_eq!(registry.get("pcre.backtrack_limit"), Some("1000000"));
        assert_eq!(registry.get("pcre.jit"), Some("1"));
        assert_eq!(registry.get("pcre.recursion_limit"), Some("100000"));
        assert_eq!(registry.get("ffi.enable"), Some("preload"));
        assert_eq!(registry.cfg_var("ffi.preload"), Some(""));
        assert_eq!(registry.get("filter.default"), Some("unsafe_raw"));
        assert_eq!(registry.get("filter.default_flags"), Some(""));
        assert_eq!(registry.get("input_encoding"), Some(""));
        assert_eq!(registry.get("internal_encoding"), Some(""));
        assert_eq!(registry.get("output_encoding"), Some(""));
        assert_eq!(registry.get("iconv.input_encoding"), Some(""));
        assert_eq!(registry.get("iconv.internal_encoding"), Some(""));
        assert_eq!(registry.get("iconv.output_encoding"), Some(""));
        assert_eq!(registry.get("session.save_path"), Some(""));
        assert_eq!(registry.get("session.name"), Some("PHPSESSID"));
        assert_eq!(registry.get("session.save_handler"), Some("files"));
        assert_eq!(registry.get("session.auto_start"), Some("0"));
        assert_eq!(registry.get("session.gc_probability"), Some("1"));
        assert_eq!(registry.get("session.gc_divisor"), Some("100"));
        assert_eq!(registry.get("session.gc_maxlifetime"), Some("1440"));
        assert_eq!(registry.get("session.serialize_handler"), Some("php"));
        assert_eq!(registry.get("session.sid_length"), Some("32"));
        assert_eq!(registry.get("session.sid_bits_per_character"), Some("4"));
        assert_eq!(registry.get("session.use_strict_mode"), Some("0"));
        assert_eq!(registry.get("session.cookie_lifetime"), Some("0"));
        assert_eq!(registry.get("session.cookie_path"), Some("/"));
        assert_eq!(registry.get("session.cookie_domain"), Some(""));
        assert_eq!(registry.get("session.cookie_secure"), Some("0"));
        assert_eq!(registry.get("session.cookie_partitioned"), Some("0"));
        assert_eq!(registry.get("session.cookie_httponly"), Some("0"));
        assert_eq!(registry.get("session.cookie_samesite"), Some(""));
        assert_eq!(registry.get("session.use_cookies"), Some("1"));
        assert_eq!(registry.get("session.use_only_cookies"), Some("1"));
        assert_eq!(registry.get("session.referer_check"), Some(""));
        assert_eq!(registry.get("session.cache_expire"), Some("180"));
        assert_eq!(registry.get("session.cache_limiter"), Some("nocache"));
        assert_eq!(registry.get("session.use_trans_sid"), Some("0"));
        assert_eq!(registry.get("session.lazy_write"), Some("1"));
        assert_eq!(
            registry.set("session.cookie_samesite", "Lax"),
            Some("".to_owned())
        );
        assert_eq!(registry.get("session.cookie_samesite"), Some("Lax"));
        assert_eq!(registry.set("session.auto_start", "1"), None);
        assert_eq!(registry.get("session.auto_start"), Some("0"));
        assert_eq!(
            registry.set_startup("session.auto_start", "on"),
            Some("0".to_owned())
        );
        assert_eq!(registry.get("session.auto_start"), Some("1"));
        assert_eq!(registry.set("ffi.enable", "1"), None);
        assert_eq!(registry.get("ffi.enable"), Some("preload"));
        assert_eq!(registry.set("missing", "value"), None);
    }

    #[test]
    fn ini_registry_canonicalizes_supported_boolean_values() {
        let mut registry = IniRegistry::default();

        assert_eq!(
            registry.set("session.cookie_secure", "TRUE"),
            Some("0".to_owned())
        );
        assert_eq!(registry.get("session.cookie_secure"), Some("1"));
        assert_eq!(
            registry.set("session.cookie_httponly", "off"),
            Some("0".to_owned())
        );
        assert_eq!(registry.get("session.cookie_httponly"), Some("0"));
        assert_eq!(
            registry.set("session.cookie_partitioned", "2"),
            Some("0".to_owned())
        );
        assert_eq!(registry.get("session.cookie_partitioned"), Some("1"));
        assert_eq!(
            registry.set("session.use_strict_mode", "yes"),
            Some("0".to_owned())
        );
        assert_eq!(registry.get("session.use_strict_mode"), Some("1"));
        assert_eq!(
            registry.set("session.use_cookies", "off"),
            Some("1".to_owned())
        );
        assert_eq!(registry.get("session.use_cookies"), Some("0"));
        assert_eq!(
            registry.set("session.use_only_cookies", "2"),
            Some("1".to_owned())
        );
        assert_eq!(registry.get("session.use_only_cookies"), Some("1"));
        assert_eq!(
            registry.set("session.use_trans_sid", "false"),
            Some("0".to_owned())
        );
        assert_eq!(registry.get("session.use_trans_sid"), Some("0"));
        assert_eq!(
            registry.set("session.lazy_write", "0"),
            Some("1".to_owned())
        );
        assert_eq!(registry.get("session.lazy_write"), Some("0"));
        assert_eq!(registry.set("pcre.jit", "-1"), Some("1".to_owned()));
        assert_eq!(registry.get("pcre.jit"), Some("1"));
    }

    #[test]
    fn ini_registry_entries_are_deterministic() {
        let registry = IniRegistry::default();
        let names = registry
            .entries()
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "arg_separator.input",
                "arg_separator.output",
                "date.timezone",
                "default_charset",
                "display_errors",
                "error_reporting",
                "ffi.enable",
                "ffi.preload",
                "filter.default",
                "filter.default_flags",
                "file_uploads",
                "iconv.input_encoding",
                "iconv.internal_encoding",
                "iconv.output_encoding",
                "ignore_user_abort",
                "include_path",
                "input_encoding",
                "internal_encoding",
                "max_file_uploads",
                "max_input_nesting_level",
                "max_input_vars",
                "memory_limit",
                "open_basedir",
                "output_encoding",
                "post_max_size",
                "pcre.backtrack_limit",
                "pcre.jit",
                "pcre.recursion_limit",
                "precision",
                "serialize_precision",
                "session.save_path",
                "session.name",
                "session.save_handler",
                "session.auto_start",
                "session.gc_probability",
                "session.gc_divisor",
                "session.gc_maxlifetime",
                "session.serialize_handler",
                "session.sid_length",
                "session.sid_bits_per_character",
                "session.use_strict_mode",
                "session.cookie_lifetime",
                "session.cookie_path",
                "session.cookie_domain",
                "session.cookie_secure",
                "session.cookie_partitioned",
                "session.cookie_httponly",
                "session.cookie_samesite",
                "session.use_cookies",
                "session.use_only_cookies",
                "session.referer_check",
                "session.cache_expire",
                "session.cache_limiter",
                "session.use_trans_sid",
                "session.lazy_write",
                "upload_max_filesize",
                "upload_tmp_dir"
            ]
        );

        let ffi_names = registry
            .entries_for_extension("FFI")
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        assert_eq!(ffi_names, vec!["ffi.enable", "ffi.preload"]);

        let filter_names = registry
            .entries_for_extension("FILTER")
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        assert_eq!(filter_names, vec!["filter.default", "filter.default_flags"]);

        let session_names = registry
            .entries_for_extension("SESSION")
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        assert_eq!(
            session_names,
            vec![
                "session.save_path",
                "session.name",
                "session.save_handler",
                "session.auto_start",
                "session.gc_probability",
                "session.gc_divisor",
                "session.gc_maxlifetime",
                "session.serialize_handler",
                "session.sid_length",
                "session.sid_bits_per_character",
                "session.use_strict_mode",
                "session.cookie_lifetime",
                "session.cookie_path",
                "session.cookie_domain",
                "session.cookie_secure",
                "session.cookie_partitioned",
                "session.cookie_httponly",
                "session.cookie_samesite",
                "session.use_cookies",
                "session.use_only_cookies",
                "session.referer_check",
                "session.cache_expire",
                "session.cache_limiter",
                "session.use_trans_sid",
                "session.lazy_write"
            ]
        );
    }
}
