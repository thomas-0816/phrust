//! Minimal runtime value model for early VM execution.

use crate::{
    FiberRef, GeneratorRef, ObjectRef, PhpArray, ReferenceCell, ResourceRef,
    object::next_object_id, string::PhpString,
};
use std::fmt;

/// Debug metadata PHP exposes when dumping a runtime `Closure` value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosureDebugInfo {
    /// Synthetic closure name, for example `{closure:/path/file.php:12}`.
    pub name: String,
    /// Source file where the closure was declared.
    pub file: String,
    /// Source line where the closure was declared.
    pub line: i64,
}

/// Borrowed runtime closure payload.
pub type ClosurePayloadRef<'a> = (
    u32,
    &'a Vec<ClosureCaptureValue>,
    Option<&'a ObjectRef>,
    Option<&'a ClosureDebugInfo>,
    Option<&'a String>,
    Option<&'a String>,
    Option<&'a String>,
);

/// Runtime callable values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallableValue {
    /// User function resolved by normalized function name.
    UserFunction {
        /// Normalized function name.
        name: String,
    },
    /// runtime closure value. The function ID is stored as its stable raw IR ID
    /// to keep `php_runtime` independent from `php_ir`.
    Closure {
        /// PHP-visible object handle for the Closure instance.
        id: u64,
        /// Raw `php_ir::FunctionId`.
        function: u32,
        /// Captured values in deterministic capture order.
        captures: Vec<ClosureCaptureValue>,
        /// Object bound as `$this` when the closure was created.
        bound_this: Option<ObjectRef>,
        /// Optional source metadata used by debug output.
        debug: Option<ClosureDebugInfo>,
        /// Lexical class scope captured when the closure was created.
        scope_class: Option<String>,
        /// Late-bound class captured when the closure was created.
        called_class: Option<String>,
        /// Declaring class captured when the closure was created.
        declaring_class: Option<String>,
    },
    /// Internal builtin resolved by deterministic builtin name.
    InternalBuiltin {
        /// Normalized builtin name.
        name: String,
    },
    /// Method callable acquired through first-class callable syntax.
    BoundMethod {
        /// Bound object or class target.
        target: CallableMethodTarget,
        /// Method name using PHP-visible spelling from the acquisition site.
        method: String,
        /// Class scope active when the callable was acquired.
        scope: Option<String>,
    },
    /// Placeholder for method callables until object/method runtime exists.
    MethodPlaceholder {
        /// Stable human-readable target description.
        target: String,
    },
    /// Explicit unresolved dynamic callable gap.
    UnresolvedDynamic {
        /// Stable human-readable target description.
        target: String,
    },
}

/// Runtime target for an acquired method callable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallableMethodTarget {
    /// Instance method target.
    Object(ObjectRef),
    /// Static method target class name.
    Class(String),
}

/// One value captured into a closure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosureCaptureValue {
    /// Captured variable name without `$`.
    pub name: String,
    /// Captured by-value snapshot. `None` means this capture aliases
    /// `reference`.
    pub value: Option<Value>,
    /// Captured by-reference cell. `None` means this capture uses `value`.
    pub reference: Option<ReferenceCell>,
}

impl ClosureCaptureValue {
    /// Creates a by-value closure capture.
    #[must_use]
    pub fn by_value(name: String, value: Value) -> Self {
        Self {
            name,
            value: Some(value),
            reference: None,
        }
    }

    /// Creates a by-reference closure capture.
    #[must_use]
    pub fn by_reference(name: String, reference: ReferenceCell) -> Self {
        Self {
            name,
            value: None,
            reference: Some(reference),
        }
    }

    /// Returns a by-value snapshot.
    #[must_use]
    pub fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// Returns a by-reference cell.
    #[must_use]
    pub fn reference(&self) -> Option<ReferenceCell> {
        self.reference.clone()
    }
}

