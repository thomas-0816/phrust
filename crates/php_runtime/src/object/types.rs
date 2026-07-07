/// Minimal runtime type adapter used by the VM for Semantic frontend annotations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeType {
    /// `int`
    Int,
    /// `float`
    Float,
    /// `string`
    String,
    /// `array` or `T[]` (typed array).
    Array {
        /// Element type for typed arrays (`T[]`), `None` for bare `array`.
        element_type: Option<Box<RuntimeType>>,
    },
    /// `callable`
    Callable,
    /// `iterable`
    Iterable,
    /// `object`
    Object,
    /// `bool`
    Bool,
    /// `null`
    Null,
    /// `void`
    Void,
    /// `mixed`
    Mixed,
    /// `never`
    Never,
    /// Literal `false`.
    False,
    /// Literal `true`.
    True,
    /// Class-like type.
    Class {
        /// Normalized lookup name.
        name: String,
        /// Source-spelled type name for PHP-visible diagnostics/reflection.
        display_name: Option<String>,
    },
    /// Nullable simple type.
    Nullable { inner: Box<RuntimeType> },
    /// Union type; matches when any member matches.
    Union { members: Vec<RuntimeType> },
    /// Intersection type; matches when every member matches.
    Intersection { members: Vec<RuntimeType> },
    /// Disjunctive normal form; each clause is usually an intersection.
    Dnf { clauses: Vec<RuntimeType> },
}
