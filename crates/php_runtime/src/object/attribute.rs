use crate::Value;

/// Runtime/reflection-visible attribute metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttributeEntry {
    /// Source-spelled attribute name.
    pub name: String,
    /// Resolved canonical class name, when Semantic frontend resolved it.
    pub resolved_name: Option<String>,
    /// Runtime fallback class name, when PHP may resolve dynamically.
    pub fallback_name: Option<String>,
    /// Runtime argument values in source order.
    pub arguments: Vec<Value>,
    /// True when this attribute name appears repeatedly on the same target.
    pub repeated_on_target: bool,
    /// Source span encoded as `(file, start, end)`.
    pub span: Option<(u32, u32, u32)>,
}
