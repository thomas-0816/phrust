//! Deterministic standard-library INI/config registry.

/// One supported INI entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IniEntrySnapshot {
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
    name: &'static str,
    global_value: &'static str,
    local_value: String,
    access: i64,
}

/// Small, deterministic registry for Composer-typical INI checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IniRegistry {
    entries: Vec<IniEntry>,
}

impl Default for IniRegistry {
    fn default() -> Self {
        Self {
            entries: default_entries()
                .into_iter()
                .map(|(name, value)| IniEntry {
                    name,
                    global_value: value,
                    local_value: value.to_owned(),
                    access: 7,
                })
                .collect(),
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
                name: entry.name,
                global_value: entry.global_value.to_owned(),
                local_value: entry.local_value.clone(),
                access: entry.access,
            })
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
        let previous = std::mem::replace(&mut entry.local_value, value.into());
        Some(previous)
    }

    fn lookup(&self, name: &str) -> Option<&IniEntry> {
        self.entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut IniEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }
}

fn default_entries() -> [(&'static str, &'static str); 11] {
    [
        ("date.timezone", "UTC"),
        ("default_charset", "UTF-8"),
        ("display_errors", "1"),
        ("error_reporting", "-1"),
        ("ignore_user_abort", "0"),
        ("include_path", "."),
        ("max_input_nesting_level", "64"),
        ("max_input_vars", "1000"),
        ("memory_limit", "128M"),
        ("precision", "14"),
        ("serialize_precision", "-1"),
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
        assert_eq!(registry.set("missing", "value"), None);
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
                "date.timezone",
                "default_charset",
                "display_errors",
                "error_reporting",
                "ignore_user_abort",
                "include_path",
                "max_input_nesting_level",
                "max_input_vars",
                "memory_limit",
                "precision",
                "serialize_precision"
            ]
        );
    }
}
