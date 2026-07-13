//! Minimal runtime value model for early VM execution.

use crate::{
    CallableMethodTarget, CallableValue, ClosurePayload, FiberRef, GeneratorRef, ObjectRef,
    PackedArrayValues, PhpArray, ReferenceCell, ResourceRef, string::PhpString,
};
use std::fmt;

/// Runtime value carried by the VM.
#[derive(Eq, PartialEq)]
pub enum Value {
    /// PHP null.
    Null,
    /// PHP boolean.
    Bool(bool),
    /// PHP integer.
    Int(i64),
    /// PHP floating-point value.
    Float(FloatValue),
    /// PHP byte string.
    String(PhpString),
    /// Uninitialized slot marker for registers/locals.
    Uninitialized,
    /// PHP array ordered-map facade.
    Array(PhpArray),
    /// Runtime object reference.
    Object(ObjectRef),
    /// PHP resource handle.
    Resource(ResourceRef),
    /// Internal fiber object.
    Fiber(FiberRef),
    /// Internal generator object.
    Generator(GeneratorRef),
    /// Callable placeholder. Boxed: `CallableValue` is ~160 bytes and would
    /// otherwise dominate `size_of::<Value>()`, which every register/local
    /// slot write and clone pays for.
    Callable(Box<CallableValue>),
    /// Reference cell exposed as a value only for explicit future reference
    /// operations. Ordinary local aliasing should use `ValueSlot`.
    Reference(ReferenceCell),
}

/// Register files and packed arrays memcpy `Value` on every clone; keep it
/// small. Every heap variant is a single pointer-sized handle (identities
/// live inside the shared cells), which is what holds the 16-byte layout.
/// If this assertion fails you added a fat variant - box its payload or
/// fold its side data into the pointee instead.
const _: () = assert!(std::mem::size_of::<Value>() <= 16);
const _: () = assert!(std::mem::size_of::<Option<Value>>() <= 16);

impl Value {
    /// Creates a callable value, boxing the payload.
    #[must_use]
    pub fn callable(callable: CallableValue) -> Self {
        Self::Callable(Box::new(callable))
    }

    /// Returns the callable payload when this value is a callable.
    #[must_use]
    pub fn as_callable(&self) -> Option<&CallableValue> {
        match self {
            Self::Callable(callable) => Some(callable),
            _ => None,
        }
    }

    /// Consumes this value, returning the boxed callable payload when it is
    /// a callable.
    #[must_use]
    pub fn into_callable(self) -> Option<Box<CallableValue>> {
        match self {
            Self::Callable(callable) => Some(callable),
            _ => None,
        }
    }
}

impl Clone for Value {
    fn clone(&self) -> Self {
        use crate::layout_stats::ValueCloneKind;

        crate::layout_stats::record_value_clone(|| match self {
            Self::Null | Self::Bool(_) | Self::Int(_) | Self::Float(_) | Self::Uninitialized => {
                ValueCloneKind::ScalarOrUninitialized
            }
            Self::String(_) => ValueCloneKind::StringHandle,
            Self::Array(_) => ValueCloneKind::ArrayHandle,
            Self::Object(_) => ValueCloneKind::ObjectHandle,
            Self::Reference(_) => ValueCloneKind::ReferenceCellHandle,
            Self::Resource(_) => ValueCloneKind::ResourceHandle,
            Self::Callable(_) => ValueCloneKind::CallableBox,
            Self::Fiber(_) | Self::Generator(_) => ValueCloneKind::FiberOrGeneratorHandle,
        });
        match self {
            Self::Null => Self::Null,
            Self::Bool(value) => Self::Bool(*value),
            Self::Int(value) => Self::Int(*value),
            Self::Float(value) => Self::Float(*value),
            Self::String(value) => Self::String(value.clone()),
            Self::Uninitialized => Self::Uninitialized,
            Self::Array(value) => Self::Array(value.clone()),
            Self::Object(value) => Self::Object(value.clone()),
            Self::Resource(value) => Self::Resource(value.clone()),
            Self::Fiber(value) => Self::Fiber(value.clone()),
            Self::Generator(value) => Self::Generator(value.clone()),
            Self::Callable(value) => Self::Callable(value.clone()),
            Self::Reference(value) => Self::Reference(value.clone()),
        }
    }
}

