use super::{
    AttributeEntry, ClassConstantEntry, ClassEnumBackingType, ClassEnumCaseEntry, ClassMethodEntry,
    ClassPropertyEntry,
};

/// Normalizes a class-like name for runtime lookup.
#[must_use]
pub fn normalize_class_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

/// Preserves PHP-visible class spelling while removing a leading root slash.
#[must_use]
pub fn display_class_name(name: &str) -> String {
    let name = name.trim_start_matches('\\');
    if name.eq_ignore_ascii_case("stdclass") {
        "stdClass".to_owned()
    } else {
        name.to_owned()
    }
}

/// Runtime class table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassEntry {
    /// Canonical class lookup name. Shared: every instance of the class
    /// clones this handle into its storage, so all instances of one runtime
    /// class alias one allocation (and the handle address doubles as a
    /// per-class identity for address-keyed resolution caches).
    pub name: std::sync::Arc<str>,
    /// Canonical parent class lookup name, when declared.
    pub parent: Option<String>,
    /// Canonical interface names implemented or extended by this class-like.
    pub interfaces: Vec<String>,
    /// Runtime-visible instance methods.
    pub methods: Vec<ClassMethodEntry>,
    /// Runtime-visible instance properties.
    pub properties: Vec<ClassPropertyEntry>,
    /// Runtime-visible class constants.
    pub constants: Vec<ClassConstantEntry>,
    /// Runtime-visible enum cases.
    pub enum_cases: Vec<ClassEnumCaseEntry>,
    /// Runtime-visible attributes on this class-like declaration.
    pub attributes: Vec<AttributeEntry>,
    /// Backing type for backed enums.
    pub enum_backing_type: Option<ClassEnumBackingType>,
    /// Raw IR function ID for `__construct`, when present.
    pub constructor_id: Option<u32>,
    /// Class declaration flags.
    pub flags: ClassFlags,
}

/// Class declaration flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassFlags {
    /// The runtime method table contains the complete inherited lineage.
    ///
    /// Native property tests may conclude that magic methods are absent only
    /// when this publication-time guarantee is set. Extension/reflection
    /// placeholder classes leave it clear and retain their cold semantics.
    pub has_complete_method_table: bool,
    /// Instances satisfy the built-in `Countable` interface.
    pub implements_countable: bool,
    /// Instances satisfy the built-in `Traversable` interface hierarchy.
    pub implements_traversable: bool,
    /// Instances may create undeclared properties without magic/deprecation
    /// handling (`stdClass` or a class carrying `AllowDynamicProperties`).
    pub allows_dynamic_properties: bool,
    /// Abstract class.
    pub is_abstract: bool,
    /// Final class.
    pub is_final: bool,
    /// Readonly class.
    pub is_readonly: bool,
    /// Interface metadata entry.
    pub is_interface: bool,
    /// Enum metadata entry.
    pub is_enum: bool,
}

#[cfg(test)]
mod tests {
    use super::display_class_name;

    #[test]
    fn stdclass_uses_its_canonical_internal_spelling() {
        assert_eq!(display_class_name("stdclass"), "stdClass");
        assert_eq!(display_class_name("\\STDCLASS"), "stdClass");
        assert_eq!(display_class_name("UserClass"), "UserClass");
    }
}
