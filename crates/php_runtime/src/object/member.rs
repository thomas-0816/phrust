use super::{AttributeEntry, RuntimeType};
use crate::Value;

/// Runtime method table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassMethodEntry {
    /// Normalized method lookup name.
    pub name: String,
    /// Source class-like that contributed the method.
    pub origin_class: String,
    /// Raw IR function ID for the method body.
    pub function_id: u32,
    /// Method flags.
    pub flags: ClassMethodFlags,
    /// Runtime-visible attributes on this method declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Runtime method flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassMethodFlags {
    /// Static method.
    pub is_static: bool,
    /// Private method.
    pub is_private: bool,
    /// Protected method.
    pub is_protected: bool,
    /// Abstract method.
    pub is_abstract: bool,
    /// Final method.
    pub is_final: bool,
}

/// Runtime property table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassPropertyEntry {
    /// Property name without `$`.
    pub name: String,
    /// Default value for new instances.
    pub default: Value,
    /// Optional runtime type enforced on property writes.
    pub type_: Option<RuntimeType>,
    /// Property flags.
    pub flags: ClassPropertyFlags,
    /// Property hook functions.
    pub hooks: ClassPropertyHooks,
    /// Runtime-visible attributes on this property declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Runtime property flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassPropertyFlags {
    /// Static property.
    pub is_static: bool,
    /// Private property.
    pub is_private: bool,
    /// Protected property.
    pub is_protected: bool,
    /// Private setter.
    pub set_is_private: bool,
    /// Protected setter.
    pub set_is_protected: bool,
    /// Readonly property.
    pub is_readonly: bool,
    /// Typed property.
    pub is_typed: bool,
}

/// Runtime property hook metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClassPropertyHooks {
    /// Raw IR function ID for `get`.
    pub get_function_id: Option<u32>,
    /// Raw IR function ID for `set`.
    pub set_function_id: Option<u32>,
    /// True when normal property storage is materialized.
    pub backed: bool,
}

/// Runtime class constant table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassConstantEntry {
    /// Constant name without the class qualifier.
    pub name: String,
    /// Runtime value.
    pub value: Value,
    /// Constant flags.
    pub flags: ClassConstantFlags,
    /// Runtime-visible attributes on this class constant declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Runtime class constant flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassConstantFlags {
    /// Private constant.
    pub is_private: bool,
    /// Protected constant.
    pub is_protected: bool,
}

/// Runtime enum backing type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassEnumBackingType {
    /// `int` backed enum.
    Int,
    /// `string` backed enum.
    String,
}

/// Runtime enum case table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassEnumCaseEntry {
    /// Case name without the class qualifier.
    pub name: String,
    /// Case backing value, when backed.
    pub value: Option<Value>,
    /// Runtime-visible attributes on this enum case declaration.
    pub attributes: Vec<AttributeEntry>,
}
