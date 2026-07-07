#![cfg_attr(target_family = "wasm", allow(dead_code))]

//! standard-library standard-library registry infrastructure.
//!
//! This crate owns metadata for PHP 8.5.7 internal extensions, functions,
//! constants, and classes. This crate intentionally keeps it infrastructure
//! only: no PHP-visible function implementation is exposed from here yet.

pub mod abi;
pub mod arginfo;
pub mod constants;
pub mod generated;
pub mod introspection;

mod extensions;

use extensions::*;
use php_runtime::api::FloatValue;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// Descriptor for one PHP extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionDescriptor {
    name: &'static str,
    enabled_by_default: bool,
    functions: Vec<FunctionDescriptor>,
    constants: Vec<ConstantDescriptor>,
    classes: Vec<ClassDescriptor>,
}

impl ExtensionDescriptor {
    /// Creates an extension descriptor.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            enabled_by_default: true,
            functions: Vec::new(),
            constants: Vec::new(),
            classes: Vec::new(),
        }
    }

    /// Marks whether this extension is enabled in the default registry.
    #[must_use]
    pub fn enabled_by_default(mut self, enabled: bool) -> Self {
        self.enabled_by_default = enabled;
        self
    }

    /// Adds a function descriptor.
    #[must_use]
    pub fn with_function(mut self, function: FunctionDescriptor) -> Self {
        self.functions.push(function);
        self
    }

    /// Adds a constant descriptor.
    #[must_use]
    pub fn with_constant(mut self, constant: ConstantDescriptor) -> Self {
        self.constants.push(constant);
        self
    }

    /// Adds a class descriptor.
    #[must_use]
    pub fn with_class(mut self, class: ClassDescriptor) -> Self {
        self.classes.push(class);
        self
    }

    /// Stable extension name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Whether the extension is enabled by default.
    #[must_use]
    pub const fn is_enabled_by_default(&self) -> bool {
        self.enabled_by_default
    }

    /// Function descriptors in stable name order.
    #[must_use]
    pub fn functions(&self) -> &[FunctionDescriptor] {
        &self.functions
    }

    /// Constant descriptors in extension registration order.
    #[must_use]
    pub fn constants(&self) -> &[ConstantDescriptor] {
        &self.constants
    }

    /// Class descriptors in stable name order.
    #[must_use]
    pub fn classes(&self) -> &[ClassDescriptor] {
        &self.classes
    }

    fn sort_symbols(&mut self) {
        self.functions.sort_by_key(FunctionDescriptor::name);
        self.classes.sort_by_key(ClassDescriptor::name);
    }
}

/// Descriptor for an internal function symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionDescriptor {
    name: &'static str,
    extension: &'static str,
    visibility: SymbolVisibility,
}

impl FunctionDescriptor {
    /// Creates a PHP-visible function descriptor.
    #[must_use]
    pub const fn php(name: &'static str, extension: &'static str) -> Self {
        Self {
            name,
            extension,
            visibility: SymbolVisibility::PhpVisible,
        }
    }

    /// Creates an internal test-only function descriptor.
    #[must_use]
    pub const fn internal_test(name: &'static str, extension: &'static str) -> Self {
        Self {
            name,
            extension,
            visibility: SymbolVisibility::InternalTestFixture,
        }
    }

    /// Stable function name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Owning extension name.
    #[must_use]
    pub const fn extension(&self) -> &'static str {
        self.extension
    }

    /// Symbol visibility classification.
    #[must_use]
    pub const fn visibility(&self) -> SymbolVisibility {
        self.visibility
    }

    /// Generated php-src stub metadata for this function, when available.
    #[must_use]
    pub fn arginfo(&self) -> Option<&'static generated::arginfo::GeneratedFunctionMetadata> {
        generated::arginfo::function_metadata(self.name)
    }
}

/// Descriptor for an internal constant symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstantDescriptor {
    name: &'static str,
    extension: &'static str,
    value: Option<ConstantValue>,
    deprecation: Option<ConstantDeprecation>,
}

impl ConstantDescriptor {
    /// Creates a constant descriptor.
    #[must_use]
    pub const fn new(name: &'static str, extension: &'static str) -> Self {
        Self {
            name,
            extension,
            value: None,
            deprecation: None,
        }
    }

    /// Creates a constant descriptor with a value.
    #[must_use]
    pub const fn with_value(
        name: &'static str,
        extension: &'static str,
        value: ConstantValue,
    ) -> Self {
        Self {
            name,
            extension,
            value: Some(value),
            deprecation: None,
        }
    }

    /// Marks this constant as deprecated in the upstream PHP surface.
    #[must_use]
    pub const fn deprecated(mut self, message: &'static str) -> Self {
        self.deprecation = Some(ConstantDeprecation::new(message));
        self
    }

    /// Stable constant name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Owning extension name.
    #[must_use]
    pub const fn extension(&self) -> &'static str {
        self.extension
    }

    /// Constant value metadata, when available.
    #[must_use]
    pub const fn value(&self) -> Option<ConstantValue> {
        self.value
    }

    /// Deprecation metadata, when accessing this constant should emit one.
    #[must_use]
    pub const fn deprecation(&self) -> Option<ConstantDeprecation> {
        self.deprecation
    }

    /// Generated php-src stub metadata for this constant, when available.
    #[must_use]
    pub fn source_metadata(
        &self,
    ) -> Option<&'static generated::arginfo::GeneratedConstantMetadata> {
        generated::arginfo::constant_metadata(None, self.name)
    }
}

/// PHP deprecation metadata for an internal constant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstantDeprecation {
    message: &'static str,
}

impl ConstantDeprecation {
    /// Creates deprecation metadata with the PHP-visible diagnostic message.
    #[must_use]
    pub const fn new(message: &'static str) -> Self {
        Self { message }
    }

    /// PHP-visible diagnostic message.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

/// Registry-safe constant value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstantValue {
    /// PHP null constant.
    Null,
    /// PHP bool constant.
    Bool(bool),
    /// PHP int constant.
    Int(i64),
    /// PHP float constant.
    Float(FloatValue),
    /// PHP string constant.
    String(&'static str),
    /// PHP packed array constant.
    Array(&'static [ConstantValue]),
}

/// Descriptor for an internal class, interface, trait, or enum symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassDescriptor {
    name: &'static str,
    extension: &'static str,
    kind: ClassKind,
}

impl ClassDescriptor {
    /// Creates a class descriptor.
    #[must_use]
    pub const fn new(name: &'static str, extension: &'static str, kind: ClassKind) -> Self {
        Self {
            name,
            extension,
            kind,
        }
    }

    /// Stable class name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Owning extension name.
    #[must_use]
    pub const fn extension(&self) -> &'static str {
        self.extension
    }

    /// Class-like kind.
    #[must_use]
    pub const fn kind(&self) -> ClassKind {
        self.kind
    }

    /// Generated php-src stub metadata for this class-like symbol, when available.
    #[must_use]
    pub fn source_metadata(&self) -> Option<&'static generated::arginfo::GeneratedClassMetadata> {
        generated::arginfo::class_metadata(self.name)
    }
}

/// PHP class-like kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassKind {
    /// PHP class.
    Class,
    /// PHP interface.
    Interface,
    /// PHP trait.
    Trait,
    /// PHP enum.
    Enum,
}

