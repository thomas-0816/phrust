//! IR unit and top-level tables.

use crate::constants::IrConstant;
use crate::function::{IrFunction, IrReturnType};
use crate::ids::{ClassId, ConstId, FileId, FunctionId, UnitId};
use crate::source_map::{IrSourceMap, IrSpan};
use serde::{Deserialize, Serialize};

/// Version marker for the runtime IR snapshot shape.
pub const IR_VERSION: u32 = 1;

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

/// Source file table entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileEntry {
    /// File ID.
    pub id: FileId,
    /// Display path.
    pub path: String,
}

/// Class table entry used by the object and runtime lowering.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassEntry {
    /// Class ID.
    pub id: ClassId,
    /// Resolved class name.
    pub name: String,
    /// Display class name preserving source spelling for `::class`.
    pub display_name: String,
    /// Resolved parent class name, when the class extends another class.
    pub parent: Option<String>,
    /// Resolved interface names implemented or extended by this class-like.
    pub interfaces: Vec<String>,
    /// Method entries in source order.
    pub methods: Vec<ClassMethodEntry>,
    /// Declared instance properties in source order.
    pub properties: Vec<ClassPropertyEntry>,
    /// Declared class constants in source order.
    pub constants: Vec<ClassConstantEntry>,
    /// Declared enum cases in source order.
    pub enum_cases: Vec<ClassEnumCaseEntry>,
    /// Attribute metadata attached to this class-like declaration.
    pub attributes: Vec<AttributeEntry>,
    /// Backing type for backed enums.
    pub enum_backing_type: Option<ClassEnumBackingType>,
    /// Constructor method function ID, when present.
    pub constructor: Option<FunctionId>,
    /// Class flags captured from Semantic frontend.
    pub flags: ClassFlags,
    /// Source span for the class declaration.
    pub span: IrSpan,
}

/// Class declaration flags.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassFlags {
    /// `abstract class`.
    pub is_abstract: bool,
    /// `final class`.
    pub is_final: bool,
    /// `readonly class`.
    pub is_readonly: bool,
    /// `interface` declaration or VM-provided internal interface metadata.
    pub is_interface: bool,
    /// `enum` declaration.
    pub is_enum: bool,
}

/// Class method table entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassMethodEntry {
    /// Normalized method lookup name.
    pub name: String,
    /// Source class-like that contributed the method.
    pub origin_class: String,
    /// Method implementation function.
    pub function: FunctionId,
    /// Method flags captured from Semantic frontend.
    pub flags: ClassMethodFlags,
    /// Attribute metadata attached to this method declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Class method flags.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassMethodFlags {
    /// `static`.
    pub is_static: bool,
    /// `private`.
    pub is_private: bool,
    /// `protected`.
    pub is_protected: bool,
    /// `abstract`.
    pub is_abstract: bool,
    /// `final`.
    pub is_final: bool,
}

/// Class property table entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassPropertyEntry {
    /// Property name without `$`.
    pub name: String,
    /// Constant-pool default when the MVP can lower it.
    pub default: Option<ConstId>,
    /// Optional Semantic frontend lowered runtime type enforced by the VM MVP.
    pub type_: Option<IrReturnType>,
    /// Property flags captured from Semantic frontend.
    pub flags: ClassPropertyFlags,
    /// Property hook functions captured from Semantic frontend.
    pub hooks: ClassPropertyHooks,
    /// Attribute metadata attached to this property declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Class property flags.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassPropertyFlags {
    /// `static`.
    pub is_static: bool,
    /// `private`.
    pub is_private: bool,
    /// `protected`.
    pub is_protected: bool,
    /// `private(set)`.
    pub set_is_private: bool,
    /// `protected(set)`.
    pub set_is_protected: bool,
    /// `readonly`.
    pub is_readonly: bool,
    /// Has a declared type.
    pub is_typed: bool,
}

/// Executable property hook metadata.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassPropertyHooks {
    /// `get` hook function.
    pub get: Option<FunctionId>,
    /// `set` hook function.
    pub set: Option<FunctionId>,
    /// True when normal property storage is materialized.
    pub backed: bool,
}

