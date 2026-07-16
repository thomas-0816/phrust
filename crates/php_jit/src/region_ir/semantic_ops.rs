//! Typed PHP semantic operations that cross the native-call boundary.
//!
//! These operations are deliberately separate from user-visible function
//! calls.  Their numeric IDs are ABI-visible and append-only: native code may
//! persist the ID in a call frame, while names and other PHP metadata remain
//! strongly typed in Region IR.

use php_ir::{IrSpan, LocalId};

use super::RegionOperand;

/// Stable runtime operation identifiers. Existing values must never be
/// renumbered or reused.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RegionSemanticOperationId {
    StaticPropertyFetch = 1,
    StaticPropertyAssign = 2,
    StaticPropertyIsset = 3,
    StaticPropertyEmpty = 4,
    StaticPropertyDimIsset = 5,
    StaticPropertyDimEmpty = 6,
    StaticPropertyDimUnset = 7,
    StaticPropertyReference = 8,
    ClassConstantFetch = 9,
    ObjectClassName = 10,
    InstanceOf = 11,
    DynamicInstanceOf = 12,
    ResolveCallable = 13,
    AcquireCallable = 14,
    PropertyFetch = 15,
    PropertyAssign = 16,
    PropertyIsset = 17,
    PropertyEmpty = 18,
    PropertyUnset = 19,
    PropertyDimAssign = 20,
    PropertyDimIsset = 21,
    PropertyDimEmpty = 22,
    PropertyDimUnset = 23,
    BindGlobal = 24,
    BoundClosureClass = 25,
}

impl RegionSemanticOperationId {
    #[must_use]
    pub const fn raw(self) -> u32 {
        self as u32
    }

    #[must_use]
    pub const fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            1 => Self::StaticPropertyFetch,
            2 => Self::StaticPropertyAssign,
            3 => Self::StaticPropertyIsset,
            4 => Self::StaticPropertyEmpty,
            5 => Self::StaticPropertyDimIsset,
            6 => Self::StaticPropertyDimEmpty,
            7 => Self::StaticPropertyDimUnset,
            8 => Self::StaticPropertyReference,
            9 => Self::ClassConstantFetch,
            10 => Self::ObjectClassName,
            11 => Self::InstanceOf,
            12 => Self::DynamicInstanceOf,
            13 => Self::ResolveCallable,
            14 => Self::AcquireCallable,
            15 => Self::PropertyFetch,
            16 => Self::PropertyAssign,
            17 => Self::PropertyIsset,
            18 => Self::PropertyEmpty,
            19 => Self::PropertyUnset,
            20 => Self::PropertyDimAssign,
            21 => Self::PropertyDimIsset,
            22 => Self::PropertyDimEmpty,
            23 => Self::PropertyDimUnset,
            24 => Self::BindGlobal,
            25 => Self::BoundClosureClass,
            _ => return None,
        })
    }
}

/// Source identity carried by every semantic operation for diagnostics,
/// exception continuations, and native callback re-entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionSemanticContext {
    pub span: IrSpan,
    pub continuation_id: u32,
}

/// A property name resolved either statically or from a runtime operand.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionPropertyName {
    Static(String),
    Dynamic(RegionOperand),
}

/// A class resolved either lexically (`self`, `parent`, `static`, or a named
/// class) or from a runtime class-string/object operand.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionClassName {
    Static(String),
    Dynamic(RegionOperand),
}

