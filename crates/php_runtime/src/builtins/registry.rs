//! Deterministic builtin registry assembled from module slices.

use super::signatures::InternalFunction;
use super::{generated, modules};
use std::sync::OnceLock;

/// Registered builtin entry.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinEntry {
    name: &'static str,
    function: InternalFunction,
    compatibility: BuiltinCompatibility,
}

impl BuiltinEntry {
    pub const fn new(
        name: &'static str,
        function: InternalFunction,
        compatibility: BuiltinCompatibility,
    ) -> Self {
        Self {
            name,
            function,
            compatibility,
        }
    }

    /// Builtin name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Internal function pointer.
    #[must_use]
    pub const fn function(self) -> InternalFunction {
        self.function
    }

    /// Compatibility classification.
    #[must_use]
    pub const fn compatibility(self) -> BuiltinCompatibility {
        self.compatibility
    }
}

/// Whether a builtin is PHP-compatible or only for local fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinCompatibility {
    /// PHP-compatible MVP builtin.
    Php,
    /// Internal test helper, not exposed as a PHP standard builtin.
    InternalTestHelper,
}

/// Deterministic builtin registry.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltinRegistry;

impl BuiltinRegistry {
    /// Creates a builtin registry view.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns entries in stable sorted order.
    #[must_use]
    pub fn entries(self) -> &'static [BuiltinEntry] {
        entries()
    }

    /// Looks up a builtin by normalized name.
    ///
    /// Entries are sorted (and asserted unique) by name at first access, so
    /// lookup is a binary search instead of a linear scan over ~650 entries.
    #[must_use]
    pub fn get(self, name: &str) -> Option<BuiltinEntry> {
        let entries = entries();
        entries
            .binary_search_by(|entry| entry.name.cmp(name))
            .ok()
            .map(|index| entries[index])
    }

    /// Returns true when a normalized name is registered.
    #[must_use]
    pub fn contains(self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

const MODULE_SLICES: &[(&str, &[BuiltinEntry])] = &[
    ("core", modules::core::ENTRIES),
    ("arrays", modules::arrays::ENTRIES),
    ("strings", modules::strings::ENTRIES),
    ("hash", modules::hash::ENTRIES),
    ("calendar", modules::calendar::ENTRIES),
    ("filter", modules::filter::ENTRIES),
    ("iconv", modules::iconv::ENTRIES),
    ("sodium", modules::sodium::ENTRIES),
    ("bcmath", modules::bcmath::ENTRIES),
    ("gmp", modules::gmp::ENTRIES),
    ("redis", modules::redis::ENTRIES),
    ("memcached", modules::memcached::ENTRIES),
    ("igbinary", modules::igbinary::ENTRIES),
    ("msgpack", modules::msgpack::ENTRIES),
    ("soap", modules::soap::ENTRIES),
    ("ftp", modules::ftp::ENTRIES),
    ("imap", modules::imap::ENTRIES),
    ("ldap", modules::ldap::ENTRIES),
    ("ssh2", modules::ssh2::ENTRIES),
    ("sockets", modules::sockets::ENTRIES),
    ("zip", modules::zip::ENTRIES),
    ("zlib", modules::zlib::ENTRIES),
    ("fileinfo", modules::fileinfo::ENTRIES),
    ("exif", modules::exif::ENTRIES),
    ("gd", modules::gd::ENTRIES),
    ("gettext", modules::gettext::ENTRIES),
    ("math", modules::math::ENTRIES),
    ("filesystem", modules::filesystem::ENTRIES),
    ("streams", modules::streams::ENTRIES),
    ("json", modules::json::ENTRIES),
    ("mbstring", modules::mbstring::ENTRIES),
    ("intl", modules::intl::ENTRIES),
    ("xml", modules::xml::ENTRIES),
    ("simplexml", modules::simplexml::ENTRIES),
    ("curl", modules::curl::ENTRIES),
    ("openssl", modules::openssl::ENTRIES),
    ("pcre", modules::pcre::ENTRIES),
    ("pdo", modules::pdo::ENTRIES),
    ("pgsql", modules::pgsql::ENTRIES),
    ("pcntl", modules::pcntl::ENTRIES),
    ("posix", modules::posix::ENTRIES),
    ("readline", modules::readline::ENTRIES),
    ("shmop", modules::shmop::ENTRIES),
    ("sysvmsg", modules::sysvmsg::ENTRIES),
    ("sysvsem", modules::sysvsem::ENTRIES),
    ("sysvshm", modules::sysvshm::ENTRIES),
    ("mysqli", modules::mysqli::ENTRIES),
    ("opcache", modules::opcache::ENTRIES),
    ("date", modules::date::ENTRIES),
    ("session", modules::session::ENTRIES),
    ("spl", modules::spl::ENTRIES),
    ("reflection", modules::reflection::ENTRIES),
];

static BUILTINS: OnceLock<Vec<BuiltinEntry>> = OnceLock::new();

fn entries() -> &'static [BuiltinEntry] {
    BUILTINS
        .get_or_init(|| {
            assert_eq!(
                MODULE_SLICES.len(),
                generated::MODULES.len(),
                "generated builtin module count must match explicit pointer mappings"
            );
            for ((module_name, entries), generated_module) in
                MODULE_SLICES.iter().zip(generated::MODULES)
            {
                assert_eq!(*module_name, generated_module.name);
                let mut actual_names = entries.iter().map(|entry| entry.name).collect::<Vec<_>>();
                actual_names.sort_unstable();
                let expected_names = generated_module
                    .functions
                    .iter()
                    .map(|entry| entry.name)
                    .collect::<Vec<_>>();
                assert_eq!(
                    actual_names, expected_names,
                    "generated builtin descriptors must match explicit function pointers for {module_name}"
                );
            }
            let mut entries = MODULE_SLICES
                .iter()
                .flat_map(|(_, entries)| entries.iter().copied())
                .collect::<Vec<_>>();
            entries.sort_unstable_by_key(|entry| entry.name);
            debug_assert!(
                entries.windows(2).all(|pair| pair[0].name != pair[1].name),
                "builtin registry names must be unique for binary-search lookup"
            );
            entries
        })
        .as_slice()
}

#[cfg(test)]
mod tests {
    use super::generated;

    #[test]
    fn generated_registry_carries_representative_arginfo_signatures() {
        let json = generated::MODULES
            .iter()
            .find(|module| module.name == "json")
            .expect("json module is generated");
        let encode = json
            .functions
            .iter()
            .find(|function| function.name == "json_encode")
            .expect("json_encode descriptor is generated");

        assert_eq!(encode.extension, "json");
        assert_eq!(encode.return_type, Some("string|false"));
        assert_eq!(encode.required_parameters, 1);
        assert_eq!(encode.total_parameters, 3);
        assert!(!encode.variadic);
    }
}