/// Runtime value carried by the VM.
#[derive(Clone, Eq, PartialEq)]
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
    /// Callable placeholder.
    Callable(CallableValue),
    /// Reference cell exposed as a value only for explicit future reference
    /// operations. Ordinary local aliasing should use `ValueSlot`.
    Reference(ReferenceCell),
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

    /// Creates a runtime closure callable value.
    #[must_use]
    pub fn closure(function: u32, captures: Vec<ClosureCaptureValue>) -> Self {
        Self::Callable(CallableValue::Closure {
            id: next_object_id(),
            function,
            captures,
            bound_this: None,
            debug: None,
            scope_class: None,
            called_class: None,
            declaring_class: None,
        })
    }

    /// Creates a runtime closure callable value with PHP debug metadata.
    #[must_use]
    pub fn closure_with_debug(
        function: u32,
        captures: Vec<ClosureCaptureValue>,
        debug: Option<ClosureDebugInfo>,
    ) -> Self {
        Self::Callable(CallableValue::Closure {
            id: next_object_id(),
            function,
            captures,
            bound_this: None,
            debug,
            scope_class: None,
            called_class: None,
            declaring_class: None,
        })
    }

    /// Creates a runtime closure callable value with PHP debug metadata and an
    /// object bound as `$this`.
    #[must_use]
    pub fn closure_with_debug_and_this(
        function: u32,
        captures: Vec<ClosureCaptureValue>,
        debug: Option<ClosureDebugInfo>,
        bound_this: Option<ObjectRef>,
    ) -> Self {
        Self::Callable(CallableValue::Closure {
            id: next_object_id(),
            function,
            captures,
            bound_this,
            debug,
            scope_class: None,
            called_class: None,
            declaring_class: None,
        })
    }

    /// Creates a runtime closure callable value with PHP debug metadata, an
    /// optional bound `$this`, and lexical class context.
    #[must_use]
    pub fn closure_with_debug_this_and_context(
        function: u32,
        captures: Vec<ClosureCaptureValue>,
        debug: Option<ClosureDebugInfo>,
        bound_this: Option<ObjectRef>,
        scope_class: Option<String>,
        called_class: Option<String>,
        declaring_class: Option<String>,
    ) -> Self {
        Self::Callable(CallableValue::Closure {
            id: next_object_id(),
            function,
            captures,
            bound_this,
            debug,
            scope_class,
            called_class,
            declaring_class,
        })
    }

    /// Creates a resolved user-function callable value.
    #[must_use]
    pub fn user_function_callable(name: impl Into<String>) -> Self {
        Self::Callable(CallableValue::UserFunction { name: name.into() })
    }

    /// Creates a resolved internal-builtin callable value.
    #[must_use]
    pub fn internal_builtin_callable(name: impl Into<String>) -> Self {
        Self::Callable(CallableValue::InternalBuiltin { name: name.into() })
    }

    /// Creates a bound method callable value.
    #[must_use]
    pub fn bound_method_callable(
        target: CallableMethodTarget,
        method: impl Into<String>,
        scope: Option<String>,
    ) -> Self {
        Self::Callable(CallableValue::BoundMethod {
            target,
            method: method.into(),
            scope,
        })
    }

    /// Creates a method-callable placeholder value.
    #[must_use]
    pub fn method_callable_placeholder(target: impl Into<String>) -> Self {
        Self::Callable(CallableValue::MethodPlaceholder {
            target: target.into(),
        })
    }

    /// Creates an unresolved dynamic callable gap value.
    #[must_use]
    pub fn unresolved_callable(target: impl Into<String>) -> Self {
        Self::Callable(CallableValue::UnresolvedDynamic {
            target: target.into(),
        })
    }

    /// Returns closure payload when this value is a runtime closure.
    #[must_use]
    pub fn as_closure(&self) -> Option<ClosurePayloadRef<'_>> {
        match self {
            Self::Callable(CallableValue::Closure {
                function,
                captures,
                bound_this,
                debug,
                scope_class,
                called_class,
                declaring_class,
                ..
            }) => Some((
                *function,
                captures,
                bound_this.as_ref(),
                debug.as_ref(),
                scope_class.as_ref(),
                called_class.as_ref(),
                declaring_class.as_ref(),
            )),
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
            Self::Callable(CallableValue::UserFunction { name }) => f
                .debug_struct("Callable")
                .field("kind", &"user_function")
                .field("name", name)
                .finish(),
            Self::Callable(CallableValue::Closure {
                function, captures, ..
            }) => f
                .debug_struct("Closure")
                .field("function", function)
                .field(
                    "captures",
                    &captures
                        .iter()
                        .map(|capture| capture.name.as_str())
                        .collect::<Vec<_>>(),
                )
                .finish(),
            Self::Callable(CallableValue::InternalBuiltin { name }) => f
                .debug_struct("Callable")
                .field("kind", &"internal_builtin")
                .field("name", name)
                .finish(),
            Self::Callable(CallableValue::BoundMethod {
                target,
                method,
                scope,
            }) => f
                .debug_struct("Callable")
                .field("kind", &"bound_method")
                .field("target", target)
                .field("method", method)
                .field("scope", scope)
                .finish(),
            Self::Callable(CallableValue::MethodPlaceholder { target }) => f
                .debug_struct("Callable")
                .field("kind", &"method_placeholder")
                .field("target", target)
                .finish(),
            Self::Callable(CallableValue::UnresolvedDynamic { target }) => f
                .debug_struct("Callable")
                .field("kind", &"unresolved_dynamic")
                .field("target", target)
                .finish(),
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
