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

    /// Constant descriptors in stable name order.
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
        self.constants.sort_by_key(ConstantDescriptor::name);
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
}

impl ConstantDescriptor {
    /// Creates a constant descriptor.
    #[must_use]
    pub const fn new(name: &'static str, extension: &'static str) -> Self {
        Self {
            name,
            extension,
            value: None,
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
        }
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

    /// Generated php-src stub metadata for this constant, when available.
    #[must_use]
    pub fn source_metadata(
        &self,
    ) -> Option<&'static generated::arginfo::GeneratedConstantMetadata> {
        generated::arginfo::constant_metadata(None, self.name)
    }
}

/// Registry-safe constant value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstantValue {
    /// PHP bool constant.
    Bool(bool),
    /// PHP int constant.
    Int(i64),
    /// PHP float constant.
    Float(php_runtime::FloatValue),
    /// PHP string constant.
    String(&'static str),
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
    /// Names are stored in sorted maps, and every descriptor's local symbol
    /// lists are sorted, so iteration order is stable across platforms.
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
    #[must_use]
    pub fn standard_library() -> Self {
        static STANDARD_LIBRARY: OnceLock<ExtensionRegistry> = OnceLock::new();
        STANDARD_LIBRARY
            .get_or_init(Self::build_standard_library)
            .clone()
    }

    fn build_standard_library() -> Self {
        Self::from_extensions([
            ExtensionDescriptor::new("core")
                .with_class(ClassDescriptor::new("Closure", "core", ClassKind::Class))
                .with_class(ClassDescriptor::new("stdClass", "core", ClassKind::Class))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_VERSION",
                    "core",
                    ConstantValue::String(constants::PHP_VERSION),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_VERSION_ID",
                    "core",
                    ConstantValue::Int(constants::PHP_VERSION_ID),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_MAJOR_VERSION",
                    "core",
                    ConstantValue::Int(constants::PHP_MAJOR_VERSION),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_MINOR_VERSION",
                    "core",
                    ConstantValue::Int(constants::PHP_MINOR_VERSION),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_RELEASE_VERSION",
                    "core",
                    ConstantValue::Int(constants::PHP_RELEASE_VERSION),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_INT_MAX",
                    "core",
                    ConstantValue::Int(constants::PHP_INT_MAX),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_INT_MIN",
                    "core",
                    ConstantValue::Int(constants::PHP_INT_MIN),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_INT_SIZE",
                    "core",
                    ConstantValue::Int(constants::PHP_INT_SIZE),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "INF",
                    "core",
                    ConstantValue::Float(constants::INF),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "NAN",
                    "core",
                    ConstantValue::Float(constants::NAN),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "DIRECTORY_SEPARATOR",
                    "core",
                    ConstantValue::String(constants::DIRECTORY_SEPARATOR),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PATH_SEPARATOR",
                    "core",
                    ConstantValue::String(constants::PATH_SEPARATOR),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_OS",
                    "core",
                    ConstantValue::String(constants::PHP_OS),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_OS_FAMILY",
                    "core",
                    ConstantValue::String(constants::PHP_OS_FAMILY),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_EOL",
                    "core",
                    ConstantValue::String(constants::PHP_EOL),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_ERROR",
                    "core",
                    ConstantValue::Int(constants::E_ERROR),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_WARNING",
                    "core",
                    ConstantValue::Int(constants::E_WARNING),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_PARSE",
                    "core",
                    ConstantValue::Int(constants::E_PARSE),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_NOTICE",
                    "core",
                    ConstantValue::Int(constants::E_NOTICE),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_CORE_ERROR",
                    "core",
                    ConstantValue::Int(constants::E_CORE_ERROR),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_CORE_WARNING",
                    "core",
                    ConstantValue::Int(constants::E_CORE_WARNING),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_COMPILE_ERROR",
                    "core",
                    ConstantValue::Int(constants::E_COMPILE_ERROR),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_COMPILE_WARNING",
                    "core",
                    ConstantValue::Int(constants::E_COMPILE_WARNING),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_USER_ERROR",
                    "core",
                    ConstantValue::Int(constants::E_USER_ERROR),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_USER_WARNING",
                    "core",
                    ConstantValue::Int(constants::E_USER_WARNING),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_USER_NOTICE",
                    "core",
                    ConstantValue::Int(constants::E_USER_NOTICE),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_STRICT",
                    "core",
                    ConstantValue::Int(constants::E_STRICT),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_RECOVERABLE_ERROR",
                    "core",
                    ConstantValue::Int(constants::E_RECOVERABLE_ERROR),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_DEPRECATED",
                    "core",
                    ConstantValue::Int(constants::E_DEPRECATED),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_USER_DEPRECATED",
                    "core",
                    ConstantValue::Int(constants::E_USER_DEPRECATED),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "E_ALL",
                    "core",
                    ConstantValue::Int(constants::E_ALL),
                )),
            ExtensionDescriptor::new("standard")
                .with_constant(ConstantDescriptor::with_value(
                    "SORT_ASC",
                    "standard",
                    ConstantValue::Int(constants::SORT_ASC),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SORT_DESC",
                    "standard",
                    ConstantValue::Int(constants::SORT_DESC),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SORT_REGULAR",
                    "standard",
                    ConstantValue::Int(constants::SORT_REGULAR),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SORT_NUMERIC",
                    "standard",
                    ConstantValue::Int(constants::SORT_NUMERIC),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SORT_STRING",
                    "standard",
                    ConstantValue::Int(constants::SORT_STRING),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SORT_LOCALE_STRING",
                    "standard",
                    ConstantValue::Int(constants::SORT_LOCALE_STRING),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SORT_NATURAL",
                    "standard",
                    ConstantValue::Int(constants::SORT_NATURAL),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SORT_FLAG_CASE",
                    "standard",
                    ConstantValue::Int(constants::SORT_FLAG_CASE),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "LC_ALL",
                    "standard",
                    ConstantValue::Int(constants::LC_ALL),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "LC_CTYPE",
                    "standard",
                    ConstantValue::Int(constants::LC_CTYPE),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "LC_NUMERIC",
                    "standard",
                    ConstantValue::Int(constants::LC_NUMERIC),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "LC_TIME",
                    "standard",
                    ConstantValue::Int(constants::LC_TIME),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "LC_COLLATE",
                    "standard",
                    ConstantValue::Int(constants::LC_COLLATE),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "LC_MONETARY",
                    "standard",
                    ConstantValue::Int(constants::LC_MONETARY),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "LC_MESSAGES",
                    "standard",
                    ConstantValue::Int(constants::LC_MESSAGES),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CASE_LOWER",
                    "standard",
                    ConstantValue::Int(constants::CASE_LOWER),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CASE_UPPER",
                    "standard",
                    ConstantValue::Int(constants::CASE_UPPER),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "COUNT_NORMAL",
                    "standard",
                    ConstantValue::Int(constants::COUNT_NORMAL),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "COUNT_RECURSIVE",
                    "standard",
                    ConstantValue::Int(constants::COUNT_RECURSIVE),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "ARRAY_FILTER_USE_BOTH",
                    "standard",
                    ConstantValue::Int(constants::ARRAY_FILTER_USE_BOTH),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "ARRAY_FILTER_USE_KEY",
                    "standard",
                    ConstantValue::Int(constants::ARRAY_FILTER_USE_KEY),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "STR_PAD_LEFT",
                    "standard",
                    ConstantValue::Int(constants::STR_PAD_LEFT),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "STR_PAD_RIGHT",
                    "standard",
                    ConstantValue::Int(constants::STR_PAD_RIGHT),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "STR_PAD_BOTH",
                    "standard",
                    ConstantValue::Int(constants::STR_PAD_BOTH),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_E",
                    "standard",
                    ConstantValue::Float(constants::M_E),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_LOG2E",
                    "standard",
                    ConstantValue::Float(constants::M_LOG2E),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_LOG10E",
                    "standard",
                    ConstantValue::Float(constants::M_LOG10E),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_LN2",
                    "standard",
                    ConstantValue::Float(constants::M_LN2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_LN10",
                    "standard",
                    ConstantValue::Float(constants::M_LN10),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_PI",
                    "standard",
                    ConstantValue::Float(constants::M_PI),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_PI_2",
                    "standard",
                    ConstantValue::Float(constants::M_PI_2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_PI_4",
                    "standard",
                    ConstantValue::Float(constants::M_PI_4),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_1_PI",
                    "standard",
                    ConstantValue::Float(constants::M_1_PI),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_2_PI",
                    "standard",
                    ConstantValue::Float(constants::M_2_PI),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_SQRTPI",
                    "standard",
                    ConstantValue::Float(constants::M_SQRTPI),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_2_SQRTPI",
                    "standard",
                    ConstantValue::Float(constants::M_2_SQRTPI),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_LNPI",
                    "standard",
                    ConstantValue::Float(constants::M_LNPI),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_EULER",
                    "standard",
                    ConstantValue::Float(constants::M_EULER),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_SQRT2",
                    "standard",
                    ConstantValue::Float(constants::M_SQRT2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_SQRT1_2",
                    "standard",
                    ConstantValue::Float(constants::M_SQRT1_2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "M_SQRT3",
                    "standard",
                    ConstantValue::Float(constants::M_SQRT3),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_ROUND_HALF_UP",
                    "standard",
                    ConstantValue::Int(constants::PHP_ROUND_HALF_UP),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_ROUND_HALF_DOWN",
                    "standard",
                    ConstantValue::Int(constants::PHP_ROUND_HALF_DOWN),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_ROUND_HALF_EVEN",
                    "standard",
                    ConstantValue::Int(constants::PHP_ROUND_HALF_EVEN),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_ROUND_HALF_ODD",
                    "standard",
                    ConstantValue::Int(constants::PHP_ROUND_HALF_ODD),
                ))
                .with_class(ClassDescriptor::new(
                    "RoundingMode",
                    "standard",
                    ClassKind::Enum,
                ))
                .with_function(FunctionDescriptor::php("abs", "standard"))
                .with_function(FunctionDescriptor::php("acos", "standard"))
                .with_function(FunctionDescriptor::php("acosh", "standard"))
                .with_function(FunctionDescriptor::php("array_all", "standard"))
                .with_function(FunctionDescriptor::php("array_any", "standard"))
                .with_function(FunctionDescriptor::php("array_chunk", "standard"))
                .with_function(FunctionDescriptor::php("array_column", "standard"))
                .with_function(FunctionDescriptor::php("array_filter", "standard"))
                .with_function(FunctionDescriptor::php("array_fill", "standard"))
                .with_function(FunctionDescriptor::php("array_find", "standard"))
                .with_function(FunctionDescriptor::php("array_find_key", "standard"))
                .with_function(FunctionDescriptor::php("array_flip", "standard"))
                .with_function(FunctionDescriptor::php("array_is_list", "standard"))
                .with_function(FunctionDescriptor::php("array_key_exists", "standard"))
                .with_function(FunctionDescriptor::php("array_key_first", "standard"))
                .with_function(FunctionDescriptor::php("array_key_last", "standard"))
                .with_function(FunctionDescriptor::php("array_keys", "standard"))
                .with_function(FunctionDescriptor::php("array_map", "standard"))
                .with_function(FunctionDescriptor::php("array_merge", "standard"))
                .with_function(FunctionDescriptor::php("array_merge_recursive", "standard"))
                .with_function(FunctionDescriptor::php("array_pad", "standard"))
                .with_function(FunctionDescriptor::php("array_pop", "standard"))
                .with_function(FunctionDescriptor::php("array_push", "standard"))
                .with_function(FunctionDescriptor::php("array_rand", "standard"))
                .with_function(FunctionDescriptor::php("array_reduce", "standard"))
                .with_function(FunctionDescriptor::php("array_replace", "standard"))
                .with_function(FunctionDescriptor::php(
                    "array_replace_recursive",
                    "standard",
                ))
                .with_function(FunctionDescriptor::php("array_reverse", "standard"))
                .with_function(FunctionDescriptor::php("array_search", "standard"))
                .with_function(FunctionDescriptor::php("array_shift", "standard"))
                .with_function(FunctionDescriptor::php("array_slice", "standard"))
                .with_function(FunctionDescriptor::php("array_splice", "standard"))
                .with_function(FunctionDescriptor::php("array_unshift", "standard"))
                .with_function(FunctionDescriptor::php("array_values", "standard"))
                .with_function(FunctionDescriptor::php("array_walk", "standard"))
                .with_function(FunctionDescriptor::php("array_walk_recursive", "standard"))
                .with_function(FunctionDescriptor::php("arsort", "standard"))
                .with_function(FunctionDescriptor::php("asin", "standard"))
                .with_function(FunctionDescriptor::php("asinh", "standard"))
                .with_function(FunctionDescriptor::php("asort", "standard"))
                .with_function(FunctionDescriptor::php("atan", "standard"))
                .with_function(FunctionDescriptor::php("atan2", "standard"))
                .with_function(FunctionDescriptor::php("atanh", "standard"))
                .with_function(FunctionDescriptor::php("base64_decode", "standard"))
                .with_function(FunctionDescriptor::php("base64_encode", "standard"))
                .with_function(FunctionDescriptor::php("base_convert", "standard"))
                .with_function(FunctionDescriptor::php("basename", "standard"))
                .with_function(FunctionDescriptor::php("bin2hex", "standard"))
                .with_function(FunctionDescriptor::php("bindec", "standard"))
                .with_function(FunctionDescriptor::php("boolval", "standard"))
                .with_function(FunctionDescriptor::php("ceil", "standard"))
                .with_function(FunctionDescriptor::php("chdir", "standard"))
                .with_function(FunctionDescriptor::php("chr", "standard"))
                .with_function(FunctionDescriptor::php("class_exists", "standard"))
                .with_function(FunctionDescriptor::php("call_user_func", "standard"))
                .with_function(FunctionDescriptor::php("call_user_func_array", "standard"))
                .with_function(FunctionDescriptor::php("clearstatcache", "standard"))
                .with_function(FunctionDescriptor::php("closedir", "standard"))
                .with_function(FunctionDescriptor::php("constant", "standard"))
                .with_function(FunctionDescriptor::php("copy", "standard"))
                .with_function(FunctionDescriptor::php("cos", "standard"))
                .with_function(FunctionDescriptor::php("cosh", "standard"))
                .with_function(FunctionDescriptor::php("count", "standard"))
                .with_function(FunctionDescriptor::php("crc32", "standard"))
                .with_function(FunctionDescriptor::php("debug_backtrace", "standard"))
                .with_function(FunctionDescriptor::php("debug_print_backtrace", "standard"))
                .with_function(FunctionDescriptor::php("decbin", "standard"))
                .with_function(FunctionDescriptor::php("dechex", "standard"))
                .with_function(FunctionDescriptor::php("decoct", "standard"))
                .with_function(FunctionDescriptor::php("deg2rad", "standard"))
                .with_function(FunctionDescriptor::php("defined", "standard"))
                .with_function(FunctionDescriptor::php("dirname", "standard"))
                .with_function(FunctionDescriptor::php("enum_exists", "standard"))
                .with_function(FunctionDescriptor::php("error_reporting", "standard"))
                .with_function(FunctionDescriptor::php("exec", "standard"))
                .with_function(FunctionDescriptor::php("exp", "standard"))
                .with_function(FunctionDescriptor::php("expm1", "standard"))
                .with_function(FunctionDescriptor::php("explode", "standard"))
                .with_function(FunctionDescriptor::php("extension_loaded", "standard"))
                .with_function(FunctionDescriptor::php("fclose", "standard"))
                .with_function(FunctionDescriptor::php("feof", "standard"))
                .with_function(FunctionDescriptor::php("fflush", "standard"))
                .with_function(FunctionDescriptor::php("fgetc", "standard"))
                .with_function(FunctionDescriptor::php("fgets", "standard"))
                .with_function(FunctionDescriptor::php("file_exists", "standard"))
                .with_function(FunctionDescriptor::php("file_get_contents", "standard"))
                .with_function(FunctionDescriptor::php("file_put_contents", "standard"))
                .with_function(FunctionDescriptor::php("filemtime", "standard"))
                .with_function(FunctionDescriptor::php("filesize", "standard"))
                .with_function(FunctionDescriptor::php("filetype", "standard"))
                .with_function(FunctionDescriptor::php("floor", "standard"))
                .with_function(FunctionDescriptor::php("floatval", "standard"))
                .with_function(FunctionDescriptor::php("flush", "standard"))
                .with_function(FunctionDescriptor::php("fdiv", "standard"))
                .with_function(FunctionDescriptor::php("fmod", "standard"))
                .with_function(FunctionDescriptor::php("fopen", "standard"))
                .with_function(FunctionDescriptor::php("fpow", "standard"))
                .with_function(FunctionDescriptor::php("fprintf", "standard"))
                .with_function(FunctionDescriptor::php("fread", "standard"))
                .with_function(FunctionDescriptor::php("fseek", "standard"))
                .with_function(FunctionDescriptor::php("ftell", "standard"))
                .with_function(FunctionDescriptor::php("function_exists", "standard"))
                .with_function(FunctionDescriptor::php("forward_static_call", "standard"))
                .with_function(FunctionDescriptor::php("func_get_arg", "standard"))
                .with_function(FunctionDescriptor::php("func_get_args", "standard"))
                .with_function(FunctionDescriptor::php("func_num_args", "standard"))
                .with_function(FunctionDescriptor::php("fwrite", "standard"))
                .with_function(FunctionDescriptor::php("get_current_user", "standard"))
                .with_function(FunctionDescriptor::php("get_cfg_var", "standard"))
                .with_function(FunctionDescriptor::php("get_called_class", "standard"))
                .with_function(FunctionDescriptor::php("get_class", "standard"))
                .with_function(FunctionDescriptor::php("get_class_methods", "standard"))
                .with_function(FunctionDescriptor::php("get_class_vars", "standard"))
                .with_function(FunctionDescriptor::php("get_debug_type", "standard"))
                .with_function(FunctionDescriptor::php("get_declared_classes", "standard"))
                .with_function(FunctionDescriptor::php(
                    "get_declared_interfaces",
                    "standard",
                ))
                .with_function(FunctionDescriptor::php("get_declared_traits", "standard"))
                .with_function(FunctionDescriptor::php("get_loaded_extensions", "standard"))
                .with_function(FunctionDescriptor::php(
                    "get_mangled_object_vars",
                    "standard",
                ))
                .with_function(FunctionDescriptor::php("get_object_vars", "standard"))
                .with_function(FunctionDescriptor::php("get_parent_class", "standard"))
                .with_function(FunctionDescriptor::php("getrandmax", "standard"))
                .with_function(FunctionDescriptor::php("get_resource_id", "standard"))
                .with_function(FunctionDescriptor::php("get_resource_type", "standard"))
                .with_function(FunctionDescriptor::php("getimagesize", "standard"))
                .with_function(FunctionDescriptor::php("getcwd", "standard"))
                .with_function(FunctionDescriptor::php("getenv", "standard"))
                .with_function(FunctionDescriptor::php("gettype", "standard"))
                .with_function(FunctionDescriptor::php("glob", "standard"))
                .with_function(FunctionDescriptor::php("hex2bin", "standard"))
                .with_function(FunctionDescriptor::php("hexdec", "standard"))
                .with_function(FunctionDescriptor::php("htmlentities", "standard"))
                .with_function(FunctionDescriptor::php("htmlspecialchars", "standard"))
                .with_function(FunctionDescriptor::php(
                    "htmlspecialchars_decode",
                    "standard",
                ))
                .with_function(FunctionDescriptor::php("hypot", "standard"))
                .with_function(FunctionDescriptor::php("hrtime", "standard"))
                .with_function(FunctionDescriptor::php("http_build_query", "standard"))
                .with_function(FunctionDescriptor::php("implode", "standard"))
                .with_function(FunctionDescriptor::php("in_array", "standard"))
                .with_function(FunctionDescriptor::php("ini_get", "standard"))
                .with_function(FunctionDescriptor::php("ini_get_all", "standard"))
                .with_function(FunctionDescriptor::php("ini_set", "standard"))
                .with_function(FunctionDescriptor::php("intdiv", "standard"))
                .with_function(FunctionDescriptor::php("interface_exists", "standard"))
                .with_function(FunctionDescriptor::php("is_a", "core"))
                .with_function(FunctionDescriptor::php("intval", "standard"))
                .with_function(FunctionDescriptor::php("is_array", "standard"))
                .with_function(FunctionDescriptor::php("is_bool", "standard"))
                .with_function(FunctionDescriptor::php("is_countable", "standard"))
                .with_function(FunctionDescriptor::php("is_dir", "standard"))
                .with_function(FunctionDescriptor::php("is_file", "standard"))
                .with_function(FunctionDescriptor::php("is_finite", "standard"))
                .with_function(FunctionDescriptor::php("is_float", "standard"))
                .with_function(FunctionDescriptor::php("is_infinite", "standard"))
                .with_function(FunctionDescriptor::php("is_int", "standard"))
                .with_function(FunctionDescriptor::php("is_iterable", "standard"))
                .with_function(FunctionDescriptor::php("is_link", "standard"))
                .with_function(FunctionDescriptor::php("is_nan", "standard"))
                .with_function(FunctionDescriptor::php("is_null", "standard"))
                .with_function(FunctionDescriptor::php("is_object", "standard"))
                .with_function(FunctionDescriptor::php("is_readable", "standard"))
                .with_function(FunctionDescriptor::php("is_resource", "standard"))
                .with_function(FunctionDescriptor::php("is_scalar", "standard"))
                .with_function(FunctionDescriptor::php("is_string", "standard"))
                .with_function(FunctionDescriptor::php("is_subclass_of", "standard"))
                .with_function(FunctionDescriptor::php("is_writable", "standard"))
                .with_function(FunctionDescriptor::php("krsort", "standard"))
                .with_function(FunctionDescriptor::php("ksort", "standard"))
                .with_function(FunctionDescriptor::php("lcfirst", "standard"))
                .with_function(FunctionDescriptor::php("lstat", "standard"))
                .with_function(FunctionDescriptor::php("log", "standard"))
                .with_function(FunctionDescriptor::php("log10", "standard"))
                .with_function(FunctionDescriptor::php("log1p", "standard"))
                .with_function(FunctionDescriptor::php("ltrim", "standard"))
                .with_function(FunctionDescriptor::php("max", "standard"))
                .with_function(FunctionDescriptor::php("md5", "standard"))
                .with_function(FunctionDescriptor::php("method_exists", "standard"))
                .with_function(FunctionDescriptor::php("min", "standard"))
                .with_function(FunctionDescriptor::php("mkdir", "standard"))
                .with_function(FunctionDescriptor::php("mime_content_type", "standard"))
                .with_function(FunctionDescriptor::php("natcasesort", "standard"))
                .with_function(FunctionDescriptor::php("natsort", "standard"))
                .with_function(FunctionDescriptor::php("number_format", "standard"))
                .with_function(FunctionDescriptor::php("ob_end_clean", "standard"))
                .with_function(FunctionDescriptor::php("ob_end_flush", "standard"))
                .with_function(FunctionDescriptor::php("ob_get_clean", "standard"))
                .with_function(FunctionDescriptor::php("ob_get_contents", "standard"))
                .with_function(FunctionDescriptor::php("ob_get_length", "standard"))
                .with_function(FunctionDescriptor::php("ob_get_level", "standard"))
                .with_function(FunctionDescriptor::php("ob_start", "standard"))
                .with_function(FunctionDescriptor::php("octdec", "standard"))
                .with_function(FunctionDescriptor::php("opendir", "standard"))
                .with_function(FunctionDescriptor::php("ord", "standard"))
                .with_function(FunctionDescriptor::php("pathinfo", "standard"))
                .with_function(FunctionDescriptor::php("parse_url", "standard"))
                .with_function(FunctionDescriptor::php("passthru", "standard"))
                .with_function(FunctionDescriptor::php("pclose", "standard"))
                .with_function(FunctionDescriptor::php("php_sapi_name", "standard"))
                .with_function(FunctionDescriptor::php("php_uname", "standard"))
                .with_function(FunctionDescriptor::php("pi", "standard"))
                .with_function(FunctionDescriptor::php("popen", "standard"))
                .with_function(FunctionDescriptor::php("print", "standard"))
                .with_function(FunctionDescriptor::php("print_r", "standard"))
                .with_function(FunctionDescriptor::php("printf", "standard"))
                .with_function(FunctionDescriptor::php("pow", "standard"))
                .with_function(FunctionDescriptor::php("property_exists", "standard"))
                .with_function(FunctionDescriptor::php("proc_close", "standard"))
                .with_function(FunctionDescriptor::php("proc_get_status", "standard"))
                .with_function(FunctionDescriptor::php("proc_open", "standard"))
                .with_function(FunctionDescriptor::php("putenv", "standard"))
                .with_function(FunctionDescriptor::php("rad2deg", "standard"))
                .with_function(FunctionDescriptor::php("rawurldecode", "standard"))
                .with_function(FunctionDescriptor::php("rawurlencode", "standard"))
                .with_function(FunctionDescriptor::php("range", "standard"))
                .with_function(FunctionDescriptor::php("readdir", "standard"))
                .with_function(FunctionDescriptor::php("readfile", "standard"))
                .with_function(FunctionDescriptor::php("realpath", "standard"))
                .with_function(FunctionDescriptor::php("rename", "standard"))
                .with_function(FunctionDescriptor::php("restore_error_handler", "standard"))
                .with_function(FunctionDescriptor::php(
                    "restore_exception_handler",
                    "standard",
                ))
                .with_function(FunctionDescriptor::php("rewind", "standard"))
                .with_function(FunctionDescriptor::php("rewinddir", "standard"))
                .with_function(FunctionDescriptor::php("rmdir", "standard"))
                .with_function(FunctionDescriptor::php("round", "standard"))
                .with_function(FunctionDescriptor::php("rsort", "standard"))
                .with_function(FunctionDescriptor::php("rtrim", "standard"))
                .with_function(FunctionDescriptor::php("scandir", "standard"))
                .with_function(FunctionDescriptor::php("serialize", "standard"))
                .with_function(FunctionDescriptor::php("set_error_handler", "standard"))
                .with_function(FunctionDescriptor::php("set_exception_handler", "standard"))
                .with_function(FunctionDescriptor::php("set_time_limit", "standard"))
                .with_function(FunctionDescriptor::php("sha1", "standard"))
                .with_function(FunctionDescriptor::php("shell_exec", "standard"))
                .with_function(FunctionDescriptor::php("sin", "standard"))
                .with_function(FunctionDescriptor::php("sinh", "standard"))
                .with_function(FunctionDescriptor::php("sizeof", "standard"))
                .with_function(FunctionDescriptor::php("sort", "standard"))
                .with_function(FunctionDescriptor::php("sprintf", "standard"))
                .with_function(FunctionDescriptor::php("sqrt", "standard"))
                .with_function(FunctionDescriptor::php("stat", "standard"))
                .with_function(FunctionDescriptor::php("stream_context_create", "standard"))
                .with_function(FunctionDescriptor::php(
                    "stream_context_get_options",
                    "standard",
                ))
                .with_function(FunctionDescriptor::php(
                    "stream_context_set_option",
                    "standard",
                ))
                .with_function(FunctionDescriptor::php("stream_copy_to_stream", "standard"))
                .with_function(FunctionDescriptor::php("stream_get_contents", "standard"))
                .with_function(FunctionDescriptor::php("stream_get_meta_data", "standard"))
                .with_function(FunctionDescriptor::php("stream_get_wrappers", "standard"))
                .with_function(FunctionDescriptor::php("stream_is_local", "standard"))
                .with_function(FunctionDescriptor::php("stream_isatty", "standard"))
                .with_function(FunctionDescriptor::php(
                    "stream_resolve_include_path",
                    "standard",
                ))
                .with_function(FunctionDescriptor::php("str_contains", "standard"))
                .with_function(FunctionDescriptor::php("str_ends_with", "standard"))
                .with_function(FunctionDescriptor::php("str_pad", "standard"))
                .with_function(FunctionDescriptor::php("str_repeat", "standard"))
                .with_function(FunctionDescriptor::php("str_replace", "standard"))
                .with_function(FunctionDescriptor::php("str_starts_with", "standard"))
                .with_function(FunctionDescriptor::php("strcasecmp", "standard"))
                .with_function(FunctionDescriptor::php("strcmp", "standard"))
                .with_function(FunctionDescriptor::php("stripos", "standard"))
                .with_function(FunctionDescriptor::php("strlen", "standard"))
                .with_function(FunctionDescriptor::php("strncasecmp", "standard"))
                .with_function(FunctionDescriptor::php("strncmp", "standard"))
                .with_function(FunctionDescriptor::php("strpos", "standard"))
                .with_function(FunctionDescriptor::php("strrev", "standard"))
                .with_function(FunctionDescriptor::php("strrpos", "standard"))
                .with_function(FunctionDescriptor::php("strtolower", "standard"))
                .with_function(FunctionDescriptor::php("strval", "standard"))
                .with_function(FunctionDescriptor::php("strtoupper", "standard"))
                .with_function(FunctionDescriptor::php("strtr", "standard"))
                .with_function(FunctionDescriptor::php("substr", "standard"))
                .with_function(FunctionDescriptor::php("system", "standard"))
                .with_function(FunctionDescriptor::php("tan", "standard"))
                .with_function(FunctionDescriptor::php("tanh", "standard"))
                .with_function(FunctionDescriptor::php("tempnam", "standard"))
                .with_function(FunctionDescriptor::php("tmpfile", "standard"))
                .with_function(FunctionDescriptor::php("touch", "standard"))
                .with_function(FunctionDescriptor::php("trim", "standard"))
                .with_function(FunctionDescriptor::php("trigger_error", "standard"))
                .with_function(FunctionDescriptor::php("trait_exists", "standard"))
                .with_function(FunctionDescriptor::php("uasort", "standard"))
                .with_function(FunctionDescriptor::php("uksort", "standard"))
                .with_function(FunctionDescriptor::php("unlink", "standard"))
                .with_function(FunctionDescriptor::php("unserialize", "standard"))
                .with_function(FunctionDescriptor::php("urldecode", "standard"))
                .with_function(FunctionDescriptor::php("urlencode", "standard"))
                .with_function(FunctionDescriptor::php("usort", "standard"))
                .with_function(FunctionDescriptor::php("ucfirst", "standard"))
                .with_function(FunctionDescriptor::php("ucwords", "standard"))
                .with_function(FunctionDescriptor::php("user_error", "standard"))
                .with_function(FunctionDescriptor::php("var_dump", "standard"))
                .with_function(FunctionDescriptor::php("var_export", "standard"))
                .with_function(FunctionDescriptor::php("version_compare", "standard"))
                .with_function(FunctionDescriptor::php("vprintf", "standard"))
                .with_function(FunctionDescriptor::php("vsprintf", "standard"))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_URL_SCHEME",
                    "standard",
                    ConstantValue::Int(constants::PHP_URL_SCHEME),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_URL_HOST",
                    "standard",
                    ConstantValue::Int(constants::PHP_URL_HOST),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_URL_PORT",
                    "standard",
                    ConstantValue::Int(constants::PHP_URL_PORT),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_URL_USER",
                    "standard",
                    ConstantValue::Int(constants::PHP_URL_USER),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_URL_PASS",
                    "standard",
                    ConstantValue::Int(constants::PHP_URL_PASS),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_URL_PATH",
                    "standard",
                    ConstantValue::Int(constants::PHP_URL_PATH),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_URL_QUERY",
                    "standard",
                    ConstantValue::Int(constants::PHP_URL_QUERY),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_URL_FRAGMENT",
                    "standard",
                    ConstantValue::Int(constants::PHP_URL_FRAGMENT),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "IMAGETYPE_GIF",
                    "standard",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "IMAGETYPE_JPEG",
                    "standard",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "IMAGETYPE_PNG",
                    "standard",
                    ConstantValue::Int(3),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "IMAGETYPE_WEBP",
                    "standard",
                    ConstantValue::Int(18),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "IMAGETYPE_AVIF",
                    "standard",
                    ConstantValue::Int(19),
                )),
            ExtensionDescriptor::new("json")
                .with_function(FunctionDescriptor::php("json_decode", "json"))
                .with_function(FunctionDescriptor::php("json_encode", "json"))
                .with_function(FunctionDescriptor::php("json_last_error", "json"))
                .with_function(FunctionDescriptor::php("json_last_error_msg", "json"))
                .with_function(FunctionDescriptor::php("json_validate", "json"))
                .with_class(ClassDescriptor::new(
                    "JsonException",
                    "json",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "JsonSerializable",
                    "json",
                    ClassKind::Interface,
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_BIGINT_AS_STRING",
                    "json",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_ERROR_DEPTH",
                    "json",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_ERROR_NONE",
                    "json",
                    ConstantValue::Int(0),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_ERROR_SYNTAX",
                    "json",
                    ConstantValue::Int(4),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_ERROR_UTF8",
                    "json",
                    ConstantValue::Int(5),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_OBJECT_AS_ARRAY",
                    "json",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_PRETTY_PRINT",
                    "json",
                    ConstantValue::Int(128),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_PRESERVE_ZERO_FRACTION",
                    "json",
                    ConstantValue::Int(1024),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_THROW_ON_ERROR",
                    "json",
                    ConstantValue::Int(4_194_304),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_UNESCAPED_SLASHES",
                    "json",
                    ConstantValue::Int(64),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "JSON_UNESCAPED_UNICODE",
                    "json",
                    ConstantValue::Int(256),
                )),
            ExtensionDescriptor::new("pcre")
                .with_function(FunctionDescriptor::php("preg_grep", "pcre"))
                .with_function(FunctionDescriptor::php("preg_last_error", "pcre"))
                .with_function(FunctionDescriptor::php("preg_last_error_msg", "pcre"))
                .with_function(FunctionDescriptor::php("preg_match", "pcre"))
                .with_function(FunctionDescriptor::php("preg_match_all", "pcre"))
                .with_function(FunctionDescriptor::php("preg_quote", "pcre"))
                .with_function(FunctionDescriptor::php("preg_replace", "pcre"))
                .with_function(FunctionDescriptor::php("preg_replace_callback", "pcre"))
                .with_function(FunctionDescriptor::php("preg_split", "pcre"))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_BAD_UTF8_ERROR",
                    "pcre",
                    ConstantValue::Int(4),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_BAD_UTF8_OFFSET_ERROR",
                    "pcre",
                    ConstantValue::Int(5),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_BACKTRACK_LIMIT_ERROR",
                    "pcre",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_GREP_INVERT",
                    "pcre",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_INTERNAL_ERROR",
                    "pcre",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_JIT_STACKLIMIT_ERROR",
                    "pcre",
                    ConstantValue::Int(6),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_NO_ERROR",
                    "pcre",
                    ConstantValue::Int(0),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_OFFSET_CAPTURE",
                    "pcre",
                    ConstantValue::Int(256),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_PATTERN_ORDER",
                    "pcre",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_RECURSION_LIMIT_ERROR",
                    "pcre",
                    ConstantValue::Int(3),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_SET_ORDER",
                    "pcre",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_SPLIT_DELIM_CAPTURE",
                    "pcre",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_SPLIT_NO_EMPTY",
                    "pcre",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_SPLIT_OFFSET_CAPTURE",
                    "pcre",
                    ConstantValue::Int(4),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PREG_UNMATCHED_AS_NULL",
                    "pcre",
                    ConstantValue::Int(512),
                )),
            ExtensionDescriptor::new("session")
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_SESSION_DISABLED",
                    "session",
                    ConstantValue::Int(0),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_SESSION_NONE",
                    "session",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "PHP_SESSION_ACTIVE",
                    "session",
                    ConstantValue::Int(2),
                ))
                .with_function(FunctionDescriptor::php("session_destroy", "session"))
                .with_function(FunctionDescriptor::php("session_id", "session"))
                .with_function(FunctionDescriptor::php("session_name", "session"))
                .with_function(FunctionDescriptor::php("session_start", "session"))
                .with_function(FunctionDescriptor::php("session_status", "session")),
            ExtensionDescriptor::new("pdo")
                .with_function(FunctionDescriptor::php("pdo_drivers", "pdo"))
                .with_class(ClassDescriptor::new("PDO", "pdo", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "PDOException",
                    "pdo",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new("PDORow", "pdo", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "PDOStatement",
                    "pdo",
                    ClassKind::Class,
                )),
            ExtensionDescriptor::new("pdo_sqlite").with_class(ClassDescriptor::new(
                "PDO_SQLite_Ext",
                "pdo_sqlite",
                ClassKind::Class,
            )),
            ExtensionDescriptor::new("mysqli")
                .with_constant(ConstantDescriptor::with_value(
                    "MYSQLI_ASSOC",
                    "mysqli",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "MYSQLI_NUM",
                    "mysqli",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "MYSQLI_BOTH",
                    "mysqli",
                    ConstantValue::Int(3),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "MYSQLI_REPORT_OFF",
                    "mysqli",
                    ConstantValue::Int(0),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "MYSQLI_STORE_RESULT",
                    "mysqli",
                    ConstantValue::Int(0),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "MYSQLI_USE_RESULT",
                    "mysqli",
                    ConstantValue::Int(1),
                ))
                .with_function(FunctionDescriptor::php("mysqli_close", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_connect", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_connect_errno", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_connect_error", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_errno", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_error", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_escape_string", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_fetch_array", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_fetch_assoc", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_fetch_row", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_free_result", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_init", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_num_fields", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_num_rows", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_prepare", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_query", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_real_connect", "mysqli"))
                .with_function(FunctionDescriptor::php(
                    "mysqli_real_escape_string",
                    "mysqli",
                ))
                .with_function(FunctionDescriptor::php("mysqli_report", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_select_db", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_set_charset", "mysqli"))
                .with_function(FunctionDescriptor::php("mysqli_stmt_init", "mysqli"))
                .with_class(ClassDescriptor::new("mysqli", "mysqli", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "mysqli_driver",
                    "mysqli",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "mysqli_result",
                    "mysqli",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "mysqli_stmt",
                    "mysqli",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "mysqli_warning",
                    "mysqli",
                    ClassKind::Class,
                )),
            ExtensionDescriptor::new("curl")
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_URL",
                    "curl",
                    ConstantValue::Int(10002),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_RETURNTRANSFER",
                    "curl",
                    ConstantValue::Int(19913),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_TIMEOUT",
                    "curl",
                    ConstantValue::Int(13),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_TIMEOUT_MS",
                    "curl",
                    ConstantValue::Int(155),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_FOLLOWLOCATION",
                    "curl",
                    ConstantValue::Int(52),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_HTTPHEADER",
                    "curl",
                    ConstantValue::Int(10023),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_POST",
                    "curl",
                    ConstantValue::Int(47),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_POSTFIELDS",
                    "curl",
                    ConstantValue::Int(10015),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_CUSTOMREQUEST",
                    "curl",
                    ConstantValue::Int(10036),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_SSL_VERIFYPEER",
                    "curl",
                    ConstantValue::Int(64),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLOPT_SSL_VERIFYHOST",
                    "curl",
                    ConstantValue::Int(81),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLINFO_EFFECTIVE_URL",
                    "curl",
                    ConstantValue::Int(1048577),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLINFO_HTTP_CODE",
                    "curl",
                    ConstantValue::Int(2097154),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLINFO_RESPONSE_CODE",
                    "curl",
                    ConstantValue::Int(2097154),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "CURLINFO_TOTAL_TIME",
                    "curl",
                    ConstantValue::Int(3145731),
                ))
                .with_function(FunctionDescriptor::php("curl_close", "curl"))
                .with_function(FunctionDescriptor::php("curl_errno", "curl"))
                .with_function(FunctionDescriptor::php("curl_error", "curl"))
                .with_function(FunctionDescriptor::php("curl_exec", "curl"))
                .with_function(FunctionDescriptor::php("curl_getinfo", "curl"))
                .with_function(FunctionDescriptor::php("curl_init", "curl"))
                .with_function(FunctionDescriptor::php("curl_setopt", "curl"))
                .with_function(FunctionDescriptor::php("curl_version", "curl"))
                .with_class(ClassDescriptor::new("CurlHandle", "curl", ClassKind::Class)),
            ExtensionDescriptor::new("openssl")
                .with_constant(ConstantDescriptor::with_value(
                    "OPENSSL_ALGO_SHA1",
                    "openssl",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "OPENSSL_ALGO_SHA256",
                    "openssl",
                    ConstantValue::Int(7),
                ))
                .with_function(FunctionDescriptor::php("openssl_digest", "openssl"))
                .with_function(FunctionDescriptor::php("openssl_get_md_methods", "openssl"))
                .with_function(FunctionDescriptor::php(
                    "openssl_random_pseudo_bytes",
                    "openssl",
                ))
                .with_function(FunctionDescriptor::php("openssl_verify", "openssl")),
            ExtensionDescriptor::new("phar")
                .with_class(ClassDescriptor::new("Phar", "phar", ClassKind::Class))
                .with_class(ClassDescriptor::new("PharData", "phar", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "PharFileInfo",
                    "phar",
                    ClassKind::Class,
                )),
            ExtensionDescriptor::new("sqlite3")
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_ASSOC",
                    "sqlite3",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_NUM",
                    "sqlite3",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_BOTH",
                    "sqlite3",
                    ConstantValue::Int(3),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_INTEGER",
                    "sqlite3",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_FLOAT",
                    "sqlite3",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_TEXT",
                    "sqlite3",
                    ConstantValue::Int(3),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_BLOB",
                    "sqlite3",
                    ConstantValue::Int(4),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_NULL",
                    "sqlite3",
                    ConstantValue::Int(5),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_OPEN_READONLY",
                    "sqlite3",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_OPEN_READWRITE",
                    "sqlite3",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_OPEN_CREATE",
                    "sqlite3",
                    ConstantValue::Int(4),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "SQLITE3_DETERMINISTIC",
                    "sqlite3",
                    ConstantValue::Int(2048),
                ))
                .with_class(ClassDescriptor::new("SQLite3", "sqlite3", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "SQLite3Exception",
                    "sqlite3",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "SQLite3Result",
                    "sqlite3",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "SQLite3Stmt",
                    "sqlite3",
                    ClassKind::Class,
                )),
            ExtensionDescriptor::new("mbstring")
                .enabled_by_default(true)
                .with_function(FunctionDescriptor::php("mb_check_encoding", "mbstring"))
                .with_function(FunctionDescriptor::php("mb_convert_encoding", "mbstring"))
                .with_function(FunctionDescriptor::php("mb_detect_encoding", "mbstring"))
                .with_function(FunctionDescriptor::php("mb_internal_encoding", "mbstring"))
                .with_function(FunctionDescriptor::php("mb_strlen", "mbstring"))
                .with_function(FunctionDescriptor::php("mb_strtolower", "mbstring"))
                .with_function(FunctionDescriptor::php("mb_strtoupper", "mbstring"))
                .with_function(FunctionDescriptor::php("mb_strpos", "mbstring"))
                .with_function(FunctionDescriptor::php("mb_substr", "mbstring")),
            ExtensionDescriptor::new("intl")
                .enabled_by_default(false)
                .with_function(FunctionDescriptor::php("grapheme_strlen", "intl"))
                .with_function(FunctionDescriptor::php("intl_get_error_code", "intl"))
                .with_function(FunctionDescriptor::php("normalizer_normalize", "intl"))
                .with_class(ClassDescriptor::new("Collator", "intl", ClassKind::Class))
                .with_class(ClassDescriptor::new("IntlChar", "intl", ClassKind::Class))
                .with_class(ClassDescriptor::new("Locale", "intl", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "NumberFormatter",
                    "intl",
                    ClassKind::Class,
                )),
            ExtensionDescriptor::new("hash")
                .with_function(FunctionDescriptor::php("hash", "hash"))
                .with_function(FunctionDescriptor::php("hash_algos", "hash"))
                .with_function(FunctionDescriptor::php("hash_equals", "hash"))
                .with_function(FunctionDescriptor::php("hash_hmac", "hash")),
            ExtensionDescriptor::new("filter")
                .with_function(FunctionDescriptor::php("filter_input", "filter"))
                .with_function(FunctionDescriptor::php("filter_var", "filter"))
                .with_constant(ConstantDescriptor::with_value(
                    "INPUT_POST",
                    "filter",
                    ConstantValue::Int(0),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "INPUT_GET",
                    "filter",
                    ConstantValue::Int(1),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "INPUT_COOKIE",
                    "filter",
                    ConstantValue::Int(2),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "INPUT_ENV",
                    "filter",
                    ConstantValue::Int(4),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "INPUT_SERVER",
                    "filter",
                    ConstantValue::Int(5),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_DEFAULT",
                    "filter",
                    ConstantValue::Int(516),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_VALIDATE_BOOL",
                    "filter",
                    ConstantValue::Int(258),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_VALIDATE_BOOLEAN",
                    "filter",
                    ConstantValue::Int(258),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_VALIDATE_URL",
                    "filter",
                    ConstantValue::Int(273),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_VALIDATE_EMAIL",
                    "filter",
                    ConstantValue::Int(274),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_VALIDATE_IP",
                    "filter",
                    ConstantValue::Int(275),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_SANITIZE_EMAIL",
                    "filter",
                    ConstantValue::Int(517),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_SANITIZE_URL",
                    "filter",
                    ConstantValue::Int(518),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_SANITIZE_NUMBER_INT",
                    "filter",
                    ConstantValue::Int(519),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_NULL_ON_FAILURE",
                    "filter",
                    ConstantValue::Int(134_217_728),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_FLAG_IPV4",
                    "filter",
                    ConstantValue::Int(1_048_576),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_FLAG_IPV6",
                    "filter",
                    ConstantValue::Int(2_097_152),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_FLAG_PATH_REQUIRED",
                    "filter",
                    ConstantValue::Int(262_144),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILTER_FLAG_QUERY_REQUIRED",
                    "filter",
                    ConstantValue::Int(524_288),
                )),
            ExtensionDescriptor::new("iconv")
                .with_function(FunctionDescriptor::php("iconv", "iconv"))
                .with_function(FunctionDescriptor::php("iconv_strlen", "iconv"))
                .with_function(FunctionDescriptor::php("iconv_strpos", "iconv"))
                .with_function(FunctionDescriptor::php("iconv_substr", "iconv"))
                .with_constant(ConstantDescriptor::with_value(
                    "ICONV_IMPL",
                    "iconv",
                    ConstantValue::String("phrust"),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "ICONV_VERSION",
                    "iconv",
                    ConstantValue::String("bounded-utf8-ascii"),
                )),
            ExtensionDescriptor::new("zlib")
                .with_function(FunctionDescriptor::php("gzcompress", "zlib"))
                .with_function(FunctionDescriptor::php("gzdecode", "zlib"))
                .with_function(FunctionDescriptor::php("gzencode", "zlib"))
                .with_function(FunctionDescriptor::php("gzuncompress", "zlib"))
                .with_function(FunctionDescriptor::php("zlib_decode", "zlib"))
                .with_constant(ConstantDescriptor::with_value(
                    "FORCE_DEFLATE",
                    "zlib",
                    ConstantValue::Int(15),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FORCE_GZIP",
                    "zlib",
                    ConstantValue::Int(31),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "ZLIB_ENCODING_RAW",
                    "zlib",
                    ConstantValue::Int(-15),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "ZLIB_ENCODING_GZIP",
                    "zlib",
                    ConstantValue::Int(31),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "ZLIB_ENCODING_DEFLATE",
                    "zlib",
                    ConstantValue::Int(15),
                )),
            ExtensionDescriptor::new("zip").with_class(ClassDescriptor::new(
                "ZipArchive",
                "zip",
                ClassKind::Class,
            )),
            ExtensionDescriptor::new("fileinfo")
                .with_function(FunctionDescriptor::php("finfo_buffer", "fileinfo"))
                .with_function(FunctionDescriptor::php("finfo_file", "fileinfo"))
                .with_function(FunctionDescriptor::php("finfo_open", "fileinfo"))
                .with_constant(ConstantDescriptor::with_value(
                    "FILEINFO_NONE",
                    "fileinfo",
                    ConstantValue::Int(0),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILEINFO_MIME_TYPE",
                    "fileinfo",
                    ConstantValue::Int(16),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILEINFO_MIME_ENCODING",
                    "fileinfo",
                    ConstantValue::Int(1024),
                ))
                .with_constant(ConstantDescriptor::with_value(
                    "FILEINFO_MIME",
                    "fileinfo",
                    ConstantValue::Int(1040),
                )),
            ExtensionDescriptor::new("exif")
                .with_function(FunctionDescriptor::php("exif_imagetype", "exif")),
            ExtensionDescriptor::new("random")
                .with_function(FunctionDescriptor::php("random_bytes", "random"))
                .with_function(FunctionDescriptor::php("random_int", "random")),
            ExtensionDescriptor::new("date")
                .with_function(FunctionDescriptor::php("date", "date"))
                .with_function(FunctionDescriptor::php("date_default_timezone_get", "date"))
                .with_function(FunctionDescriptor::php("date_default_timezone_set", "date"))
                .with_function(FunctionDescriptor::php("strtotime", "date"))
                .with_function(FunctionDescriptor::php("time", "date"))
                .with_function(FunctionDescriptor::php("timezone_identifiers_list", "date"))
                .with_class(ClassDescriptor::new(
                    "DateInterval",
                    "date",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new("DateTime", "date", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "DateTimeImmutable",
                    "date",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "DateTimeInterface",
                    "date",
                    ClassKind::Interface,
                ))
                .with_class(ClassDescriptor::new(
                    "DateTimeZone",
                    "date",
                    ClassKind::Class,
                )),
            ExtensionDescriptor::new("spl")
                .with_function(FunctionDescriptor::php("iterator_count", "spl"))
                .with_function(FunctionDescriptor::php("iterator_to_array", "spl"))
                .with_function(FunctionDescriptor::php("spl_autoload_call", "spl"))
                .with_function(FunctionDescriptor::php("spl_autoload_functions", "spl"))
                .with_function(FunctionDescriptor::php("spl_autoload_register", "spl"))
                .with_function(FunctionDescriptor::php("spl_autoload_unregister", "spl"))
                .with_function(FunctionDescriptor::php("spl_object_hash", "spl"))
                .with_function(FunctionDescriptor::php("spl_object_id", "spl"))
                .with_class(ClassDescriptor::new(
                    "ArrayAccess",
                    "spl",
                    ClassKind::Interface,
                ))
                .with_class(ClassDescriptor::new(
                    "AppendIterator",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "ArrayIterator",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new("ArrayObject", "spl", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "BadFunctionCallException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "BadMethodCallException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "Countable",
                    "spl",
                    ClassKind::Interface,
                ))
                .with_class(ClassDescriptor::new(
                    "DomainException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "EmptyIterator",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "InvalidArgumentException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "Iterator",
                    "spl",
                    ClassKind::Interface,
                ))
                .with_class(ClassDescriptor::new(
                    "IteratorAggregate",
                    "spl",
                    ClassKind::Interface,
                ))
                .with_class(ClassDescriptor::new(
                    "IteratorIterator",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "LengthException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "LimitIterator",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "LogicException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "OutOfBoundsException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "OutOfRangeException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "OverflowException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "RangeException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "RecursiveArrayIterator",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "RecursiveIterator",
                    "spl",
                    ClassKind::Interface,
                ))
                .with_class(ClassDescriptor::new(
                    "RuntimeException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "SeekableIterator",
                    "spl",
                    ClassKind::Interface,
                ))
                .with_class(ClassDescriptor::new(
                    "Serializable",
                    "spl",
                    ClassKind::Interface,
                ))
                .with_class(ClassDescriptor::new(
                    "SplDoublyLinkedList",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new("SplFileInfo", "spl", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "SplFileObject",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "SplFixedArray",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "SplObjectStorage",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new("SplQueue", "spl", ClassKind::Class))
                .with_class(ClassDescriptor::new("SplStack", "spl", ClassKind::Class))
                .with_class(ClassDescriptor::new(
                    "SplTempFileObject",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "Traversable",
                    "spl",
                    ClassKind::Interface,
                ))
                .with_class(ClassDescriptor::new(
                    "UnderflowException",
                    "spl",
                    ClassKind::Class,
                ))
                .with_class(ClassDescriptor::new(
                    "UnexpectedValueException",
                    "spl",
                    ClassKind::Class,
                )),
            reflection_extension(),
            tokenizer_extension(),
            ExtensionDescriptor::new("test")
                .enabled_by_default(false)
                .with_function(FunctionDescriptor::internal_test(
                    "__php_std_test_probe",
                    "test",
                )),
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

fn reflection_extension() -> ExtensionDescriptor {
    let mut extension = ExtensionDescriptor::new("reflection");
    for class in generated::arginfo::GENERATED_CLASSES
        .iter()
        .filter(|class| class.extension == "reflection")
    {
        extension = extension.with_class(ClassDescriptor::new(
            class.name,
            "reflection",
            generated_class_kind(class.kind),
        ));
    }
    extension
}

fn generated_class_kind(kind: &str) -> ClassKind {
    match kind {
        "interface" => ClassKind::Interface,
        "trait" => ClassKind::Trait,
        "enum" => ClassKind::Enum,
        _ => ClassKind::Class,
    }
}

fn tokenizer_extension() -> ExtensionDescriptor {
    let mut extension = ExtensionDescriptor::new("tokenizer")
        .with_function(FunctionDescriptor::php("token_get_all", "tokenizer"))
        .with_function(FunctionDescriptor::php("token_name", "tokenizer"))
        .with_class(ClassDescriptor::new(
            "PhpToken",
            "tokenizer",
            ClassKind::Class,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "TOKEN_PARSE",
            "tokenizer",
            ConstantValue::Int(php_runtime::tokenizer::TOKEN_PARSE),
        ));
    for (index, token_name) in php_lexer::TOKENIZER_TOKEN_NAMES.iter().enumerate() {
        extension = extension.with_constant(ConstantDescriptor::with_value(
            token_name.as_php_name(),
            "tokenizer",
            ConstantValue::Int(php_lexer::TOKENIZER_TOKEN_ID_BASE + index as i64),
        ));
    }
    extension
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
    use php_runtime::{BuiltinCompatibility, BuiltinRegistry};

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
    fn bounded_mbstring_is_enabled_while_intl_stays_hidden() {
        let registry = ExtensionRegistry::standard_library();

        assert!(registry.is_extension_enabled("mbstring"));
        assert!(!registry.is_extension_enabled("intl"));

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
                registry.enabled_php_function(name).is_none(),
                "{name} should stay hidden while intl is disabled"
            );
        }

        for name in ["Collator", "IntlChar", "Locale", "NumberFormatter"] {
            assert!(
                registry.enabled_class(name).is_none(),
                "{name} should stay hidden while intl is disabled"
            );
        }
    }

    #[test]
    fn infrastructure_registry_exposes_no_php_visible_functions() {
        let mut registry = ExtensionRegistry::standard_library();
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
            "htmlspecialchars",
            "htmlspecialchars_decode",
            "htmlentities",
            "http_build_query",
            "md5",
            "ord",
            "parse_url",
            "rawurldecode",
            "rawurlencode",
            "sha1",
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

        for name in ["hash", "hash_hmac"] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a hash function"
            );
        }
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
    fn standard_registry_tracks_stdlib_formatting_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in ["fprintf", "printf", "sprintf", "vprintf", "vsprintf"] {
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
    fn standard_registry_tracks_stdlib_error_handling_functions() {
        let registry = ExtensionRegistry::standard_library();

        for name in [
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
            "filetype",
            "stat",
            "lstat",
            "clearstatcache",
        ] {
            assert!(
                registry.enabled_php_function(name).is_some(),
                "{name} should be registered as a standard function"
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
            "stream_context_get_options",
            "stream_context_set_option",
            "stream_resolve_include_path",
            "stream_is_local",
            "stream_isatty",
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
            "JSON_ERROR_NONE",
            "JSON_THROW_ON_ERROR",
            "JSON_OBJECT_AS_ARRAY",
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
            "preg_grep",
            "preg_last_error",
            "preg_last_error_msg",
            "preg_match",
            "preg_match_all",
            "preg_quote",
            "preg_replace",
            "preg_replace_callback",
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
            "PREG_GREP_INVERT",
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
    fn visible_stdlib_functions_have_generated_arginfo() {
        let registry = ExtensionRegistry::standard_library();
        let missing = registry
            .extensions()
            .flat_map(ExtensionDescriptor::functions)
            .filter(|function| function.visibility() == SymbolVisibility::PhpVisible)
            .filter(|function| function.arginfo().is_none())
            .map(FunctionDescriptor::name)
            .collect::<Vec<_>>();

        assert_eq!(
            missing,
            ["print"],
            "`print` is a PHP language construct; visible function descriptors should otherwise have generated php-src arginfo"
        );
    }

    #[test]
    fn runtime_builtin_registry_entries_have_generated_arginfo() {
        let missing = BuiltinRegistry::new()
            .entries()
            .iter()
            .copied()
            .filter(|entry| entry.compatibility() == BuiltinCompatibility::Php)
            .filter(|entry| generated::arginfo::function_metadata(entry.name()).is_none())
            .map(php_runtime::BuiltinEntry::name)
            .collect::<Vec<_>>();

        assert_eq!(
            missing,
            ["print"],
            "`print` is a PHP language construct; all function builtins should have generated php-src arginfo"
        );
    }

    #[test]
    fn unknown_extension_mutation_is_rejected() {
        let mut registry = ExtensionRegistry::standard_library();
        assert_eq!(
            registry.enable_extension("missing"),
            Err(RegistryError::UnknownExtension("missing"))
        );
    }
}
