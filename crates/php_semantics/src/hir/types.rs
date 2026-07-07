//! HIR type records.

use crate::hir::{FullyQualifiedName, QualifiedName, TypeId};

/// Type record stored in the module type arena.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirType {
    kind: HirTypeKind,
    context: TypeContext,
    source_form: String,
}

impl HirType {
    /// Creates a type record.
    #[must_use]
    pub fn new(kind: HirTypeKind, context: TypeContext, source_form: impl Into<String>) -> Self {
        Self {
            kind,
            context,
            source_form: source_form.into(),
        }
    }

    /// Returns the type kind.
    #[must_use]
    pub const fn kind(&self) -> &HirTypeKind {
        &self.kind
    }

    /// Returns the source context in which this type appeared.
    #[must_use]
    pub const fn context(&self) -> TypeContext {
        self.context
    }

    /// Returns the type source spelling with trivia removed.
    #[must_use]
    pub fn source_form(&self) -> &str {
        &self.source_form
    }
}

/// Type families produced by Semantic frontend type lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HirTypeKind {
    /// Lowering could not produce a complete type.
    Missing,
    /// Placeholder for a CST type that has not been lowered yet.
    Unlowered,
    /// Class-like or aliasable named type.
    Named {
        /// Source-level name.
        name: QualifiedName,
        /// Resolved fully-qualified class-like name when statically known.
        resolved: Option<FullyQualifiedName>,
    },
    /// Builtin type atom.
    Builtin(BuiltinType),
    /// Nullable type syntax.
    Nullable {
        /// Inner type.
        inner: TypeId,
        /// Whether the equivalent normalized union includes `null`.
        normalized_null: bool,
    },
    /// Union type syntax.
    Union {
        /// Member type IDs in source order.
        members: Vec<TypeId>,
        /// True when this came from `?T` normalization.
        normalized_from_nullable: bool,
    },
    /// Intersection type syntax.
    Intersection {
        /// Member type IDs in source order.
        members: Vec<TypeId>,
    },
    /// DNF-shaped type syntax.
    Dnf {
        /// Member type IDs in source order.
        members: Vec<TypeId>,
    },
    /// `self`.
    SelfType,
    /// `parent`.
    ParentType,
    /// `static`.
    StaticType,
    /// `mixed`.
    Mixed,
    /// `never`.
    Never,
    /// `void`.
    Void,
    /// `null`.
    Null,
    /// `false`.
    False,
    /// `true`.
    True,
    /// Array with typed elements (`int[]`, `string[]`, `Test[]`).
    ArrayOf {
        /// Element type.
        element_type: TypeId,
    },
}

/// Builtin type atoms that are not special context keywords.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinType {
    /// `array`.
    Array,
    /// `callable`.
    Callable,
    /// `object`.
    Object,
    /// `iterable`.
    Iterable,
    /// `bool`.
    Bool,
    /// `int`.
    Int,
    /// `float`.
    Float,
    /// `string`.
    String,
}

impl BuiltinType {
    /// Returns stable lowercase JSON text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Array => "array",
            Self::Callable => "callable",
            Self::Object => "object",
            Self::Iterable => "iterable",
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Float => "float",
            Self::String => "string",
        }
    }
}

/// Source context in which a type annotation appears.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeContext {
    /// Function, method, closure, or arrow-function parameter.
    Parameter,
    /// Function-like return type.
    Return,
    /// Class property type.
    Property,
    /// Class constant type.
    ClassConstant,
    /// Closure `use` list, if future parser shapes expose one.
    ClosureUse,
    /// `catch` type list.
    Catch,
    /// Enum backing type.
    EnumBacking,
}

impl TypeContext {
    /// Returns stable lowercase JSON text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parameter => "parameter",
            Self::Return => "return",
            Self::Property => "property",
            Self::ClassConstant => "class_constant",
            Self::ClosureUse => "closure_use",
            Self::Catch => "catch",
            Self::EnumBacking => "enum_backing",
        }
    }
}
