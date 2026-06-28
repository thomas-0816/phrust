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
    name.trim_start_matches('\\').to_owned()
}

/// Runtime class table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassEntry {
    /// Canonical class lookup name.
    pub name: String,
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
