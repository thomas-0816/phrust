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

/// Closed integer interval proved by SSA construction or publication.
///
/// `None` on [`SsaValueFact`] means that the value may occupy the complete
/// PHP integer domain. Keeping the interval on the value fact makes
/// overflow, divisor, and shift admission a producer/consumer property
/// instead of an operation-local runtime guess.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SsaIntegerRange {
    pub minimum: i64,
    pub maximum: i64,
}

impl SsaIntegerRange {
    pub const FULL: Self = Self {
        minimum: i64::MIN,
        maximum: i64::MAX,
    };

    #[must_use]
    pub const fn exact(value: i64) -> Self {
        Self {
            minimum: value,
            maximum: value,
        }
    }

    #[must_use]
    pub const fn new(minimum: i64, maximum: i64) -> Option<Self> {
        if minimum <= maximum {
            Some(Self { minimum, maximum })
        } else {
            None
        }
    }

    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        Self::new(
            self.minimum.checked_add(other.minimum)?,
            self.maximum.checked_add(other.maximum)?,
        )
    }

    #[must_use]
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        Self::new(
            self.minimum.checked_sub(other.maximum)?,
            self.maximum.checked_sub(other.minimum)?,
        )
    }

    #[must_use]
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        let products = [
            self.minimum.checked_mul(other.minimum)?,
            self.minimum.checked_mul(other.maximum)?,
            self.maximum.checked_mul(other.minimum)?,
            self.maximum.checked_mul(other.maximum)?,
        ];
        Some(Self {
            minimum: *products.iter().min()?,
            maximum: *products.iter().max()?,
        })
    }

    #[must_use]
    pub const fn excludes(self, value: i64) -> bool {
        value < self.minimum || value > self.maximum
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            minimum: if self.minimum < other.minimum {
                self.minimum
            } else {
                other.minimum
            },
            maximum: if self.maximum > other.maximum {
                self.maximum
            } else {
                other.maximum
            },
        }
    }
}

/// One independently tracked value/class/ownership fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SsaValueFact {
    pub class: SsaValueClass,
    pub certainty: SsaCertainty,
    pub ownership: SsaOwnership,
    pub integer_range: Option<SsaIntegerRange>,
}

impl SsaValueFact {
    pub const UNKNOWN: Self = Self {
        class: SsaValueClass::MixedHandle,
        certainty: SsaCertainty::Unknown,
        ownership: SsaOwnership::Unknown,
        integer_range: None,
    };

    #[must_use]
    pub const fn exact(class: SsaValueClass, ownership: SsaOwnership) -> Self {
        Self {
            class,
            certainty: SsaCertainty::Exact,
            ownership,
            integer_range: None,
        }
    }

    #[must_use]
    pub const fn known(class: SsaValueClass, ownership: SsaOwnership) -> Self {
        Self {
            class,
            certainty: SsaCertainty::KnownClass,
            ownership,
            integer_range: None,
        }
    }

    #[must_use]
    pub const fn with_integer_range(self, integer_range: SsaIntegerRange) -> Self {
        Self {
            integer_range: Some(integer_range),
            ..self
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
            // Most integers stay untagged machine values, but namespace-
            // colliding integers use authoritative direct-int slots. Floats
            // likewise live in direct slots. Their SSA class therefore owns
            // a conditional native lifecycle even though both remain exact
            // PHP scalars; retain/release lower to no-ops for immediates.
            SsaValueClass::Int
                | SsaValueClass::Float
                | SsaValueClass::StringHandle
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
