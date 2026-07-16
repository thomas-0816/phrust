//! Deterministic builtin registry assembled from module slices.

use super::signatures::InternalFunction;
use super::{generated, modules};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Registered builtin entry.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinEntry {
    name: &'static str,
    function: InternalFunction,
    compatibility: BuiltinCompatibility,
    handler_kind: BuiltinHandlerKind,
    helper_id: u32,
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
            handler_kind: BuiltinHandlerKind::Generic,
            helper_id: 0,
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

    /// Capability class used by compact VM and native call adapters.
    #[must_use]
    pub const fn handler_kind(self) -> BuiltinHandlerKind {
        self.handler_kind
    }

    /// Stable name-derived helper ID exposed to native code.
    #[must_use]
    pub const fn helper_id(self) -> u32 {
        self.helper_id
    }

    /// Refines generated registry metadata with an intrinsic handler class.
    #[must_use]
    pub const fn with_handler_kind(mut self, handler_kind: BuiltinHandlerKind) -> Self {
        self.handler_kind = handler_kind;
        self
    }
}

/// Service capability required by a builtin handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinHandlerKind {
    Pure0,
    Pure1,
    Pure2,
    Pure3,
    BorrowedN,
    Json,
    Pcre,
    Filesystem,
    Http,
    Session,
    Mysql,
    Generic,
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

    /// Looks up the stable helper identity persisted by native call metadata.
    #[must_use]
    pub fn get_by_helper_id(self, helper_id: u32) -> Option<BuiltinEntry> {
        let entries = entries();
        let index = HELPER_INDEX
            .get_or_init(|| {
                entries
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| (entry.helper_id, index))
                    .collect()
            })
            .get(&helper_id)
            .copied()?;
        entries.get(index).copied()
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
static HELPER_INDEX: OnceLock<HashMap<u32, usize>> = OnceLock::new();

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
                .flat_map(|(module, entries)| {
                    entries.iter().copied().map(|mut entry| {
                        entry.handler_kind = module_handler_kind(module);
                        entry.helper_id = stable_builtin_helper_id(entry.name);
                        entry
                    })
                })
                .collect::<Vec<_>>();
            entries.sort_unstable_by_key(|entry| entry.name);
            debug_assert!(
                entries.windows(2).all(|pair| pair[0].name != pair[1].name),
                "builtin registry names must be unique for binary-search lookup"
            );
            let mut helper_ids = entries.iter().map(|entry| entry.helper_id).collect::<Vec<_>>();
            helper_ids.sort_unstable();
            assert!(
                helper_ids.windows(2).all(|pair| pair[0] != pair[1]),
                "builtin helper IDs must be collision-free"
            );
            entries
        })
        .as_slice()
}

fn module_handler_kind(module: &str) -> BuiltinHandlerKind {
    match module {
        "json" => BuiltinHandlerKind::Json,
        "pcre" => BuiltinHandlerKind::Pcre,
        "filesystem" => BuiltinHandlerKind::Filesystem,
        "curl" => BuiltinHandlerKind::Http,
        "session" => BuiltinHandlerKind::Session,
        "mysqli" => BuiltinHandlerKind::Mysql,
        _ => BuiltinHandlerKind::Generic,
    }
}

fn stable_builtin_helper_id(name: &str) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in name.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    if hash == 0 { 1 } else { hash }
}

#[cfg(test)]
mod tests {
    use super::{BuiltinHandlerKind, BuiltinRegistry, generated};

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

    #[test]
    fn registry_attaches_capabilities_and_stable_helper_ids() {
        let registry = BuiltinRegistry::new();
        let json = registry.get("json_encode").expect("json_encode");
        let session = registry.get("session_start").expect("session_start");
        let strlen = registry.get("strlen").expect("strlen");

        assert_eq!(json.handler_kind(), BuiltinHandlerKind::Json);
        assert_eq!(session.handler_kind(), BuiltinHandlerKind::Session);
        assert_eq!(strlen.handler_kind(), BuiltinHandlerKind::Generic);
        assert_ne!(json.helper_id(), 0);
        assert_eq!(
            BuiltinRegistry::new()
                .get_by_helper_id(json.helper_id())
                .map(|entry| entry.name()),
            Some("json_encode")
        );
        assert_eq!(
            json.helper_id(),
            BuiltinRegistry::new()
                .get("json_encode")
                .unwrap()
                .helper_id()
        );
    }
}