/// Equatable wrapper around an `f64` bit pattern.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct FloatValue(u64);

impl FloatValue {
    /// Stores the exact `f64` bit pattern.
    #[must_use]
    pub const fn from_f64(value: f64) -> Self {
        Self(value.to_bits())
    }

    /// Returns the represented `f64`.
    #[must_use]
    pub const fn to_f64(self) -> f64 {
        f64::from_bits(self.0)
    }
}

impl From<f64> for FloatValue {
    fn from(value: f64) -> Self {
        Self::from_f64(value)
    }
}

impl Value {
    /// Creates a PHP string value from bytes.
    #[must_use]
    pub fn string(bytes: impl Into<Vec<u8>>) -> Self {
        Self::String(PhpString::from_bytes(bytes))
    }

    /// Creates a float value while preserving the exact bit pattern.
    #[must_use]
    pub const fn float(value: f64) -> Self {
        Self::Float(FloatValue::from_f64(value))
    }

    /// Returns true for the uninitialized slot marker.
    #[must_use]
    pub const fn is_uninitialized(&self) -> bool {
        matches!(self, Self::Uninitialized)
    }

    /// Returns a PHP string reference when this value is a string.
    #[must_use]
    pub const fn as_php_string(&self) -> Option<&PhpString> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// Creates a packed-array facade for early variadic arguments.
    #[must_use]
    pub fn packed_array(elements: Vec<Value>) -> Self {
        Self::Array(PhpArray::from_packed(elements))
    }

    /// Returns packed-array elements when the array keys are exactly `0..len`.
    #[must_use]
    pub fn packed_elements(&self) -> Option<Vec<&Value>> {
        match self {
            Self::Array(array) => array.packed_elements(),
            _ => None,
        }
    }

    /// Returns a non-allocating packed-array value iterator when tracked
    /// metadata proves the keys are exactly `0..len`.
    #[must_use]
    pub fn packed_values_fast(&self) -> Option<PackedArrayValues<'_>> {
        match self {
            Self::Array(array) => array.packed_values_fast(),
            _ => None,
        }
    }

    /// Creates a runtime closure callable value.
    #[must_use]
    pub fn closure(payload: ClosurePayload) -> Self {
        Self::Callable(Box::new(CallableValue::Closure(payload)))
    }

    /// Creates a resolved user-function callable value.
    #[must_use]
    pub fn user_function_callable(name: impl Into<String>) -> Self {
        Self::Callable(Box::new(CallableValue::UserFunction { name: name.into() }))
    }

    /// Creates a resolved internal-builtin callable value.
    #[must_use]
    pub fn internal_builtin_callable(name: impl Into<String>) -> Self {
        Self::Callable(Box::new(CallableValue::InternalBuiltin {
            name: name.into(),
        }))
    }

    /// Creates a bound method callable value.
    #[must_use]
    pub fn bound_method_callable(
        target: CallableMethodTarget,
        method: impl Into<String>,
        scope: Option<String>,
    ) -> Self {
        Self::Callable(Box::new(CallableValue::BoundMethod {
            target,
            method: method.into(),
            scope,
        }))
    }

    /// Creates a method-callable placeholder value.
    #[must_use]
    pub fn method_callable_placeholder(target: impl Into<String>) -> Self {
        Self::Callable(Box::new(CallableValue::MethodPlaceholder {
            target: target.into(),
        }))
    }

    /// Creates an unresolved dynamic callable gap value.
    #[must_use]
    pub fn unresolved_callable(target: impl Into<String>) -> Self {
        Self::Callable(Box::new(CallableValue::UnresolvedDynamic {
            target: target.into(),
        }))
    }

    /// Returns closure payload when this value is a runtime closure.
    #[must_use]
    pub fn as_closure(&self) -> Option<&ClosurePayload> {
        match self.as_callable()? {
            CallableValue::Closure(payload) => Some(payload),
            _ => None,
        }
    }
}

