//! Runtime callable and closure value payloads.

use crate::{ObjectRef, ReferenceCell, object::next_object_id, value::Value};
use std::sync::Arc;

/// Debug metadata PHP exposes when dumping a runtime `Closure` value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosureDebugInfo {
    /// Synthetic closure name, for example `{closure:/path/file.php:12}`.
    pub name: String,
    /// Source file where the closure was declared.
    pub file: String,
    /// Source line where the closure was declared.
    pub line: i64,
    /// Declaration parameter metadata exposed by `var_dump($closure)`.
    pub parameters: Vec<ClosureDebugParameter>,
}

/// One parameter entry exposed by closure debug output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosureDebugParameter {
    /// Parameter name without the leading `$`.
    pub name: String,
    /// True when callers must pass this argument.
    pub required: bool,
}

/// Lexical and dynamic class context carried by a runtime closure.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClosureContext {
    /// Optional owning dynamic VM unit for closures created by include/eval.
    pub owner_unit: Option<usize>,
    /// Lexical class scope captured when the closure was created.
    pub scope_class: Option<Arc<str>>,
    /// Late-bound class captured when the closure was created.
    pub called_class: Option<Arc<str>>,
    /// Declaring class captured when the closure was created.
    pub declaring_class: Option<Arc<str>>,
}

/// Runtime closure payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosurePayload {
    /// PHP-visible object handle for the Closure instance.
    pub id: u64,
    /// Raw `php_ir::FunctionId`.
    pub function: u32,
    /// Captured values in deterministic capture order.
    pub captures: Vec<ClosureCaptureValue>,
    /// Object bound as `$this` when the closure was created.
    pub bound_this: Option<ObjectRef>,
    /// Optional source metadata used by debug output.
    pub debug: Option<Box<ClosureDebugInfo>>,
    /// Lexical and dynamic class context.
    pub context: ClosureContext,
}

impl ClosurePayload {
    /// Creates a closure payload with a fresh PHP-visible Closure object handle.
    #[must_use]
    pub fn new(function: u32, captures: Vec<ClosureCaptureValue>) -> Self {
        Self {
            id: next_object_id(),
            function,
            captures,
            bound_this: None,
            debug: None,
            context: ClosureContext::default(),
        }
    }

    /// Attaches PHP debug metadata.
    #[must_use]
    pub fn with_debug(mut self, debug: Option<ClosureDebugInfo>) -> Self {
        self.debug = debug.map(Box::new);
        self
    }

    /// Attaches an object bound as `$this`.
    #[must_use]
    pub fn with_bound_this(mut self, bound_this: Option<ObjectRef>) -> Self {
        self.bound_this = bound_this;
        self
    }

    /// Attaches lexical class context.
    #[must_use]
    pub fn with_context(mut self, context: ClosureContext) -> Self {
        self.context = context;
        self
    }

    /// Stamps the owning dynamic VM unit that created this closure.
    #[must_use]
    pub fn with_owner_unit(mut self, owner_unit: Option<usize>) -> Self {
        self.context.owner_unit = owner_unit;
        self
    }
}

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
    Closure(ClosurePayload),
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