/// One typed semantic operation. Object/property operands live here as well
/// as in the call's materialization list so validation never has to infer the
/// operation from a synthetic function name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionSemanticOp {
    StaticPropertyFetch {
        context: RegionSemanticContext,
        class_name: RegionClassName,
        property: String,
    },
    StaticPropertyAssign {
        context: RegionSemanticContext,
        class_name: RegionClassName,
        property: String,
        value: RegionOperand,
    },
    StaticPropertyIsset {
        context: RegionSemanticContext,
        class_name: RegionClassName,
        property: String,
    },
    StaticPropertyEmpty {
        context: RegionSemanticContext,
        class_name: RegionClassName,
        property: String,
    },
    StaticPropertyDimIsset {
        context: RegionSemanticContext,
        class_name: RegionClassName,
        property: String,
        dimensions: Vec<RegionOperand>,
    },
    StaticPropertyDimEmpty {
        context: RegionSemanticContext,
        class_name: RegionClassName,
        property: String,
        dimensions: Vec<RegionOperand>,
    },
    StaticPropertyDimUnset {
        context: RegionSemanticContext,
        class_name: RegionClassName,
        property: String,
        dimensions: Vec<RegionOperand>,
    },
    StaticPropertyReference {
        context: RegionSemanticContext,
        target: LocalId,
        class_name: RegionClassName,
        property: String,
        dimensions: Vec<RegionOperand>,
    },
    ClassConstantFetch {
        context: RegionSemanticContext,
        class_name: String,
        constant: String,
    },
    ObjectClassName {
        context: RegionSemanticContext,
        object: RegionOperand,
    },
    InstanceOf {
        context: RegionSemanticContext,
        object: RegionOperand,
        class_name: String,
    },
    DynamicInstanceOf {
        context: RegionSemanticContext,
        object: RegionOperand,
        target: RegionOperand,
    },
    ResolveCallable {
        context: RegionSemanticContext,
        callable: php_ir::instruction::CallableKind,
    },
    AcquireCallable {
        context: RegionSemanticContext,
        value: RegionOperand,
    },
    PropertyFetch {
        context: RegionSemanticContext,
        object: RegionOperand,
        property: RegionPropertyName,
    },
    PropertyAssign {
        context: RegionSemanticContext,
        object: RegionOperand,
        property: RegionPropertyName,
        value: RegionOperand,
    },
    PropertyIsset {
        context: RegionSemanticContext,
        object: RegionOperand,
        property: RegionPropertyName,
    },
    PropertyEmpty {
        context: RegionSemanticContext,
        object: RegionOperand,
        property: RegionPropertyName,
    },
    PropertyUnset {
        context: RegionSemanticContext,
        object: RegionOperand,
        property: RegionPropertyName,
    },
    PropertyDimAssign {
        context: RegionSemanticContext,
        object: RegionOperand,
        property: RegionPropertyName,
        dimensions: Vec<RegionOperand>,
        value: RegionOperand,
        append: bool,
    },
    PropertyDimIsset {
        context: RegionSemanticContext,
        object: RegionOperand,
        property: RegionPropertyName,
        dimensions: Vec<RegionOperand>,
    },
    PropertyDimEmpty {
        context: RegionSemanticContext,
        object: RegionOperand,
        property: RegionPropertyName,
        dimensions: Vec<RegionOperand>,
    },
    PropertyDimUnset {
        context: RegionSemanticContext,
        object: RegionOperand,
        property: RegionPropertyName,
        dimensions: Vec<RegionOperand>,
    },
    BindGlobal {
        context: RegionSemanticContext,
        local: LocalId,
        name: String,
    },
    BoundClosureClass {
        context: RegionSemanticContext,
        bound_object: RegionOperand,
    },
}

impl RegionSemanticOp {
    #[must_use]
    pub const fn operation_id(&self) -> RegionSemanticOperationId {
        match self {
            Self::StaticPropertyFetch { .. } => RegionSemanticOperationId::StaticPropertyFetch,
            Self::StaticPropertyAssign { .. } => RegionSemanticOperationId::StaticPropertyAssign,
            Self::StaticPropertyIsset { .. } => RegionSemanticOperationId::StaticPropertyIsset,
            Self::StaticPropertyEmpty { .. } => RegionSemanticOperationId::StaticPropertyEmpty,
            Self::StaticPropertyDimIsset { .. } => {
                RegionSemanticOperationId::StaticPropertyDimIsset
            }
            Self::StaticPropertyDimEmpty { .. } => {
                RegionSemanticOperationId::StaticPropertyDimEmpty
            }
            Self::StaticPropertyDimUnset { .. } => {
                RegionSemanticOperationId::StaticPropertyDimUnset
            }
            Self::StaticPropertyReference { .. } => {
                RegionSemanticOperationId::StaticPropertyReference
            }
            Self::ClassConstantFetch { .. } => RegionSemanticOperationId::ClassConstantFetch,
            Self::ObjectClassName { .. } => RegionSemanticOperationId::ObjectClassName,
            Self::InstanceOf { .. } => RegionSemanticOperationId::InstanceOf,
            Self::DynamicInstanceOf { .. } => RegionSemanticOperationId::DynamicInstanceOf,
            Self::ResolveCallable { .. } => RegionSemanticOperationId::ResolveCallable,
            Self::AcquireCallable { .. } => RegionSemanticOperationId::AcquireCallable,
            Self::PropertyFetch { .. } => RegionSemanticOperationId::PropertyFetch,
            Self::PropertyAssign { .. } => RegionSemanticOperationId::PropertyAssign,
            Self::PropertyIsset { .. } => RegionSemanticOperationId::PropertyIsset,
            Self::PropertyEmpty { .. } => RegionSemanticOperationId::PropertyEmpty,
            Self::PropertyUnset { .. } => RegionSemanticOperationId::PropertyUnset,
            Self::PropertyDimAssign { .. } => RegionSemanticOperationId::PropertyDimAssign,
            Self::PropertyDimIsset { .. } => RegionSemanticOperationId::PropertyDimIsset,
            Self::PropertyDimEmpty { .. } => RegionSemanticOperationId::PropertyDimEmpty,
            Self::PropertyDimUnset { .. } => RegionSemanticOperationId::PropertyDimUnset,
            Self::BindGlobal { .. } => RegionSemanticOperationId::BindGlobal,
            Self::BoundClosureClass { .. } => RegionSemanticOperationId::BoundClosureClass,
        }
    }