impl fmt::Debug for FloatValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_f64().fmt(f)
    }
}

impl fmt::Display for FloatValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_f64().fmt(f)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("Null"),
            Self::Bool(value) => f.debug_tuple("Bool").field(value).finish(),
            Self::Int(value) => f.debug_tuple("Int").field(value).finish(),
            Self::Float(value) => f.debug_tuple("Float").field(value).finish(),
            Self::String(value) => f.debug_tuple("String").field(value).finish(),
            Self::Uninitialized => f.write_str("Uninitialized"),
            Self::Array(array) => f.debug_struct("Array").field("len", &array.len()).finish(),
            Self::Object(object) => f
                .debug_struct("Object")
                .field("id", &object.id())
                .field("class_name", &object.class_name())
                .finish(),
            Self::Resource(resource) => f
                .debug_struct("Resource")
                .field("id", &resource.id().get())
                .field("type", &resource.resource_type())
                .field("open", &resource.is_open())
                .finish(),
            Self::Fiber(fiber) => f
                .debug_struct("Fiber")
                .field("id", &fiber.id())
                .field("state", &fiber.state())
                .finish(),
            Self::Generator(generator) => f
                .debug_struct("Generator")
                .field("id", &generator.id())
                .field("state", &generator.state())
                .finish(),
            Self::Callable(callable) => match callable.as_ref() {
                CallableValue::UserFunction { name } => f
                    .debug_struct("Callable")
                    .field("kind", &"user_function")
                    .field("name", name)
                    .finish(),
                CallableValue::Closure(payload) => f
                    .debug_struct("Closure")
                    .field("function", &payload.function)
                    .field(
                        "captures",
                        &payload
                            .captures
                            .iter()
                            .map(|capture| capture.name.as_str())
                            .collect::<Vec<_>>(),
                    )
                    .finish(),
                CallableValue::InternalBuiltin { name } => f
                    .debug_struct("Callable")
                    .field("kind", &"internal_builtin")
                    .field("name", name)
                    .finish(),
                CallableValue::BoundMethod {
                    target,
                    method,
                    scope,
                } => f
                    .debug_struct("Callable")
                    .field("kind", &"bound_method")
                    .field("target", target)
                    .field("method", method)
                    .field("scope", scope)
                    .finish(),
                CallableValue::MethodPlaceholder { target } => f
                    .debug_struct("Callable")
                    .field("kind", &"method_placeholder")
                    .field("target", target)
                    .finish(),
                CallableValue::UnresolvedDynamic { target } => f
                    .debug_struct("Callable")
                    .field("kind", &"unresolved_dynamic")
                    .field("target", target)
                    .finish(),
            },
            Self::Reference(_) => f.write_str("Reference(<placeholder>)"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("null"),
            Self::Bool(true) => f.write_str("true"),
            Self::Bool(false) => f.write_str("false"),
            Self::Int(value) => value.fmt(f),
            Self::Float(value) => value.fmt(f),
            Self::String(value) => value.fmt(f),
            Self::Uninitialized => f.write_str("<uninitialized>"),
            Self::Array(_) => f.write_str("<array>"),
            Self::Object(object) => f.write_str(&format!("object({})", object.class_name())),
            Self::Resource(resource) => f.write_str(&format!(
                "resource({}) of type ({})",
                resource.id().get(),
                resource.resource_type()
            )),
            Self::Fiber(_) => f.write_str("object(Fiber)"),
            Self::Generator(_) => f.write_str("object(Generator)"),
            Self::Callable(_) => f.write_str("<callable>"),
            Self::Reference(_) => f.write_str("<reference>"),
        }
    }
}