/// Class constant table entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassConstantEntry {
    /// Constant name without the class qualifier.
    pub name: String,
    /// Constant-pool value when the MVP can lower it.
    pub value: Option<ConstId>,
    /// Constant flags captured from Semantic frontend.
    pub flags: ClassConstantFlags,
    /// Attribute metadata attached to this class constant declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Class constant flags.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassConstantFlags {
    /// `private`.
    pub is_private: bool,
    /// `protected`.
    pub is_protected: bool,
}

/// Enum backing type metadata.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClassEnumBackingType {
    /// `int` backed enum.
    Int,
    /// `string` backed enum.
    String,
}

/// Enum case table entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassEnumCaseEntry {
    /// Case name without the class qualifier.
    pub name: String,
    /// Backing value for backed enum cases.
    pub value: Option<ConstId>,
    /// Attribute metadata attached to this enum case declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Runtime/reflection-visible attribute metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttributeEntry {
    /// Source-spelled attribute name.
    pub name: String,
    /// Resolved canonical class name, when Semantic frontend resolved it.
    pub resolved_name: Option<String>,
    /// Runtime fallback class name, when PHP may resolve dynamically.
    pub fallback_name: Option<String>,
    /// Constant-pool argument values in source order.
    pub arguments: Vec<ConstId>,
    /// True when this attribute name appears repeatedly on the same target.
    pub repeated_on_target: bool,
    /// Source span for the attribute.
    pub span: IrSpan,
}

/// Named function lookup entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FunctionEntry {
    /// Normalized lookup name.
    pub name: String,
    /// Function table ID.
    pub function: FunctionId,
}

/// Runtime-visible constant lookup entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GlobalConstantEntry {
    /// Canonical runtime lookup name.
    pub name: String,
    /// Constant-pool value.
    pub value: crate::ids::ConstId,
    /// Source span for the constant declaration.
    pub span: IrSpan,
}

/// Compiled IR unit.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct IrUnit {
    /// IR version.
    pub version: u32,
    /// Unit ID.
    pub id: UnitId,
    /// Constant pool.
    pub constants: Vec<IrConstant>,
    /// Function table.
    pub functions: Vec<IrFunction>,
    /// Deterministic normalized function-name lookup table.
    pub function_table: Vec<FunctionEntry>,
    /// Deterministic runtime constant lookup table.
    pub constant_table: Vec<GlobalConstantEntry>,
    /// Class skeleton table.
    pub classes: Vec<ClassEntry>,
    /// File/source table.
    pub files: Vec<FileEntry>,
    /// Entry function.
    pub entry: FunctionId,
    /// File-level `declare(strict_types=1)` for the current single-file unit.
    pub strict_types: bool,
    /// IR-to-HIR/source mapping.
    pub source_map: IrSourceMap,
}

impl IrUnit {
    /// Creates an empty unit.
    #[must_use]
    pub fn new(id: UnitId) -> Self {
        Self {
            version: IR_VERSION,
            id,
            constants: Vec::new(),
            functions: Vec::new(),
            function_table: Vec::new(),
            constant_table: Vec::new(),
            classes: Vec::new(),
            files: Vec::new(),
            entry: FunctionId::new(0),
            strict_types: false,
            source_map: IrSourceMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{display_class_name, normalize_class_name};

    #[test]
    fn class_name_lookup_is_case_insensitive_without_root_slash() {
        assert_eq!(normalize_class_name("\\App\\Thing"), "app\\thing");
        assert_eq!(
            normalize_class_name("DateTimeImmutable"),
            "datetimeimmutable"
        );
    }

    #[test]
    fn class_name_display_preserves_source_spelling_without_root_slash() {
        assert_eq!(display_class_name("\\App\\Thing"), "App\\Thing");
        assert_eq!(display_class_name("DateTimeImmutable"), "DateTimeImmutable");
    }
}