/// Whether a symbol is PHP-visible or only present for tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolVisibility {
    /// Visible to PHP code once the owning extension is enabled.
    PhpVisible,
    /// Internal test-only descriptor; never listed as a public PHP function.
    InternalTestFixture,
}

/// Deterministic extension registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionRegistry {
    extensions: BTreeMap<&'static str, ExtensionDescriptor>,
    enabled: BTreeSet<&'static str>,
}

impl ExtensionRegistry {
    /// Creates a registry from descriptors.
    ///
    /// Names are stored in sorted maps. Functions and classes are sorted by
    /// name, while constants preserve extension registration order because
    /// PHP-visible APIs expose that order.
    #[must_use]
    pub fn from_extensions(extensions: impl IntoIterator<Item = ExtensionDescriptor>) -> Self {
        let mut map = BTreeMap::new();
        let mut enabled = BTreeSet::new();
        for mut extension in extensions {
            extension.sort_symbols();
            if extension.is_enabled_by_default() {
                enabled.insert(extension.name());
            }
            map.insert(extension.name(), extension);
        }
        Self {
            extensions: map,
            enabled,
        }
    }

    /// Returns the default standard-library infrastructure registry.
    ///
    /// Returns a shared static: the registry is immutable after construction
    /// and cloning it per call was a measurable per-compile cost.
    #[must_use]
    pub fn standard_library() -> &'static Self {
        static STANDARD_LIBRARY: OnceLock<ExtensionRegistry> = OnceLock::new();
        STANDARD_LIBRARY.get_or_init(Self::build_standard_library)
    }

    fn build_standard_library() -> Self {
        Self::from_extensions([
            standard_library_core_extension(),
            standard_library_standard_extension(),
            standard_library_json_extension(),
            standard_library_pcre_extension(),
            standard_library_session_extension(),
            standard_library_pdo_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_pdo_mysql_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_pdo_pgsql_extension(),
            standard_library_pdo_sqlite_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_pgsql_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_pcntl_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_posix_extension(),
            standard_library_readline_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_shmop_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_sysvmsg_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_sysvsem_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_sysvshm_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_mysqli_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_curl_extension(),
            #[cfg(not(target_family = "wasm"))]
            standard_library_openssl_extension(),
            standard_library_phar_extension(),
            standard_library_sqlite3_extension(),
            standard_library_mbstring_extension(),
            standard_library_intl_extension(),
            standard_library_xml_extension(),
            standard_library_dom_extension(),
            standard_library_simplexml_extension(),
            standard_library_xmlreader_extension(),
            standard_library_xmlwriter_extension(),
            standard_library_xsl_extension(),
            standard_library_hash_extension(),
            standard_library_gettext_extension(),
            standard_library_ctype_extension(),
            standard_library_calendar_extension(),
            standard_library_filter_extension(),
            standard_library_iconv_extension(),
            standard_library_sodium_extension(),
            standard_library_bcmath_extension(),
            standard_library_gmp_extension(),
            standard_library_apcu_extension(),
            standard_library_redis_extension(),
            standard_library_memcached_extension(),
            standard_library_imagick_extension(),
            standard_library_igbinary_extension(),
            standard_library_msgpack_extension(),
            standard_library_opcache_extension(),
            standard_library_soap_extension(),
            standard_library_ftp_extension(),
            standard_library_imap_extension(),
            standard_library_ldap_extension(),
            standard_library_ssh2_extension(),
            standard_library_sockets_extension(),
            standard_library_zlib_extension(),
            standard_library_zip_extension(),
            standard_library_fileinfo_extension(),
            standard_library_ffi_extension(),
            standard_library_exif_extension(),
            standard_library_gd_extension(),
            standard_library_random_extension(),
            standard_library_date_extension(),
            standard_library_spl_extension(),
            reflection_extension(),
            tokenizer_extension(),
            standard_library_test_extension(),
        ])
    }

    /// Returns extension descriptors in stable name order.
    pub fn extensions(&self) -> impl Iterator<Item = &ExtensionDescriptor> {
        self.extensions.values()
    }

    /// Looks up an extension descriptor.
    #[must_use]
    pub fn extension(&self, name: &str) -> Option<&ExtensionDescriptor> {
        self.extensions.get(name)
    }

    /// Looks up an extension case-insensitively.
    #[must_use]
    pub fn extension_case_insensitive(&self, name: &str) -> Option<&ExtensionDescriptor> {
        self.extensions
            .iter()
            .find(|(extension_name, _)| extension_name.eq_ignore_ascii_case(name))
            .map(|(_, extension)| extension)
    }

    /// Returns true when an extension exists and is enabled.
    #[must_use]
    pub fn is_extension_enabled(&self, name: &str) -> bool {
        self.enabled
            .iter()
            .any(|extension_name| extension_name.eq_ignore_ascii_case(name))
    }

    /// Enables an existing extension.
    pub fn enable_extension(&mut self, name: &'static str) -> Result<(), RegistryError> {
        if !self.extensions.contains_key(name) {
            return Err(RegistryError::UnknownExtension(name));
        }
        self.enabled.insert(name);
        Ok(())
    }

    /// Disables an existing extension.
    pub fn disable_extension(&mut self, name: &'static str) -> Result<(), RegistryError> {
        if !self.extensions.contains_key(name) {
            return Err(RegistryError::UnknownExtension(name));
        }
        self.enabled.remove(name);
        Ok(())
    }

    /// Returns PHP-visible enabled function descriptors in stable order.
    #[must_use]
    pub fn enabled_php_functions(&self) -> Vec<&FunctionDescriptor> {
        let mut functions = Vec::new();
        for extension_name in &self.enabled {
            let Some(extension) = self.extensions.get(extension_name) else {
                continue;
            };
            for function in extension.functions() {
                if function.visibility() == SymbolVisibility::PhpVisible {
                    functions.push(function);
                }
            }
        }
        functions.sort_by_key(|function| function.name());
        functions
    }

    /// Returns enabled constant descriptors in stable extension and
    /// registration order.
    #[must_use]
    pub fn enabled_constants(&self) -> Vec<&ConstantDescriptor> {
        let mut constants = Vec::new();
        for extension_name in &self.enabled {
            let Some(extension) = self.extensions.get(extension_name) else {
                continue;
            };
            constants.extend(extension.constants());
        }
        constants
    }

    /// Returns enabled extension names in stable order.
    #[must_use]
    pub fn enabled_extension_names(&self) -> Vec<&'static str> {
        self.enabled.iter().copied().collect()
    }

    /// Finds a PHP-visible function case-insensitively among enabled extensions.
    #[must_use]
    pub fn enabled_php_function(&self, name: &str) -> Option<&FunctionDescriptor> {
        self.enabled_php_functions()
            .into_iter()
            .find(|function| function.name().eq_ignore_ascii_case(name))
    }

    /// Finds an enabled class/interface/trait/enum case-insensitively.
    #[must_use]
    pub fn enabled_class(&self, name: &str) -> Option<&ClassDescriptor> {
        for extension_name in &self.enabled {
            let Some(extension) = self.extensions.get(extension_name) else {
                continue;
            };
            if let Some(class) = extension
                .classes()
                .iter()
                .find(|class| class.name().eq_ignore_ascii_case(name))
            {
                return Some(class);
            }
        }
        None
    }

    /// Finds an enabled constant by exact name.
    #[must_use]
    pub fn enabled_constant(&self, name: &str) -> Option<&ConstantDescriptor> {
        for extension_name in &self.enabled {
            let Some(extension) = self.extensions.get(extension_name) else {
                continue;
            };
            if let Some(constant) = extension
                .constants()
                .iter()
                .find(|item| item.name() == name)
            {
                return Some(constant);
            }
        }
        None
    }
}

