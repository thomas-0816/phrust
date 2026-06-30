//! IR constant pool values.

use serde::{Deserialize, Serialize};

/// Literal constants stored in an IR unit.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum IrConstant {
    /// PHP `null`.
    Null,
    /// PHP boolean.
    Bool(bool),
    /// PHP integer.
    Int(i64),
    /// PHP float.
    Float(f64),
    /// PHP string bytes represented as UTF-8 for the MVP.
    String(String),
    /// PHP string bytes that cannot be represented losslessly as UTF-8.
    StringBytes(Vec<u8>),
    /// Runtime-resolved global constant in a constant-expression initializer.
    NamedConstant(String),
    /// Runtime-resolved class constant in a constant-expression initializer.
    ClassConstant {
        /// Class-like name as resolved by the semantic frontend when possible.
        class_name: String,
        /// Constant name without the class qualifier.
        constant_name: String,
    },
    /// PHP array literal whose keys and values are constant-pool values.
    Array(Vec<IrConstantArrayEntry>),
}

/// One constant PHP array entry.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct IrConstantArrayEntry {
    /// Explicit key. `None` means append with the next integer key.
    pub key: Option<IrConstant>,
    /// Stored value.
    pub value: IrConstant,
}