    #[must_use]
    pub const fn context(&self) -> RegionSemanticContext {
        match self {
            Self::StaticPropertyFetch { context, .. }
            | Self::StaticPropertyAssign { context, .. }
            | Self::StaticPropertyIsset { context, .. }
            | Self::StaticPropertyEmpty { context, .. }
            | Self::StaticPropertyDimIsset { context, .. }
            | Self::StaticPropertyDimEmpty { context, .. }
            | Self::StaticPropertyDimUnset { context, .. }
            | Self::StaticPropertyReference { context, .. }
            | Self::ClassConstantFetch { context, .. }
            | Self::ObjectClassName { context, .. }
            | Self::InstanceOf { context, .. }
            | Self::DynamicInstanceOf { context, .. }
            | Self::ResolveCallable { context, .. }
            | Self::AcquireCallable { context, .. }
            | Self::PropertyFetch { context, .. }
            | Self::PropertyAssign { context, .. }
            | Self::PropertyIsset { context, .. }
            | Self::PropertyEmpty { context, .. }
            | Self::PropertyUnset { context, .. }
            | Self::PropertyDimAssign { context, .. }
            | Self::PropertyDimIsset { context, .. }
            | Self::PropertyDimEmpty { context, .. }
            | Self::PropertyDimUnset { context, .. }
            | Self::BindGlobal { context, .. }
            | Self::BoundClosureClass { context, .. } => *context,
        }
    }

    #[must_use]
    pub fn materialized_operand_count(&self) -> usize {
        let class_operands = |class_name: &RegionClassName| {
            usize::from(matches!(class_name, RegionClassName::Dynamic(_)))
        };
        let property_operands = |property: &RegionPropertyName| {
            usize::from(matches!(property, RegionPropertyName::Dynamic(_)))
        };
        match self {
            Self::StaticPropertyFetch { class_name, .. }
            | Self::StaticPropertyIsset { class_name, .. }
            | Self::StaticPropertyEmpty { class_name, .. } => class_operands(class_name),
            Self::StaticPropertyAssign { class_name, .. } => class_operands(class_name) + 1,
            Self::StaticPropertyDimIsset {
                class_name,
                dimensions,
                ..
            }
            | Self::StaticPropertyDimEmpty {
                class_name,
                dimensions,
                ..
            }
            | Self::StaticPropertyDimUnset {
                class_name,
                dimensions,
                ..
            }
            | Self::StaticPropertyReference {
                class_name,
                dimensions,
                ..
            } => class_operands(class_name) + dimensions.len(),
            Self::ClassConstantFetch { .. }
            | Self::ResolveCallable { .. }
            | Self::BindGlobal { .. } => 0,
            Self::ObjectClassName { .. }
            | Self::InstanceOf { .. }
            | Self::AcquireCallable { .. }
            | Self::BoundClosureClass { .. } => 1,
            Self::DynamicInstanceOf { .. } => 2,
            Self::PropertyFetch { property, .. }
            | Self::PropertyIsset { property, .. }
            | Self::PropertyEmpty { property, .. }
            | Self::PropertyUnset { property, .. } => 1 + property_operands(property),
            Self::PropertyAssign { property, .. } => 2 + property_operands(property),
            Self::PropertyDimAssign {
                property,
                dimensions,
                ..
            } => 2 + property_operands(property) + dimensions.len(),
            Self::PropertyDimIsset {
                property,
                dimensions,
                ..
            }
            | Self::PropertyDimEmpty {
                property,
                dimensions,
                ..
            }
            | Self::PropertyDimUnset {
                property,
                dimensions,
                ..
            } => 1 + property_operands(property) + dimensions.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RegionSemanticOperationId as Id;

    #[test]
    fn semantic_operation_ids_are_dense_and_round_trip() {
        for raw in 1..=25 {
            let operation = Id::from_raw(raw).expect("declared semantic operation");
            assert_eq!(operation.raw(), raw);
        }
        assert_eq!(Id::from_raw(0), None);
        assert_eq!(Id::from_raw(26), None);
    }
}