/// Registry construction or mutation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// The requested extension name is not registered.
    UnknownExtension(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::api::{BuiltinCompatibility, BuiltinEntry, BuiltinRegistry};

    const FUNCTIONS_WITH_EXTERNAL_ARGINFO: &[&str] = &[
        "apcu_add",
        "apcu_cache_info",
        "apcu_clear_cache",
        "apcu_dec",
        "apcu_delete",
        "apcu_enabled",
        "apcu_exists",
        "apcu_fetch",
        "apcu_inc",
        "apcu_sma_info",
        "apcu_store",
        "igbinary_serialize",
        "igbinary_unserialize",
        "imap_8bit",
        "imap_alerts",
        "imap_append",
        "imap_base64",
        "imap_binary",
        "imap_check",
        "imap_close",
        "imap_delete",
        "imap_errors",
        "imap_expunge",
        "imap_fetch_overview",
        "imap_fetchbody",
        "imap_fetchheader",
        "imap_fetchstructure",
        "imap_gc",
        "imap_headerinfo",
        "imap_headers",
        "imap_last_error",
        "imap_list",
        "imap_listscan",
        "imap_mail_copy",
        "imap_mail_move",
        "imap_mailboxmsginfo",
        "imap_num_msg",
        "imap_num_recent",
        "imap_open",
        "imap_ping",
        "imap_qprint",
        "imap_reopen",
        "imap_search",
        "imap_sort",
        "imap_status",
        "imap_undelete",
        "imap_utf7_decode",
        "imap_utf7_encode",
        "imap_utf8",
        "msgpack_pack",
        "msgpack_serialize",
        "msgpack_unpack",
        "msgpack_unserialize",
        "print",
        "ssh2_auth_hostbased_file",
        "ssh2_auth_none",
        "ssh2_auth_password",
        "ssh2_auth_pubkey_file",
        "ssh2_connect",
        "ssh2_disconnect",
        "ssh2_exec",
        "ssh2_fingerprint",
        "ssh2_forward_accept",
        "ssh2_forward_listen",
        "ssh2_methods_negotiated",
        "ssh2_publickey_add",
        "ssh2_publickey_init",
        "ssh2_publickey_list",
        "ssh2_publickey_remove",
        "ssh2_scp_recv",
        "ssh2_scp_send",
        "ssh2_sftp",
        "ssh2_sftp_chmod",
        "ssh2_sftp_lstat",
        "ssh2_sftp_mkdir",
        "ssh2_sftp_readlink",
        "ssh2_sftp_realpath",
        "ssh2_sftp_rename",
        "ssh2_sftp_rmdir",
        "ssh2_sftp_stat",
        "ssh2_sftp_symlink",
        "ssh2_sftp_unlink",
        "ssh2_shell",
        "ssh2_tunnel",
    ];

    const CONSTANTS_WITH_EXTERNAL_ARGINFO: &[&str] = &[
        "CL_EXPUNGE",
        "CP_UID",
        "FT_INTERNAL",
        "FT_PEEK",
        "FT_PREFETCHTEXT",
        "FT_UID",
        "NIL",
        "OP_ANONYMOUS",
        "OP_DEBUG",
        "OP_EXPUNGE",
        "OP_HALFOPEN",
        "OP_READONLY",
        "SA_ALL",
        "SA_MESSAGES",
        "SA_RECENT",
        "SA_UIDNEXT",
        "SA_UIDVALIDITY",
        "SA_UNSEEN",
        "SE_UID",
        "SORTARRIVAL",
        "SORTCC",
        "SORTDATE",
        "SORTFROM",
        "SORTSIZE",
        "SORTSUBJECT",
        "SORTTO",
        "SSH2_DEFAULT_TERMINAL",
        "SSH2_DEFAULT_TERM_HEIGHT",
        "SSH2_DEFAULT_TERM_UNIT",
        "SSH2_DEFAULT_TERM_WIDTH",
        "SSH2_FINGERPRINT_HEX",
        "SSH2_FINGERPRINT_MD5",
        "SSH2_FINGERPRINT_RAW",
        "SSH2_FINGERPRINT_SHA1",
        "SSH2_TERM_UNIT_CHARS",
        "SSH2_TERM_UNIT_PIXELS",
        "ST_UID",
    ];

    #[test]
    fn registry_iteration_is_deterministic() {
        let registry = ExtensionRegistry::from_extensions([
            ExtensionDescriptor::new("zeta")
                .with_function(FunctionDescriptor::php("z_func", "zeta"))
                .with_function(FunctionDescriptor::php("a_func", "zeta")),
            ExtensionDescriptor::new("core")
                .with_constant(ConstantDescriptor::new("PHP_VERSION", "core"))
                .with_class(ClassDescriptor::new("Exception", "core", ClassKind::Class)),
        ]);

        let names: Vec<_> = registry
            .extensions()
            .map(ExtensionDescriptor::name)
            .collect();
        assert_eq!(names, ["core", "zeta"]);

        let zeta = registry.extension("zeta").expect("zeta extension");
        let function_names: Vec<_> = zeta
            .functions()
            .iter()
            .map(FunctionDescriptor::name)
            .collect();
        assert_eq!(function_names, ["a_func", "z_func"]);
    }

    #[test]
    fn extensions_can_be_enabled_and_disabled() {
        let mut registry = ExtensionRegistry::from_extensions([
            ExtensionDescriptor::new("core"),
            ExtensionDescriptor::new("json"),
        ]);

        assert!(registry.is_extension_enabled("core"));
        assert!(registry.is_extension_enabled("json"));
        registry.disable_extension("json").expect("disable json");
        assert!(!registry.is_extension_enabled("json"));
        registry.enable_extension("json").expect("enable json");
        assert!(registry.is_extension_enabled("json"));

        registry.disable_extension("core").expect("disable core");
        assert!(!registry.is_extension_enabled("core"));
    }

    #[test]
    fn bounded_mbstring_and_intl_are_enabled() {
        let registry = ExtensionRegistry::standard_library();

        assert!(registry.is_extension_enabled("mbstring"));
        assert!(registry.is_extension_enabled("intl"));

        for name in [
            "mb_check_encoding",
            "mb_convert_encoding",
            "mb_detect_encoding",
            "mb_internal_encoding",
            "mb_strlen",
            "mb_strtolower",
            "mb_strtoupper",
            "mb_substr",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be visible in the bounded mbstring MVP"
            );
        }

        for name in [
            "grapheme_strlen",
            "intl_get_error_code",
            "normalizer_normalize",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be visible in the bounded intl MVP"
            );
        }

        for name in [
            "Collator",
            "IntlChar",
            "Locale",
            "Normalizer",
            "NumberFormatter",
        ] {
            assert!(
                registry.enabled_class(name).is_some(),
                "{name} should be visible in the bounded intl MVP"
            );
        }
    }

    #[test]
    fn infrastructure_registry_exposes_no_php_visible_functions() {
        let mut registry = ExtensionRegistry::standard_library().clone();
        registry.enable_extension("test").expect("enable test");

        assert!(
            registry
                .enabled_php_function("__php_std_test_probe")
                .is_none()
        );
        let test = registry.extension("test").expect("test extension");
        assert_eq!(
            test.functions()[0].visibility(),
            SymbolVisibility::InternalTestFixture
        );
    }

    #[test]
    fn standard_registry_tracks_stdlib_encoding_hash_url_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "base64_decode",
            "base64_encode",
            "bin2hex",
            "chr",
            "crc32",
            "hex2bin",
            "get_html_translation_table",
            "html_entity_decode",
            "htmlspecialchars",
            "htmlspecialchars_decode",
            "htmlentities",
            "http_build_query",
            "md5",
            "ord",
            "parse_str",
            "parse_url",
            "rawurldecode",
            "rawurlencode",
            "sha1",
            "str_split",
            "urldecode",
            "urlencode",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_parse_url_component_constants() {
        let registry = ExtensionRegistry::standard_library();

        for (name, expected) in [
            ("PHP_QUERY_RFC1738", constants::PHP_QUERY_RFC1738),
            ("PHP_QUERY_RFC3986", constants::PHP_QUERY_RFC3986),
            ("PHP_URL_SCHEME", constants::PHP_URL_SCHEME),
            ("PHP_URL_HOST", constants::PHP_URL_HOST),
            ("PHP_URL_PORT", constants::PHP_URL_PORT),
            ("PHP_URL_USER", constants::PHP_URL_USER),
            ("PHP_URL_PASS", constants::PHP_URL_PASS),
            ("PHP_URL_PATH", constants::PHP_URL_PATH),
            ("PHP_URL_QUERY", constants::PHP_URL_QUERY),
            ("PHP_URL_FRAGMENT", constants::PHP_URL_FRAGMENT),
        ] {
            assert_eq!(
                registry
                    .enabled_constant(name)
                    .and_then(ConstantDescriptor::value),
                Some(ConstantValue::Int(expected)),
                "{name} should be registered with its PHP value"
            );
        }

        let php_url_names: Vec<_> = registry
            .enabled_constants()
            .into_iter()
            .map(ConstantDescriptor::name)
            .filter(|name| name.starts_with("PHP_URL_"))
            .collect();
        assert_eq!(
            php_url_names,
            [
                "PHP_URL_SCHEME",
                "PHP_URL_HOST",
                "PHP_URL_PORT",
                "PHP_URL_USER",
                "PHP_URL_PASS",
                "PHP_URL_PATH",
                "PHP_URL_QUERY",
                "PHP_URL_FRAGMENT",
            ]
        );
    }

    #[test]
    fn standard_registry_tracks_array_sort_and_filter_constants() {
        let registry = ExtensionRegistry::standard_library();

        for (name, expected) in [
            ("SORT_ASC", constants::SORT_ASC),
            ("SORT_DESC", constants::SORT_DESC),
            ("SORT_REGULAR", constants::SORT_REGULAR),
            ("SORT_NUMERIC", constants::SORT_NUMERIC),
            ("SORT_STRING", constants::SORT_STRING),
            ("SORT_LOCALE_STRING", constants::SORT_LOCALE_STRING),
            ("SORT_NATURAL", constants::SORT_NATURAL),
            ("SORT_FLAG_CASE", constants::SORT_FLAG_CASE),
            ("CASE_LOWER", constants::CASE_LOWER),
            ("CASE_UPPER", constants::CASE_UPPER),
            ("COUNT_NORMAL", constants::COUNT_NORMAL),
            ("COUNT_RECURSIVE", constants::COUNT_RECURSIVE),
            ("ARRAY_FILTER_USE_BOTH", constants::ARRAY_FILTER_USE_BOTH),
            ("ARRAY_FILTER_USE_KEY", constants::ARRAY_FILTER_USE_KEY),
        ] {
            assert_eq!(
                registry
                    .enabled_constant(name)
                    .and_then(ConstantDescriptor::value),
                Some(ConstantValue::Int(expected)),
                "{name} should be registered with its PHP value"
            );
        }
    }

    #[test]
    fn optional_hash_and_random_extensions_track_stdlib_symbols() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "hash",
            "hash_algos",
            "hash_copy",
            "hash_equals",
            "hash_file",
            "hash_final",
            "hash_hmac",
            "hash_hmac_algos",
            "hash_hmac_file",
            "hash_hkdf",
            "hash_init",
            "hash_pbkdf2",
            "hash_update",
            "hash_update_file",
            "hash_update_stream",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a hash function"
            );
        }
        assert!(registry.enabled_class("HashContext").is_some());
        assert_eq!(
            registry
                .enabled_constant("HASH_HMAC")
                .and_then(ConstantDescriptor::value),
            Some(ConstantValue::Int(1))
        );
        for name in ["random_bytes", "random_int"] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a random function"
            );
        }
        assert!(registry.is_extension_enabled("hash"));
        assert!(registry.is_extension_enabled("random"));
    }

    #[test]
    fn shmop_extension_tracks_functions_and_class() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "shmop_open",
            "shmop_read",
            "shmop_write",
            "shmop_size",
            "shmop_delete",
            "shmop_close",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a shmop function"
            );
        }
        assert!(registry.enabled_class("Shmop").is_some());
    }

    #[test]
    fn readline_extension_tracks_noninteractive_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "readline",
            "readline_info",
            "readline_add_history",
            "readline_clear_history",
            "readline_list_history",
            "readline_read_history",
            "readline_write_history",
            "readline_completion_function",
            "readline_callback_handler_install",
            "readline_callback_read_char",
            "readline_callback_handler_remove",
            "readline_redisplay",
            "readline_on_new_line",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a readline function"
            );
        }
        assert_eq!(
            registry
                .enabled_constant("READLINE_LIB")
                .and_then(ConstantDescriptor::value),
            Some(ConstantValue::String("phrust"))
        );
    }

    #[test]
    fn sysv_extensions_track_functions_classes_and_constants() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "msg_get_queue",
            "msg_send",
            "msg_receive",
            "msg_remove_queue",
            "msg_stat_queue",
            "msg_set_queue",
            "msg_queue_exists",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a sysvmsg function"
            );
        }
        for name in ["sem_get", "sem_acquire", "sem_release", "sem_remove"] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a sysvsem function"
            );
        }
        for name in [
            "shm_attach",
            "shm_detach",
            "shm_has_var",
            "shm_put_var",
            "shm_get_var",
            "shm_remove_var",
            "shm_remove",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a sysvshm function"
            );
        }

        assert!(registry.enabled_class("SysvMessageQueue").is_some());
        assert!(registry.enabled_class("SysvSemaphore").is_some());
        assert!(registry.enabled_class("SysvSharedMemory").is_some());
        assert_eq!(
            registry
                .enabled_constant("MSG_ENOMSG")
                .and_then(ConstantDescriptor::value),
            Some(ConstantValue::Int(libc::ENOMSG as i64))
        );
    }

    #[test]
    fn standard_registry_tracks_bounded_gd_image_capabilities() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "gd_info",
            "imagecopyresampled",
            "imagecreatefromjpeg",
            "imagecreatefrompng",
            "imagecreatefromstring",
            "imagecreatetruecolor",
            "imagetypes",
            "imagedestroy",
            "imagejpeg",
            "imagepng",
            "imagesx",
            "imagesy",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a GD function"
            );
        }

        for (name, expected) in [
            ("IMG_GIF", constants::IMG_GIF),
            ("IMG_JPG", constants::IMG_JPG),
            ("IMG_JPEG", constants::IMG_JPEG),
            ("IMG_PNG", constants::IMG_PNG),
            ("IMG_WEBP", constants::IMG_WEBP),
            ("IMG_AVIF", constants::IMG_AVIF),
        ] {
            assert_eq!(
                registry
                    .enabled_constant(name)
                    .and_then(ConstantDescriptor::value),
                Some(ConstantValue::Int(expected)),
                "{name} should be registered with its PHP GD bit value"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_formatting_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "addcslashes",
            "fprintf",
            "printf",
            "sprintf",
            "vprintf",
            "vsprintf",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_array_basic_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "array_all",
            "array_any",
            "array_chunk",
            "array_column",
            "array_diff_key",
            "array_filter",
            "array_fill",
            "array_find",
            "array_find_key",
            "array_flip",
            "array_is_list",
            "array_key_exists",
            "array_key_first",
            "array_key_last",
            "array_keys",
            "array_map",
            "array_merge",
            "array_merge_recursive",
            "array_pad",
            "array_pop",
            "array_push",
            "array_rand",
            "array_reduce",
            "array_replace",
            "array_replace_recursive",
            "array_reverse",
            "array_search",
            "array_shift",
            "array_slice",
            "array_splice",
            "array_unshift",
            "array_values",
            "array_walk",
            "array_walk_recursive",
            "arsort",
            "asort",
            "count",
            "in_array",
            "krsort",
            "ksort",
            "natcasesort",
            "natsort",
            "range",
            "rsort",
            "sizeof",
            "sort",
            "uasort",
            "uksort",
            "usort",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_math_numeric_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "abs",
            "ceil",
            "floor",
            "fdiv",
            "fmod",
            "intdiv",
            "is_finite",
            "is_infinite",
            "is_nan",
            "ignore_user_abort",
            "max",
            "min",
            "number_format",
            "pow",
            "round",
            "set_time_limit",
            "sqrt",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }

        assert_eq!(
            registry
                .enabled_class("RoundingMode")
                .map(ClassDescriptor::kind),
            Some(ClassKind::Enum)
        );
    }

    #[test]
    fn standard_registry_tracks_stdlib_symbol_introspection_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "define",
            "defined",
            "constant",
            "function_exists",
            "class_exists",
            "call_user_func",
            "call_user_func_array",
            "forward_static_call",
            "debug_backtrace",
            "debug_print_backtrace",
            "func_get_arg",
            "func_get_args",
            "func_num_args",
            "interface_exists",
            "trait_exists",
            "enum_exists",
            "method_exists",
            "property_exists",
            "is_a",
            "is_subclass_of",
            "get_called_class",
            "get_class",
            "get_class_methods",
            "get_class_vars",
            "get_parent_class",
            "get_declared_classes",
            "get_declared_interfaces",
            "get_declared_traits",
            "get_mangled_object_vars",
            "get_object_vars",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_ini_config_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in ["ini_get", "ini_set", "ini_get_all", "get_cfg_var"] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_platform_check_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "extension_loaded",
            "get_loaded_extensions",
            "ini_get",
            "defined",
            "constant",
            "class_exists",
            "function_exists",
            "hrtime",
            "phpversion",
            "version_compare",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a platform-check function"
            );
        }

        assert!(
            registry.enabled_constant("PHP_VERSION_ID").is_some(),
            "PHP_VERSION_ID should be registered as a platform-check constant"
        );
    }

    #[test]
    fn standard_registry_tracks_stdlib_process_surface_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "proc_open",
            "proc_close",
            "proc_get_status",
            "popen",
            "pclose",
            "shell_exec",
            "exec",
            "passthru",
            "system",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a process-surface function"
            );
        }
    }

    #[test]
    fn pcntl_extension_tracks_cli_process_control_symbols() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "pcntl_alarm",
            "pcntl_async_signals",
            "pcntl_exec",
            "pcntl_fork",
            "pcntl_signal",
            "pcntl_signal_dispatch",
            "pcntl_wait",
            "pcntl_waitpid",
            "pcntl_wexitstatus",
            "pcntl_wifexited",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a pcntl function"
            );
        }

        for name in ["SIG_DFL", "SIG_IGN", "SIGCHLD", "SIGUSR1", "WNOHANG"] {
            assert!(
                registry.enabled_constant(name).is_some(),
                "{name} should be registered as a pcntl constant"
            );
        }
    }

    #[test]
    fn ffi_extension_tracks_disabled_surface_metadata() {
        let registry = ExtensionRegistry::standard_library();

        let extension = registry.extension("ffi").expect("ffi extension");
        assert!(extension.enabled_by_default);

        for name in [
            "FFI",
            "FFI\\CData",
            "FFI\\CType",
            "FFI\\Exception",
            "FFI\\ParserException",
        ] {
            let class = registry.enabled_class(name).expect("ffi class");
            assert_eq!(class.extension, "ffi");
            assert_eq!(class.kind, ClassKind::Class);
        }

        for name in [
            "addr",
            "alignof",
            "arrayType",
            "cast",
            "cdef",
            "free",
            "isNull",
            "load",
            "memcmp",
            "memcpy",
            "memset",
            "new",
            "scope",
            "sizeof",
            "string",
            "type",
            "typeof",
        ] {
            let method =
                generated::arginfo::method_metadata("FFI", name).expect("ffi method metadata");
            assert_eq!(method.extension, "ffi");
            assert!(method.is_static);
        }
    }

    #[test]
    fn imagick_extension_tracks_backend_gated_surface_metadata() {
        let registry = ExtensionRegistry::standard_library();

        let extension = registry.extension("imagick").expect("imagick extension");
        assert!(extension.enabled_by_default);
        assert!(extension.functions().is_empty());
        assert!(extension.constants().is_empty());

        for name in [
            "Imagick",
            "ImagickDraw",
            "ImagickPixel",
            "ImagickPixelIterator",
            "ImagickException",
        ] {
            let class = registry.enabled_class(name).expect("imagick class");
            assert_eq!(class.extension, "imagick");
            assert_eq!(class.kind, ClassKind::Class);
            assert!(
                class.source_metadata().is_none(),
                "PECL Imagick classes must not pretend to have php-src arginfo"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_error_handling_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "error_log",
            "error_reporting",
            "set_error_handler",
            "restore_error_handler",
            "trigger_error",
            "user_error",
            "set_exception_handler",
            "restore_exception_handler",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }

        assert_eq!(
            registry
                .enabled_constant("E_USER_WARNING")
                .and_then(ConstantDescriptor::value),
            Some(ConstantValue::Int(constants::E_USER_WARNING))
        );
    }

    #[test]
    fn standard_registry_tracks_stdlib_output_buffering_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "ob_start",
            "ob_get_contents",
            "ob_get_clean",
            "ob_get_length",
            "ob_get_level",
            "ob_end_clean",
            "ob_end_flush",
            "flush",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_environment_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "getenv",
            "putenv",
            "php_sapi_name",
            "php_uname",
            "get_current_user",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_http_memory_and_password_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "header",
            "header_remove",
            "headers_list",
            "headers_sent",
            "http_response_code",
            "setcookie",
            "setrawcookie",
            "memory_get_usage",
            "memory_get_peak_usage",
            "password_hash",
            "password_verify",
            "password_needs_rehash",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }

        for (name, expected) in [
            ("PASSWORD_DEFAULT", constants::PASSWORD_DEFAULT),
            ("PASSWORD_BCRYPT", constants::PASSWORD_BCRYPT),
        ] {
            assert_eq!(
                registry
                    .enabled_constant(name)
                    .and_then(ConstantDescriptor::value),
                Some(ConstantValue::String(expected)),
                "{name} should be registered with its PHP value"
            );
        }

        assert_eq!(
            registry
                .enabled_constant("PASSWORD_BCRYPT_DEFAULT_COST")
                .and_then(ConstantDescriptor::value),
            Some(ConstantValue::Int(constants::PASSWORD_BCRYPT_DEFAULT_COST))
        );
    }

    #[test]
    fn standard_registry_tracks_stdlib_stream_resource_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in ["get_resource_id", "get_resource_type", "is_resource"] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_path_and_stat_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "basename",
            "dirname",
            "pathinfo",
            "realpath",
            "file_exists",
            "is_file",
            "is_dir",
            "is_link",
            "is_readable",
            "is_writable",
            "filesize",
            "filemtime",
            "fileperms",
            "fileowner",
            "filegroup",
            "filetype",
            "stat",
            "lstat",
            "chmod",
            "umask",
            "clearstatcache",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_wordpress_bootstrap_constants() {
        let registry = ExtensionRegistry::standard_library();

        for (name, expected) in [
            ("PHP_SAPI", ConstantValue::String(constants::PHP_SAPI)),
            ("PHP_BINARY", ConstantValue::String(constants::PHP_BINARY)),
            (
                "DEFAULT_INCLUDE_PATH",
                ConstantValue::String(constants::DEFAULT_INCLUDE_PATH),
            ),
            (
                "PHP_MAXPATHLEN",
                ConstantValue::Int(constants::PHP_MAXPATHLEN),
            ),
            (
                "DEBUG_BACKTRACE_PROVIDE_OBJECT",
                ConstantValue::Int(constants::DEBUG_BACKTRACE_PROVIDE_OBJECT),
            ),
            (
                "DEBUG_BACKTRACE_IGNORE_ARGS",
                ConstantValue::Int(constants::DEBUG_BACKTRACE_IGNORE_ARGS),
            ),
            ("FILE_APPEND", ConstantValue::Int(constants::FILE_APPEND)),
            ("LOCK_EX", ConstantValue::Int(constants::LOCK_EX)),
            ("ENT_QUOTES", ConstantValue::Int(constants::ENT_QUOTES)),
            (
                "HTML_SPECIALCHARS",
                ConstantValue::Int(constants::HTML_SPECIALCHARS),
            ),
            ("DATE_ATOM", ConstantValue::String(constants::DATE_ATOM)),
            (
                "DATE_RFC2822",
                ConstantValue::String(constants::DATE_RFC2822),
            ),
        ] {
            assert_eq!(
                registry
                    .enabled_constant(name)
                    .and_then(ConstantDescriptor::value),
                Some(expected),
                "{name} should be registered with its runtime value"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_runtime_constant_families() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "FILE_APPEND",
            "FILE_USE_INCLUDE_PATH",
            "FILE_IGNORE_NEW_LINES",
            "FILE_SKIP_EMPTY_LINES",
            "FILE_NO_DEFAULT_CONTEXT",
            "LOCK_SH",
            "LOCK_EX",
            "LOCK_UN",
            "LOCK_NB",
            "SEEK_SET",
            "SEEK_CUR",
            "SEEK_END",
            "GLOB_BRACE",
            "GLOB_MARK",
            "GLOB_NOSORT",
            "GLOB_NOCHECK",
            "GLOB_NOESCAPE",
            "GLOB_ERR",
            "GLOB_ONLYDIR",
            "PATHINFO_DIRNAME",
            "PATHINFO_BASENAME",
            "PATHINFO_EXTENSION",
            "PATHINFO_FILENAME",
            "INI_USER",
            "INI_PERDIR",
            "INI_SYSTEM",
            "INI_ALL",
            "INI_SCANNER_NORMAL",
            "INI_SCANNER_RAW",
            "INI_SCANNER_TYPED",
            "FNM_NOESCAPE",
            "FNM_PATHNAME",
            "FNM_PERIOD",
            "FNM_CASEFOLD",
            "HTML_SPECIALCHARS",
            "HTML_ENTITIES",
            "ENT_COMPAT",
            "ENT_QUOTES",
            "ENT_NOQUOTES",
            "ENT_IGNORE",
            "ENT_SUBSTITUTE",
            "ENT_DISALLOWED",
            "ENT_HTML401",
            "ENT_XML1",
            "ENT_XHTML",
            "ENT_HTML5",
            "CHAR_MAX",
        ] {
            assert!(
                registry.enabled_constant(name).is_some(),
                "{name} should be registered as a standard runtime constant"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_file_io_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "fopen",
            "fclose",
            "fread",
            "fwrite",
            "fgets",
            "fgetc",
            "feof",
            "fflush",
            "fseek",
            "ftell",
            "rewind",
            "file_get_contents",
            "file_put_contents",
            "readfile",
            "copy",
            "rename",
            "unlink",
            "mkdir",
            "rmdir",
            "touch",
            "tempnam",
            "tmpfile",
            "sys_get_temp_dir",
            "disk_free_space",
            "disk_total_space",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_directory_glob_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "opendir",
            "readdir",
            "rewinddir",
            "closedir",
            "scandir",
            "dir",
            "glob",
            "getcwd",
            "chdir",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn standard_registry_tracks_stdlib_stream_context_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "stream_get_wrappers",
            "stream_get_meta_data",
            "stream_get_contents",
            "stream_copy_to_stream",
            "stream_context_create",
            "stream_context_get_default",
            "stream_context_get_options",
            "stream_context_set_default",
            "stream_context_set_option",
            "stream_resolve_include_path",
            "stream_is_local",
            "stream_isatty",
            "stream_set_timeout",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
            );
        }
    }

    #[test]
    fn json_extension_tracks_stdlib_symbols() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "json_decode",
            "json_encode",
            "json_last_error",
            "json_last_error_msg",
            "json_validate",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a json function"
            );
        }
        for name in [
            "JSON_BIGINT_AS_STRING",
            "JSON_HEX_TAG",
            "JSON_HEX_AMP",
            "JSON_HEX_APOS",
            "JSON_HEX_QUOT",
            "JSON_FORCE_OBJECT",
            "JSON_NUMERIC_CHECK",
            "JSON_PRETTY_PRINT",
            "JSON_UNESCAPED_SLASHES",
            "JSON_UNESCAPED_UNICODE",
            "JSON_PARTIAL_OUTPUT_ON_ERROR",
            "JSON_PRESERVE_ZERO_FRACTION",
            "JSON_UNESCAPED_LINE_TERMINATORS",
            "JSON_INVALID_UTF8_IGNORE",
            "JSON_INVALID_UTF8_SUBSTITUTE",
            "JSON_OBJECT_AS_ARRAY",
            "JSON_ERROR_NONE",
            "JSON_ERROR_DEPTH",
            "JSON_ERROR_STATE_MISMATCH",
            "JSON_ERROR_CTRL_CHAR",
            "JSON_ERROR_SYNTAX",
            "JSON_ERROR_UTF8",
            "JSON_ERROR_RECURSION",
            "JSON_ERROR_INF_OR_NAN",
            "JSON_ERROR_UNSUPPORTED_TYPE",
            "JSON_ERROR_INVALID_PROPERTY_NAME",
            "JSON_ERROR_UTF16",
            "JSON_THROW_ON_ERROR",
        ] {
            assert!(
                registry.enabled_constant(name).is_some(),
                "{name} should be registered as a json constant"
            );
        }
        assert!(matches!(
            registry
                .enabled_class("JsonException")
                .map(ClassDescriptor::kind),
            Some(ClassKind::Class)
        ));
        assert!(matches!(
            registry
                .enabled_class("JsonSerializable")
                .map(ClassDescriptor::kind),
            Some(ClassKind::Interface)
        ));
    }

    #[test]
    fn pcre_extension_tracks_stdlib_symbols() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "preg_filter",
            "preg_grep",
            "preg_last_error",
            "preg_last_error_msg",
            "preg_match",
            "preg_match_all",
            "preg_quote",
            "preg_replace",
            "preg_replace_callback",
            "preg_replace_callback_array",
            "preg_split",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a pcre function"
            );
        }
        for name in [
            "PREG_NO_ERROR",
            "PREG_OFFSET_CAPTURE",
            "PREG_PATTERN_ORDER",
            "PREG_SET_ORDER",
            "PREG_SPLIT_NO_EMPTY",
            "PREG_SPLIT_DELIM_CAPTURE",
            "PREG_SPLIT_OFFSET_CAPTURE",
            "PREG_GREP_INVERT",
            "PREG_UNMATCHED_AS_NULL",
            "PREG_INTERNAL_ERROR",
            "PREG_BACKTRACK_LIMIT_ERROR",
            "PREG_RECURSION_LIMIT_ERROR",
            "PREG_BAD_UTF8_ERROR",
            "PREG_BAD_UTF8_OFFSET_ERROR",
            "PREG_JIT_STACKLIMIT_ERROR",
        ] {
            assert!(
                registry.enabled_constant(name).is_some(),
                "{name} should be registered as a pcre constant"
            );
        }
    }

    #[test]
    fn date_extension_tracks_stdlib_timezone_symbols() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "date",
            "date_default_timezone_get",
            "date_default_timezone_set",
            "strtotime",
            "time",
            "timezone_identifiers_list",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a date function"
            );
        }
        for name in [
            "DateInterval",
            "DateTime",
            "DateTimeImmutable",
            "DateTimeZone",
        ] {
            assert!(matches!(
                registry.enabled_class(name).map(ClassDescriptor::kind),
                Some(ClassKind::Class)
            ));
        }
        assert!(matches!(
            registry
                .enabled_class("DateTimeInterface")
                .map(ClassDescriptor::kind),
            Some(ClassKind::Interface)
        ));
        for name in [
            "DATE_ATOM",
            "DATE_COOKIE",
            "DATE_ISO8601",
            "DATE_ISO8601_EXPANDED",
            "DATE_RFC1036",
            "DATE_RFC1123",
            "DATE_RFC2822",
            "DATE_RFC3339",
            "DATE_RFC3339_EXTENDED",
            "DATE_RFC7231",
            "DATE_RFC822",
            "DATE_RFC850",
            "DATE_RSS",
            "DATE_W3C",
        ] {
            assert!(
                registry.enabled_constant(name).is_some(),
                "{name} should be registered as a date constant"
            );
        }
    }

    #[test]
    fn filter_extension_tracks_option_constants_for_registered_builtins() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "filter_has_var",
            "filter_input",
            "filter_input_array",
            "filter_var",
            "filter_var_array",
            "filter_list",
            "filter_id",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a filter function"
            );
        }
        for name in [
            "INPUT_POST",
            "INPUT_GET",
            "INPUT_COOKIE",
            "INPUT_ENV",
            "INPUT_SERVER",
            "FILTER_DEFAULT",
            "FILTER_UNSAFE_RAW",
            "FILTER_FLAG_NONE",
            "FILTER_REQUIRE_ARRAY",
            "FILTER_REQUIRE_SCALAR",
            "FILTER_FORCE_ARRAY",
            "FILTER_VALIDATE_BOOL",
            "FILTER_VALIDATE_BOOLEAN",
            "FILTER_VALIDATE_INT",
            "FILTER_VALIDATE_FLOAT",
            "FILTER_VALIDATE_REGEXP",
            "FILTER_VALIDATE_URL",
            "FILTER_VALIDATE_EMAIL",
            "FILTER_VALIDATE_IP",
            "FILTER_VALIDATE_MAC",
            "FILTER_VALIDATE_DOMAIN",
            "FILTER_SANITIZE_STRING",
            "FILTER_SANITIZE_STRIPPED",
            "FILTER_SANITIZE_ENCODED",
            "FILTER_SANITIZE_SPECIAL_CHARS",
            "FILTER_SANITIZE_EMAIL",
            "FILTER_SANITIZE_URL",
            "FILTER_SANITIZE_NUMBER_INT",
            "FILTER_SANITIZE_NUMBER_FLOAT",
            "FILTER_SANITIZE_FULL_SPECIAL_CHARS",
            "FILTER_SANITIZE_ADD_SLASHES",
            "FILTER_NULL_ON_FAILURE",
            "FILTER_FLAG_ALLOW_OCTAL",
            "FILTER_FLAG_ALLOW_HEX",
            "FILTER_FLAG_STRIP_LOW",
            "FILTER_FLAG_STRIP_HIGH",
            "FILTER_FLAG_STRIP_BACKTICK",
            "FILTER_FLAG_ENCODE_LOW",
            "FILTER_FLAG_ENCODE_HIGH",
            "FILTER_FLAG_ENCODE_AMP",
            "FILTER_FLAG_NO_ENCODE_QUOTES",
            "FILTER_FLAG_EMPTY_STRING_NULL",
            "FILTER_FLAG_ALLOW_FRACTION",
            "FILTER_FLAG_ALLOW_THOUSAND",
            "FILTER_FLAG_ALLOW_SCIENTIFIC",
            "FILTER_FLAG_IPV4",
            "FILTER_FLAG_IPV6",
            "FILTER_FLAG_NO_RES_RANGE",
            "FILTER_FLAG_NO_PRIV_RANGE",
            "FILTER_FLAG_GLOBAL_RANGE",
            "FILTER_FLAG_HOSTNAME",
            "FILTER_FLAG_EMAIL_UNICODE",
            "FILTER_FLAG_PATH_REQUIRED",
            "FILTER_FLAG_QUERY_REQUIRED",
        ] {
            assert!(
                registry.enabled_constant(name).is_some(),
                "{name} should be registered as a filter constant"
            );
        }
    }

    #[test]
    fn session_extension_tracks_state_constants_for_registered_builtins() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
            "session_destroy",
            "session_id",
            "session_name",
            "session_start",
            "session_status",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a session function"
            );
        }
        for name in [
            "PHP_SESSION_DISABLED",
            "PHP_SESSION_NONE",
            "PHP_SESSION_ACTIVE",
        ] {
            assert!(
                registry.enabled_constant(name).is_some(),
                "{name} should be registered as a session constant"
            );
        }
    }

    #[test]
    fn spl_extension_tracks_stdlib_basis_symbols() {
        let registry = ExtensionRegistry::standard_library();

        assert!(registry.is_extension_enabled("spl"));
        for name in [
            "spl_autoload_call",
            "spl_autoload_functions",
            "spl_autoload_register",
            "spl_autoload_unregister",
            "spl_object_hash",
            "spl_object_id",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as an spl function"
            );
        }
        for name in [
            "ArrayAccess",
            "Countable",
            "Iterator",
            "IteratorAggregate",
            "RecursiveIterator",
            "SeekableIterator",
            "Serializable",
            "Traversable",
        ] {
            assert!(matches!(
                registry.enabled_class(name).map(ClassDescriptor::kind),
                Some(ClassKind::Interface)
            ));
        }
        for name in [
            "AppendIterator",
            "ArrayIterator",
            "ArrayObject",
            "BadFunctionCallException",
            "BadMethodCallException",
            "DomainException",
            "EmptyIterator",
            "InvalidArgumentException",
            "IteratorIterator",
            "LengthException",
            "LimitIterator",
            "LogicException",
            "OutOfBoundsException",
            "OutOfRangeException",
            "OverflowException",
            "RangeException",
            "RecursiveArrayIterator",
            "RuntimeException",
            "SplDoublyLinkedList",
            "SplFileInfo",
            "SplFileObject",
            "SplFixedArray",
            "SplObjectStorage",
            "SplQueue",
            "SplStack",
            "SplTempFileObject",
            "UnderflowException",
            "UnexpectedValueException",
        ] {
            assert!(matches!(
                registry.enabled_class(name).map(ClassDescriptor::kind),
                Some(ClassKind::Class)
            ));
        }
    }

    #[test]
    fn reflection_extension_tracks_generated_arginfo_classes() {
        let registry = ExtensionRegistry::standard_library();

        assert!(registry.is_extension_enabled("reflection"));
        for name in [
            "ReflectionAttribute",
            "ReflectionClass",
            "ReflectionEnum",
            "ReflectionExtension",
            "ReflectionFunction",
            "ReflectionMethod",
            "ReflectionParameter",
            "ReflectionProperty",
        ] {
            assert!(matches!(
                registry.enabled_class(name).map(ClassDescriptor::kind),
                Some(ClassKind::Class)
            ));
        }
        assert!(matches!(
            registry
                .enabled_class("Reflector")
                .map(ClassDescriptor::kind),
            Some(ClassKind::Interface)
        ));
    }

    #[test]
    fn registered_extensions_import_generated_arginfo_classlikes() {
        let registry = ExtensionRegistry::standard_library();

        for (name, kind) in [
            ("ArgumentCountError", ClassKind::Class),
            ("ErrorException", ClassKind::Class),
            ("RecursiveRegexIterator", ClassKind::Class),
            ("SplPriorityQueue", ClassKind::Class),
            ("SplSubject", ClassKind::Interface),
            ("Transliterator", ClassKind::Class),
            ("Random\\Engine\\Mt19937", ClassKind::Class),
        ] {
            let class = registry
                .enabled_class(name)
                .unwrap_or_else(|| panic!("{name} should be registered from generated arginfo"));
            assert_eq!(class.kind(), kind, "{name} should use generated kind");
            assert!(
                class.source_metadata().is_some(),
                "{name} should keep php-src stub provenance"
            );
        }

        assert!(
            registry.enabled_class("_ZendTestClass").is_none(),
            "php-src test fixtures must not be enabled by default"
        );
    }

    #[test]
    fn visible_stdlib_functions_have_generated_arginfo() {
        let registry = ExtensionRegistry::standard_library();
        let mut missing = registry
            .extensions()
            .flat_map(ExtensionDescriptor::functions)
            .filter(|function| function.visibility() == SymbolVisibility::PhpVisible)
            .filter(|function| function.arginfo().is_none())
            .map(FunctionDescriptor::name)
            .collect::<Vec<_>>();
        missing.sort_unstable();

        assert_eq!(
            missing, FUNCTIONS_WITH_EXTERNAL_ARGINFO,
            "`print` is a PHP language construct; external extension slices without pinned php-src stubs must be explicitly audited here; visible function descriptors should otherwise have generated php-src arginfo"
        );
    }

    #[test]
    fn visible_stdlib_constants_have_generated_metadata_or_platform_note() {
        let registry = ExtensionRegistry::standard_library();
        let mut missing = registry
            .enabled_constants()
            .into_iter()
            .filter(|constant| constant.source_metadata().is_none())
            .map(ConstantDescriptor::name)
            .collect::<Vec<_>>();
        missing.sort_unstable();

        assert_eq!(
            missing, CONSTANTS_WITH_EXTERNAL_ARGINFO,
            "registered constants should stay backed by generated php-src metadata unless their external extension slice is explicitly audited here"
        );
    }

    #[test]
    fn deprecated_filter_string_constants_keep_oracle_messages() {
        let registry = ExtensionRegistry::standard_library();

        for (name, message) in [
            (
                "FILTER_SANITIZE_STRING",
                "Constant FILTER_SANITIZE_STRING is deprecated since 8.1, use htmlspecialchars() instead",
            ),
            (
                "FILTER_SANITIZE_STRIPPED",
                "Constant FILTER_SANITIZE_STRIPPED is deprecated since 8.1, use htmlspecialchars() instead",
            ),
        ] {
            let constant = registry
                .enabled_constant(name)
                .unwrap_or_else(|| panic!("{name} should be registered"));
            assert_eq!(
                constant
                    .deprecation()
                    .map(|deprecation| deprecation.message()),
                Some(message)
            );
        }
    }

    #[test]
    fn runtime_builtin_registry_entries_have_generated_arginfo() {
        let mut missing = BuiltinRegistry::new()
            .entries()
            .iter()
            .copied()
            .filter(|entry| entry.compatibility() == BuiltinCompatibility::Php)
            .filter(|entry| generated::arginfo::function_metadata(entry.name()).is_none())
            .map(BuiltinEntry::name)
            .collect::<Vec<_>>();
        missing.sort_unstable();

        assert_eq!(
            missing, FUNCTIONS_WITH_EXTERNAL_ARGINFO,
            "`print` is a PHP language construct; external extension slices without pinned php-src stubs must be explicitly audited here; all function builtins should otherwise have generated php-src arginfo"
        );
    }

    #[test]
    fn unknown_extension_mutation_is_rejected() {
        let mut registry = ExtensionRegistry::standard_library().clone();
        assert_eq!(
            registry.enable_extension("missing"),
            Err(RegistryError::UnknownExtension("missing"))
        );
    }
}
