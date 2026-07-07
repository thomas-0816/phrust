//! Deterministic builtin registry assembled from module slices.

use super::modules;
use super::signatures::InternalFunction;
use std::sync::OnceLock;

/// Registered builtin entry.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinEntry {
    name: &'static str,
    function: InternalFunction,
    compatibility: BuiltinCompatibility,
}

impl BuiltinEntry {
    pub(in crate::builtins) const fn new(
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

const MODULE_SLICES: &[&[BuiltinEntry]] = &[
    modules::core::ENTRIES,
    modules::arrays::ENTRIES,
    modules::strings::ENTRIES,
    modules::hash::ENTRIES,
    modules::ctype::ENTRIES,
    modules::calendar::ENTRIES,
    modules::filter::ENTRIES,
    modules::iconv::ENTRIES,
    modules::sodium::ENTRIES,
    modules::bcmath::ENTRIES,
    modules::gmp::ENTRIES,
    modules::apcu::ENTRIES,
    modules::redis::ENTRIES,
    modules::memcached::ENTRIES,
    modules::igbinary::ENTRIES,
    modules::msgpack::ENTRIES,
    modules::soap::ENTRIES,
    modules::ftp::ENTRIES,
    modules::imap::ENTRIES,
    modules::ldap::ENTRIES,
    modules::ssh2::ENTRIES,
    modules::sockets::ENTRIES,
    modules::zip::ENTRIES,
    modules::zlib::ENTRIES,
    modules::fileinfo::ENTRIES,
    modules::exif::ENTRIES,
    modules::gd::ENTRIES,
    modules::gettext::ENTRIES,
    modules::math::ENTRIES,
    modules::filesystem::ENTRIES,
    modules::streams::ENTRIES,
    modules::json::ENTRIES,
    modules::mbstring::ENTRIES,
    modules::intl::ENTRIES,
    modules::xml::ENTRIES,
    modules::simplexml::ENTRIES,
    #[cfg(not(target_family = "wasm"))]
    modules::curl::ENTRIES,
    #[cfg(not(target_family = "wasm"))]
    modules::openssl::ENTRIES,
    modules::pcre::ENTRIES,
    modules::pdo::ENTRIES,
    #[cfg(not(target_family = "wasm"))]
    modules::pgsql::ENTRIES,
    #[cfg(not(target_family = "wasm"))]
    modules::pcntl::ENTRIES,
    #[cfg(not(target_family = "wasm"))]
    modules::posix::ENTRIES,
    modules::readline::ENTRIES,
    modules::shmop::ENTRIES,
    modules::sysvmsg::ENTRIES,
    modules::sysvsem::ENTRIES,
    modules::sysvshm::ENTRIES,
    #[cfg(not(target_family = "wasm"))]
    modules::mysqli::ENTRIES,
    modules::opcache::ENTRIES,
    modules::date::ENTRIES,
    modules::session::ENTRIES,
    modules::spl::ENTRIES,
    modules::reflection::ENTRIES,
];

static BUILTINS: OnceLock<Vec<BuiltinEntry>> = OnceLock::new();

fn entries() -> &'static [BuiltinEntry] {
    BUILTINS
        .get_or_init(|| {
            let mut entries = MODULE_SLICES
                .iter()
                .flat_map(|entries| entries.iter().copied())
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
