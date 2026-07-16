//! Value and ownership facts consumed by executable Region IR lowering.
//!
//! Cranelift's `Variable` construction supplies the final machine SSA and phi
//! nodes. These facts are the PHP-specific half of that SSA contract: they
//! decide which values may remain unboxed, which locals may be promoted, and
//! where a runtime ownership boundary is still required.

mod executable;

pub use executable::{ExecutableSsaGraph, build_executable_ssa};

/// PHP-visible value classes tracked independently from ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SsaValueClass {
    Uninitialized,
    Null,
    Bool,
    Int,
    Float,
    StringHandle,
    ArrayHandle,
    ObjectHandle,
    ReferenceHandle,
    CallableHandle,
    ResourceHandle,
    GeneratorHandle,
    FiberHandle,
    MixedHandle,
}

/// Strength of one value-class fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SsaCertainty {
    /// The producer guarantees the exact PHP value class.
    Exact,
    /// Multiple paths agree on a class, but not a particular payload.
    KnownClass,
    /// The class is not statically constrained.
    Unknown,
}

/// Compiler-visible ownership of a native value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SsaOwnership {
    ImmortalConstant,
    Borrowed,
    Owned,
    Moved,
    Escaped,
    AliasedReference,
    Unknown,
}

/// One independently tracked value/class/ownership fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SsaValueFact {
    pub class: SsaValueClass,
    pub certainty: SsaCertainty,
    pub ownership: SsaOwnership,
}

impl SsaValueFact {
    pub const UNKNOWN: Self = Self {
        class: SsaValueClass::MixedHandle,
        certainty: SsaCertainty::Unknown,
        ownership: SsaOwnership::Unknown,
    };

    #[must_use]
    pub const fn exact(class: SsaValueClass, ownership: SsaOwnership) -> Self {
        Self {
            class,
            certainty: SsaCertainty::Exact,
            ownership,
        }
    }

    #[must_use]
    pub const fn known(class: SsaValueClass, ownership: SsaOwnership) -> Self {
        Self {
            class,
            certainty: SsaCertainty::KnownClass,
            ownership,
        }
    }

    #[must_use]
    pub const fn is_exact_scalar(self) -> bool {
        !matches!(self.certainty, SsaCertainty::Unknown)
            && matches!(
                self.class,
                SsaValueClass::Null
                    | SsaValueClass::Bool
                    | SsaValueClass::Int
                    | SsaValueClass::Float
            )
    }

    #[must_use]
    pub const fn has_runtime_lifecycle(self) -> bool {
        matches!(
            self.class,
            SsaValueClass::StringHandle
                | SsaValueClass::ArrayHandle
                | SsaValueClass::ObjectHandle
                | SsaValueClass::ReferenceHandle
                | SsaValueClass::CallableHandle
                | SsaValueClass::ResourceHandle
                | SsaValueClass::GeneratorHandle
                | SsaValueClass::FiberHandle
                | SsaValueClass::MixedHandle
        )
    }
}
