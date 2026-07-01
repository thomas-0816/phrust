//! Runtime diagnostics shared by the VM and CLI.

use crate::builtins::RuntimeSourceSpan;
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticLocation, DiagnosticPhase, DiagnosticSeverity,
    DiagnosticSpan,
};
use std::collections::BTreeMap;

/// Runtime diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeSeverity {
    /// PHP warning; execution may continue.
    Warning,
    /// PHP notice.
    Notice,
    /// PHP deprecation notice.
    Deprecation,
    /// Recoverable runtime error.
    RecoverableError,
    /// Fatal runtime error.
    FatalError,
    /// Explicit unsupported feature.
    UnsupportedFeature,
}

/// PHP-visible runtime event family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeEventKind {
    /// PHP warning; execution may continue.
    Warning,
    /// PHP notice; execution may continue.
    Notice,
    /// PHP deprecation; execution may continue.
    Deprecation,
    /// Catchable PHP `Error`/`Exception` object.
    CatchableException,
    /// Fatal runtime error that terminates execution.
    FatalError,
    /// Explicit unsupported feature.
    UnsupportedFeature,
}

impl RuntimeEventKind {
    /// Stable JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Notice => "notice",
            Self::Deprecation => "deprecation",
            Self::CatchableException => "catchable_exception",
            Self::FatalError => "fatal_error",
            Self::UnsupportedFeature => "unsupported_feature",
        }
    }
}

impl RuntimeSeverity {
    /// Stable JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Notice => "notice",
            Self::Deprecation => "deprecation",
            Self::RecoverableError => "recoverable_error",
            Self::FatalError => "fatal_error",
            Self::UnsupportedFeature => "unsupported_feature",
        }
    }

    /// Shared diagnostic severity.
    #[must_use]
    pub const fn envelope_severity(self) -> DiagnosticSeverity {
        match self {
            Self::Warning => DiagnosticSeverity::Warning,
            Self::Notice => DiagnosticSeverity::Notice,
            Self::Deprecation => DiagnosticSeverity::Deprecation,
            Self::RecoverableError => DiagnosticSeverity::RecoverableError,
            Self::FatalError => DiagnosticSeverity::FatalError,
            Self::UnsupportedFeature => DiagnosticSeverity::UnsupportedFeature,
        }
    }
}

/// Optional PHP-reference classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpReferenceClassification {
    /// PHP warning.
    Warning,
    /// PHP notice.
    Notice,
    /// PHP deprecation.
    Deprecation,
    /// PHP `TypeError`.
    TypeError,
    /// PHP `ValueError`.
    ValueError,
    /// PHP `ArgumentCountError`.
    ArgumentCountError,
    /// PHP `DivisionByZeroError`.
    DivisionByZeroError,
    /// PHP `UnhandledMatchError`.
    UnhandledMatchError,
    /// PHP `Error`.
    Error,
    /// PHP fatal error.
    FatalError,
    /// Unsupported/deferred behavior in this runtime.
    Unsupported,
}

impl PhpReferenceClassification {
    /// Stable JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Notice => "notice",
            Self::Deprecation => "deprecation",
            Self::TypeError => "type_error",
            Self::ValueError => "value_error",
            Self::ArgumentCountError => "argument_count_error",
            Self::DivisionByZeroError => "division_by_zero_error",
            Self::UnhandledMatchError => "unhandled_match_error",
            Self::Error => "error",
            Self::FatalError => "fatal_error",
            Self::Unsupported => "unsupported",
        }
    }

    /// Returns the PHP Throwable class used by VM exception propagation.
    #[must_use]
    pub const fn throwable_class(self) -> Option<&'static str> {
        match self {
            Self::TypeError => Some("TypeError"),
            Self::ValueError => Some("ValueError"),
            Self::ArgumentCountError => Some("ArgumentCountError"),
            Self::DivisionByZeroError => Some("DivisionByZeroError"),
            Self::UnhandledMatchError => Some("UnhandledMatchError"),
            Self::Error => Some("Error"),
            Self::Warning
            | Self::Notice
            | Self::Deprecation
            | Self::FatalError
            | Self::Unsupported => None,
        }
    }

    /// Infers a PHP-reference classification from stable runtime diagnostic IDs.
    #[must_use]
    pub fn from_diagnostic_id(id: &str) -> Option<Self> {
        match id {
            "E_PHP_RUNTIME_BUILTIN_ARITY"
            | "E_PHP_STD_MISSING_ARGUMENT"
            | "E_PHP_STD_TOO_MANY_ARGUMENTS"
            | "E_PHP_VM_TOO_FEW_ARGS"
            | "E_PHP_VM_TOO_MANY_ARGS" => Some(Self::ArgumentCountError),
            "E_PHP_RUNTIME_BUILTIN_TYPE"
            | "E_PHP_STD_TYPE_ERROR"
            | "E_PHP_VM_SPL_TYPE_ERROR"
            | "E_PHP_RUNTIME_TYPE_ERROR"
            | "E_PHP_RUNTIME_NON_NUMERIC_STRING"
            | "E_PHP_RUNTIME_UNSUPPORTED_OPERAND_TYPES"
            | "E_PHP_VM_TOSTRING_RETURN_TYPE"
            | "E_PHP_VM_STRING_OFFSET_TYPE"
            | "E_PHP_VM_PARAM_TYPE_MISMATCH"
            | "E_PHP_VM_DYNAMIC_CLASS_NAME_TYPE"
            | "E_PHP_VM_AUTOLOAD_INVALID_CALLBACK"
            | "E_PHP_VM_PROPERTY_TYPE_MISMATCH"
            | "E_PHP_VM_FIRST_CLASS_CALLABLE_NOT_CALLABLE"
            | "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_FUNCTION"
            | "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_METHOD"
            | "E_PHP_VM_FIRST_CLASS_CALLABLE_NON_STATIC_METHOD"
            | "E_PHP_VM_FIRST_CLASS_CALLABLE_UNRESOLVED_DYNAMIC" => Some(Self::TypeError),
            "E_PHP_RUNTIME_BUILTIN_VALUE"
            | "E_PHP_RUNTIME_JSON_EXCEPTION"
            | "E_PHP_VM_SPL_VALUE_ERROR"
            | "E_PHP_STD_VALUE_ERROR" => Some(Self::ValueError),
            "E_PHP_RUNTIME_DIVISION_BY_ZERO" => Some(Self::DivisionByZeroError),
            "E_PHP_VM_UNHANDLED_MATCH" => Some(Self::UnhandledMatchError),
            "E_PHP_RUNTIME_UNDEFINED_FUNCTION"
            | "E_PHP_RUNTIME_OBJECT_TO_STRING_GAP"
            | "E_PHP_VM_BY_REF_ARG_NOT_REFERENCEABLE"
            | "E_PHP_VM_UNKNOWN_NAMED_ARG"
            | "E_PHP_VM_DUPLICATE_NAMED_ARG"
            | "E_PHP_VM_ARRAY_KEY_CONVERSION" => Some(Self::Error),
            "E_PHP_RUNTIME_UNDEFINED_VARIABLE_WARNING"
            | "E_PHP_RUNTIME_ARRAY_TO_STRING_WARNING"
            | "E_PHP_RUNTIME_NON_NUMERIC_STRING_WARNING"
            | "E_PHP_RUNTIME_OBJECT_NUMERIC_CAST_WARNING"
            | "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING" => Some(Self::Warning),
            id if id.contains("_DEPRECATED_") || id.contains("_DEPRECATION") => {
                Some(Self::Deprecation)
            }
            _ => None,
        }
    }
}

/// Structured VM compile diagnostic details.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmCompileDiagnostic {
    /// A child method reduces inherited visibility.
    MethodVisibilityOverride {
        class_name: String,
        method_name: String,
        required_visibility: String,
        parent_class_name: String,
        weaker_suffix: String,
    },
    /// A child method changes static-ness.
    StaticMethodOverride {
        class_name: String,
        method_name: String,
        parent_class_name: String,
        parent_is_static: bool,
    },
    /// A child method signature is incompatible with an inherited method.
    MethodSignatureOverride {
        class_name: String,
        method_name: String,
        actual_signature: String,
        expected_signature: String,
    },
    /// An interface method is not public.
    InterfaceMethodVisibility {
        class_name: String,
        method_name: String,
    },
    /// An interface method has a body.
    InterfaceMethodBody {
        class_name: String,
        method_name: String,
    },
    /// A class is missing an interface method implementation.
    InterfaceMethodMissing {
        class_name: String,
        interface_name: String,
        method_name: String,
    },
    /// A class method does not match an interface signature.
    InterfaceMethodSignature {
        class_name: String,
        method_name: String,
        actual_signature: String,
        expected_signature: String,
    },
    /// An interface constant is not public.
    InterfaceConstantVisibility {
        class_name: String,
        constant_name: String,
    },
    /// An interface contains an unsupported plain property.
    InterfaceProperty {
        class_name: String,
        property_name: String,
    },
    /// A class extends a final class.
    FinalClassExtend {
        class_name: String,
        parent_class_name: String,
    },
    /// A child method overrides a final method.
    FinalMethodOverride {
        class_name: String,
        method_name: String,
        parent_class_name: String,
    },
    /// A child property changes static-ness.
    PropertyStaticOverride {
        class_name: String,
        property_name: String,
        parent_class_name: String,
        parent_is_static: bool,
    },
    /// A child property reduces inherited visibility.
    PropertyVisibilityOverride {
        class_name: String,
        property_name: String,
        required_visibility: String,
        parent_class_name: String,
        weaker_suffix: String,
    },
    /// A child class constant reduces inherited visibility.
    ClassConstantVisibilityOverride {
        class_name: String,
        constant_name: String,
        required_visibility: String,
        parent_class_name: String,
        weaker_suffix: String,
    },
    /// A class extends an interface.
    ClassExtendsInterface {
        class_name: String,
        interface_name: String,
    },
    /// A class implements a non-interface.
    ImplementsNonInterface {
        class_name: String,
        target_name: String,
        message: String,
    },
    /// A class implements Traversable directly.
    TraversableDirectImplementation { class_name: String },
}

impl VmCompileDiagnostic {
    /// Stable diagnostic ID.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
            Self::MethodVisibilityOverride { .. } => "E_PHP_VM_METHOD_VISIBILITY_OVERRIDE",
            Self::StaticMethodOverride { .. } => "E_PHP_VM_STATIC_METHOD_OVERRIDE",
            Self::MethodSignatureOverride { .. } => "E_PHP_VM_METHOD_SIGNATURE_OVERRIDE",
            Self::InterfaceMethodVisibility { .. } => "E_PHP_VM_INTERFACE_METHOD_VISIBILITY",
            Self::InterfaceMethodBody { .. } => "E_PHP_VM_INTERFACE_METHOD_BODY",
            Self::InterfaceMethodMissing { .. } => "E_PHP_VM_INTERFACE_METHOD_MISSING",
            Self::InterfaceMethodSignature { .. } => "E_PHP_VM_INTERFACE_METHOD_SIGNATURE",
            Self::InterfaceConstantVisibility { .. } => "E_PHP_VM_INTERFACE_CONSTANT_VISIBILITY",
            Self::InterfaceProperty { .. } => "E_PHP_VM_INTERFACE_PROPERTY",
            Self::FinalClassExtend { .. } => "E_PHP_VM_FINAL_CLASS_EXTEND",
            Self::FinalMethodOverride { .. } => "E_PHP_VM_FINAL_METHOD_OVERRIDE",
            Self::PropertyStaticOverride { .. } => "E_PHP_VM_PROPERTY_STATIC_OVERRIDE",
            Self::PropertyVisibilityOverride { .. } => "E_PHP_VM_PROPERTY_VISIBILITY_OVERRIDE",
            Self::ClassConstantVisibilityOverride { .. } => {
                "E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE"
            }
            Self::ClassExtendsInterface { .. } => "E_PHP_VM_CLASS_EXTENDS_INTERFACE",
            Self::ImplementsNonInterface { .. } => "E_PHP_VM_IMPLEMENTS_NON_INTERFACE",
            Self::TraversableDirectImplementation { .. } => {
                "E_PHP_VM_TRAVERSABLE_DIRECT_IMPLEMENTATION"
            }
        }
    }

    /// PHP-compatible diagnostic message without the stable ID prefix.
    #[must_use]
    pub fn php_message(&self) -> String {
        match self {
            Self::MethodVisibilityOverride {
                class_name,
                method_name,
                required_visibility,
                parent_class_name,
                weaker_suffix,
            } => format!(
                "Access level to {class_name}::{method_name}() must be {required_visibility} (as in class {parent_class_name}){weaker_suffix}"
            ),
            Self::StaticMethodOverride {
                class_name,
                method_name,
                parent_class_name,
                parent_is_static,
            } => {
                if *parent_is_static {
                    format!(
                        "Cannot make static method {parent_class_name}::{method_name}() non static in class {class_name}"
                    )
                } else {
                    format!(
                        "Cannot make non static method {parent_class_name}::{method_name}() static in class {class_name}"
                    )
                }
            }
            Self::MethodSignatureOverride {
                actual_signature,
                expected_signature,
                ..
            } => format!(
                "Declaration of {actual_signature} must be compatible with {expected_signature}"
            ),
            Self::InterfaceMethodVisibility {
                class_name,
                method_name,
            } => format!(
                "Access type for interface method {class_name}::{method_name}() must be public"
            ),
            Self::InterfaceMethodBody {
                class_name,
                method_name,
            } => format!("Interface function {class_name}::{method_name}() cannot contain body"),
            Self::InterfaceMethodMissing {
                class_name,
                interface_name,
                method_name,
            } => format!("class {class_name} must implement {interface_name}::{method_name}"),
            Self::InterfaceMethodSignature {
                actual_signature,
                expected_signature,
                ..
            } => format!(
                "Declaration of {actual_signature} must be compatible with {expected_signature}"
            ),
            Self::InterfaceConstantVisibility {
                class_name,
                constant_name,
            } => format!(
                "Access type for interface constant {class_name}::{constant_name} must be public"
            ),
            Self::InterfaceProperty { .. } => {
                "Interfaces may only include hooked properties".to_owned()
            }
            Self::FinalClassExtend {
                class_name,
                parent_class_name,
            } => format!("Class {class_name} cannot extend final class {parent_class_name}"),
            Self::FinalMethodOverride {
                parent_class_name,
                method_name,
                ..
            } => format!("Cannot override final method {parent_class_name}::{method_name}()"),
            Self::PropertyStaticOverride {
                class_name,
                property_name,
                parent_class_name,
                parent_is_static,
            } => {
                if *parent_is_static {
                    format!(
                        "Cannot redeclare static {parent_class_name}::${property_name} as non static {class_name}::${property_name}"
                    )
                } else {
                    format!(
                        "Cannot redeclare non static {parent_class_name}::${property_name} as static {class_name}::${property_name}"
                    )
                }
            }
            Self::PropertyVisibilityOverride {
                class_name,
                property_name,
                required_visibility,
                parent_class_name,
                weaker_suffix,
            } => format!(
                "Access level to {class_name}::${property_name} must be {required_visibility} (as in class {parent_class_name}){weaker_suffix}"
            ),
            Self::ClassConstantVisibilityOverride {
                class_name,
                constant_name,
                required_visibility,
                parent_class_name,
                weaker_suffix,
            } => format!(
                "Access level to {class_name}::{constant_name} must be {required_visibility} (as in class {parent_class_name}){weaker_suffix}"
            ),
            Self::ClassExtendsInterface {
                class_name,
                interface_name,
            } => format!("Class {class_name} cannot extend interface {interface_name}"),
            Self::ImplementsNonInterface { message, .. } => message.clone(),
            Self::TraversableDirectImplementation { class_name } => format!(
                "Class {class_name} must implement interface Traversable as part of either Iterator or IteratorAggregate"
            ),
        }
    }

    /// Stable status message including the diagnostic ID prefix.
    #[must_use]
    pub fn status_message(&self) -> String {
        format!("{}: {}", self.id(), self.php_message())
    }

    /// PHP-compatible fatal message for CLI/executor diagnostic lines.
    #[must_use]
    pub fn php_fatal_message(&self) -> String {
        match self {
            Self::InterfaceMethodMissing {
                class_name,
                interface_name,
                method_name,
            } => format!(
                "Class {class_name} contains 1 abstract method and must therefore be declared abstract or implement the remaining method ({interface_name}::{method_name})"
            ),
            _ => self.php_message(),
        }
    }

    /// Structured payload fields for shared diagnostic envelopes.
    #[must_use]
    pub fn envelope_context(&self) -> BTreeMap<String, String> {
        let mut context = BTreeMap::new();
        context.insert("payload".to_string(), "vm_compile".to_string());
        match self {
            Self::MethodVisibilityOverride {
                class_name,
                method_name,
                required_visibility,
                parent_class_name,
                weaker_suffix,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("method".to_string(), method_name.clone());
                context.insert(
                    "required_visibility".to_string(),
                    required_visibility.clone(),
                );
                context.insert("parent_class".to_string(), parent_class_name.clone());
                context.insert("weaker_suffix".to_string(), weaker_suffix.clone());
            }
            Self::StaticMethodOverride {
                class_name,
                method_name,
                parent_class_name,
                parent_is_static,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("method".to_string(), method_name.clone());
                context.insert("parent_class".to_string(), parent_class_name.clone());
                context.insert("parent_is_static".to_string(), parent_is_static.to_string());
            }
            Self::MethodSignatureOverride {
                class_name,
                method_name,
                actual_signature,
                expected_signature,
            }
            | Self::InterfaceMethodSignature {
                class_name,
                method_name,
                actual_signature,
                expected_signature,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("method".to_string(), method_name.clone());
                context.insert("actual_signature".to_string(), actual_signature.clone());
                context.insert("expected_signature".to_string(), expected_signature.clone());
            }
            Self::InterfaceMethodVisibility {
                class_name,
                method_name,
            }
            | Self::InterfaceMethodBody {
                class_name,
                method_name,
            }
            | Self::FinalMethodOverride {
                class_name,
                method_name,
                ..
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("method".to_string(), method_name.clone());
            }
            Self::InterfaceMethodMissing {
                class_name,
                interface_name,
                method_name,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("interface".to_string(), interface_name.clone());
                context.insert("method".to_string(), method_name.clone());
            }
            Self::InterfaceConstantVisibility {
                class_name,
                constant_name,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("constant".to_string(), constant_name.clone());
            }
            Self::InterfaceProperty {
                class_name,
                property_name,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("property".to_string(), property_name.clone());
            }
            Self::FinalClassExtend {
                class_name,
                parent_class_name,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("parent_class".to_string(), parent_class_name.clone());
            }
            Self::PropertyStaticOverride {
                class_name,
                property_name,
                parent_class_name,
                parent_is_static,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("property".to_string(), property_name.clone());
                context.insert("parent_class".to_string(), parent_class_name.clone());
                context.insert("parent_is_static".to_string(), parent_is_static.to_string());
            }
            Self::PropertyVisibilityOverride {
                class_name,
                property_name,
                required_visibility,
                parent_class_name,
                weaker_suffix,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("property".to_string(), property_name.clone());
                context.insert(
                    "required_visibility".to_string(),
                    required_visibility.clone(),
                );
                context.insert("parent_class".to_string(), parent_class_name.clone());
                context.insert("weaker_suffix".to_string(), weaker_suffix.clone());
            }
            Self::ClassConstantVisibilityOverride {
                class_name,
                constant_name,
                required_visibility,
                parent_class_name,
                weaker_suffix,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("constant".to_string(), constant_name.clone());
                context.insert(
                    "required_visibility".to_string(),
                    required_visibility.clone(),
                );
                context.insert("parent_class".to_string(), parent_class_name.clone());
                context.insert("weaker_suffix".to_string(), weaker_suffix.clone());
            }
            Self::ClassExtendsInterface {
                class_name,
                interface_name,
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("interface".to_string(), interface_name.clone());
            }
            Self::ImplementsNonInterface {
                class_name,
                target_name,
                ..
            } => {
                context.insert("class".to_string(), class_name.clone());
                context.insert("target".to_string(), target_name.clone());
            }
            Self::TraversableDirectImplementation { class_name } => {
                context.insert("class".to_string(), class_name.clone());
            }
        }
        context
    }
}

/// Additional structured payload attached to selected runtime diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeDiagnosticPayload {
    /// VM compile diagnostic payload.
    VmCompile(VmCompileDiagnostic),
    /// WordPress bring-up diagnostic classification payload.
    WordPressBringup(WordPressDiagnosticContext),
}

impl RuntimeDiagnosticPayload {
    /// Structured payload fields for shared diagnostic envelopes and compact JSON.
    #[must_use]
    pub fn envelope_context(&self) -> BTreeMap<String, String> {
        match self {
            Self::VmCompile(payload) => payload.envelope_context(),
            Self::WordPressBringup(payload) => payload.envelope_context(),
        }
    }
}

/// Additive diagnostic metadata for WordPress bring-up closure reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WordPressDiagnosticContext {
    fields: BTreeMap<String, String>,
}

impl WordPressDiagnosticContext {
    /// Creates a context with the required stable error class.
    #[must_use]
    pub fn new(error_class: impl Into<String>) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert("wordpress_error_class".to_string(), error_class.into());
        Self { fields }
    }

    /// Adds a field when the value is available.
    #[must_use]
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Adds a field when the value is available.
    #[must_use]
    pub fn with_optional_field(
        self,
        key: impl Into<String>,
        value: Option<impl Into<String>>,
    ) -> Self {
        match value {
            Some(value) => self.with_field(key, value),
            None => self,
        }
    }

    /// Returns the deterministic context fields.
    #[must_use]
    pub const fn fields(&self) -> &BTreeMap<String, String> {
        &self.fields
    }

    /// Structured payload fields for shared diagnostic envelopes.
    #[must_use]
    pub fn envelope_context(&self) -> BTreeMap<String, String> {
        let mut context = BTreeMap::new();
        context.insert("payload".to_string(), "wordpress_bringup".to_string());
        context.extend(self.fields.clone());
        context
    }
}

/// One deterministic runtime stack frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeStackFrame {
    function: String,
}

impl RuntimeStackFrame {
    /// Creates a stack frame entry.
    #[must_use]
    pub fn new(function: impl Into<String>) -> Self {
        Self {
            function: function.into(),
        }
    }

    /// Function name.
    #[must_use]
    pub fn function(&self) -> &str {
        &self.function
    }
}

/// Structured runtime diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeDiagnostic {
    id: String,
    severity: RuntimeSeverity,
    message: String,
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
    php_reference: Option<PhpReferenceClassification>,
    payload: Option<RuntimeDiagnosticPayload>,
}

impl RuntimeDiagnostic {
    /// Creates a diagnostic.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        severity: RuntimeSeverity,
        message: impl Into<String>,
        source_span: RuntimeSourceSpan,
        stack_trace: Vec<RuntimeStackFrame>,
        php_reference: Option<PhpReferenceClassification>,
    ) -> Self {
        Self {
            id: id.into(),
            severity,
            message: message.into(),
            source_span,
            stack_trace,
            php_reference,
            payload: None,
        }
    }

    /// Creates a diagnostic with an additional typed payload.
    #[must_use]
    pub fn with_payload(
        id: impl Into<String>,
        severity: RuntimeSeverity,
        message: impl Into<String>,
        source_span: RuntimeSourceSpan,
        stack_trace: Vec<RuntimeStackFrame>,
        php_reference: Option<PhpReferenceClassification>,
        payload: RuntimeDiagnosticPayload,
    ) -> Self {
        Self {
            id: id.into(),
            severity,
            message: message.into(),
            source_span,
            stack_trace,
            php_reference,
            payload: Some(payload),
        }
    }

    /// Diagnostic ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Severity.
    #[must_use]
    pub const fn severity(&self) -> RuntimeSeverity {
        self.severity
    }

    /// Message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Source span.
    #[must_use]
    pub const fn source_span(&self) -> &RuntimeSourceSpan {
        &self.source_span
    }

    /// Stack trace frames.
    #[must_use]
    pub fn stack_trace(&self) -> &[RuntimeStackFrame] {
        &self.stack_trace
    }

    /// PHP-reference classification.
    #[must_use]
    pub const fn php_reference(&self) -> Option<PhpReferenceClassification> {
        self.php_reference
    }

    /// Explicit or ID-inferred PHP-reference classification.
    #[must_use]
    pub fn php_reference_or_inferred(&self) -> Option<PhpReferenceClassification> {
        self.php_reference
            .or_else(|| PhpReferenceClassification::from_diagnostic_id(self.id()))
    }

    /// PHP-visible runtime event family.
    #[must_use]
    pub fn event_kind(&self) -> RuntimeEventKind {
        match self.severity {
            RuntimeSeverity::Warning => RuntimeEventKind::Warning,
            RuntimeSeverity::Notice => RuntimeEventKind::Notice,
            RuntimeSeverity::Deprecation => RuntimeEventKind::Deprecation,
            RuntimeSeverity::RecoverableError => RuntimeEventKind::CatchableException,
            RuntimeSeverity::FatalError => self
                .php_reference_or_inferred()
                .and_then(PhpReferenceClassification::throwable_class)
                .map_or(RuntimeEventKind::FatalError, |_| {
                    RuntimeEventKind::CatchableException
                }),
            RuntimeSeverity::UnsupportedFeature => RuntimeEventKind::UnsupportedFeature,
        }
    }

    /// Additional typed diagnostic payload.
    #[must_use]
    pub const fn payload(&self) -> Option<&RuntimeDiagnosticPayload> {
        self.payload.as_ref()
    }

    /// Returns a copy of this diagnostic with additional typed payload.
    #[must_use]
    pub fn with_diagnostic_payload(mut self, payload: RuntimeDiagnosticPayload) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Stable compact JSON representation.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"id\":\"");
        out.push_str(&escape_json(&self.id));
        out.push_str("\",\"severity\":\"");
        out.push_str(self.severity.as_str());
        out.push_str("\",\"message\":\"");
        out.push_str(&escape_json(&self.message));
        out.push_str("\",\"span\":{");
        out.push_str("\"file\":");
        match &self.source_span.file {
            Some(file) => {
                out.push('"');
                out.push_str(&escape_json(file));
                out.push('"');
            }
            None => out.push_str("null"),
        }
        out.push_str(",\"start\":");
        out.push_str(&self.source_span.start.to_string());
        out.push_str(",\"end\":");
        out.push_str(&self.source_span.end.to_string());
        out.push_str("},\"stack\":[");
        for (index, frame) in self.stack_trace.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"function\":\"");
            out.push_str(&escape_json(frame.function()));
            out.push_str("\"}");
        }
        out.push_str("],\"php_reference\":");
        match self.php_reference {
            Some(classification) => {
                out.push('"');
                out.push_str(classification.as_str());
                out.push('"');
            }
            None => out.push_str("null"),
        }
        if let Some(payload) = &self.payload {
            out.push_str(",\"context\":{");
            for (index, (key, value)) in payload.envelope_context().iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push('"');
                out.push_str(&escape_json(key));
                out.push_str("\":\"");
                out.push_str(&escape_json(value));
                out.push('"');
            }
            out.push('}');
        }
        out.push('}');
        out
    }

    /// Shared diagnostic envelope for debug and JSON diagnostic renderers.
    #[must_use]
    pub fn to_diagnostic_envelope(&self) -> DiagnosticEnvelope {
        let mut context = BTreeMap::new();
        if let Some(classification) = self.php_reference {
            context.insert(
                "php_reference".to_string(),
                classification.as_str().to_string(),
            );
        }
        context.insert(
            "stack_depth".to_string(),
            self.stack_trace.len().to_string(),
        );
        if !self.stack_trace.is_empty() {
            context.insert(
                "stack".to_string(),
                self.stack_trace
                    .iter()
                    .map(RuntimeStackFrame::function)
                    .collect::<Vec<_>>()
                    .join(" > "),
            );
        }
        if let Some(payload) = &self.payload {
            context.extend(payload.envelope_context());
        }

        let mut envelope = DiagnosticEnvelope::new(
            self.id.clone(),
            DiagnosticLayer::runtime(),
            DiagnosticPhase::new("execute"),
            self.severity.envelope_severity(),
            self.message.clone(),
        )
        .with_location(DiagnosticLocation::new(
            self.source_span.file.as_deref(),
            None::<&str>,
            Some(DiagnosticSpan::new(
                self.source_span.start as usize,
                self.source_span.end as usize,
            )),
        ))
        .with_context(context);
        envelope.php_visible = true;
        envelope
    }
}

/// Runtime error wrapper kept separate from VM control flow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeError {
    diagnostic: RuntimeDiagnostic,
}

impl RuntimeError {
    /// Creates a runtime error.
    #[must_use]
    pub const fn new(diagnostic: RuntimeDiagnostic) -> Self {
        Self { diagnostic }
    }

    /// Returns the diagnostic.
    #[must_use]
    pub const fn diagnostic(&self) -> &RuntimeDiagnostic {
        &self.diagnostic
    }
}

/// Undefined variable warning helper.
#[must_use]
pub fn undefined_variable_warning(
    name: impl Into<String>,
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    let name = name.into();
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNDEFINED_VARIABLE_WARNING",
        RuntimeSeverity::Warning,
        format!("Undefined variable ${name}"),
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::Warning),
    )
}

/// TypeError MVP helper.
#[must_use]
pub fn type_error_mvp(
    message: impl Into<String>,
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_TYPE_ERROR",
        RuntimeSeverity::FatalError,
        message,
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::TypeError),
    )
}

/// ValueError MVP helper.
#[must_use]
pub fn value_error_mvp(
    message: impl Into<String>,
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        RuntimeSeverity::FatalError,
        message,
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::ValueError),
    )
}

/// ArgumentCountError MVP helper.
#[must_use]
pub fn argument_count_error_mvp(
    message: impl Into<String>,
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_BUILTIN_ARITY",
        RuntimeSeverity::FatalError,
        message,
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::ArgumentCountError),
    )
}

/// DivisionByZero MVP helper.
#[must_use]
pub fn division_by_zero_mvp(
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_DIVISION_BY_ZERO",
        RuntimeSeverity::FatalError,
        "division by zero",
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::DivisionByZeroError),
    )
}

/// UnhandledMatchError MVP helper.
#[must_use]
pub fn unhandled_match_error_mvp(
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_VM_UNHANDLED_MATCH",
        RuntimeSeverity::FatalError,
        "match expression did not match any arm",
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::UnhandledMatchError),
    )
}

/// Array-to-string warning helper.
#[must_use]
pub fn array_to_string_warning(
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_ARRAY_TO_STRING_WARNING",
        RuntimeSeverity::Warning,
        "Array to string conversion",
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::Warning),
    )
}

/// Leading numeric-string arithmetic warning helper.
#[must_use]
pub fn leading_numeric_string_warning(
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_NON_NUMERIC_STRING_WARNING",
        RuntimeSeverity::Warning,
        "A non-numeric value encountered",
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::Warning),
    )
}

/// Non-numeric arithmetic TypeError helper.
#[must_use]
pub fn non_numeric_string_type_error(
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_NON_NUMERIC_STRING",
        RuntimeSeverity::FatalError,
        "non-numeric string cannot be used as a number",
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::TypeError),
    )
}

/// Undefined function helper.
#[must_use]
pub fn undefined_function(
    name: impl AsRef<str>,
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    let name = name.as_ref();
    let payload = RuntimeDiagnosticPayload::WordPressBringup(
        WordPressDiagnosticContext::new("stdlib_builtin")
            .with_field("requested_name", name)
            .with_field("normalized_name", name.to_ascii_lowercase())
            .with_field("lookup_kind", "function"),
    );
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNDEFINED_FUNCTION",
        RuntimeSeverity::FatalError,
        format!("undefined function {name}"),
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::Error),
    )
    .with_diagnostic_payload(payload)
}

/// Unsupported feature helper.
#[must_use]
pub fn unsupported_feature(
    id: impl Into<String>,
    message: impl Into<String>,
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        id,
        RuntimeSeverity::UnsupportedFeature,
        message,
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::Unsupported),
    )
}

fn escape_json(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        PhpReferenceClassification, RuntimeDiagnostic, RuntimeDiagnosticPayload, RuntimeEventKind,
        RuntimeSeverity, RuntimeStackFrame, VmCompileDiagnostic, argument_count_error_mvp,
        array_to_string_warning, division_by_zero_mvp, leading_numeric_string_warning,
        undefined_function, undefined_variable_warning, unhandled_match_error_mvp, value_error_mvp,
    };
    use crate::RuntimeSourceSpan;

    #[test]
    fn errors_runtime_diagnostics_are_stable_json() {
        let diagnostic = undefined_function(
            "missing",
            RuntimeSourceSpan {
                file: Some("fixture.php".to_owned()),
                start: 1,
                end: 8,
            },
            vec![RuntimeStackFrame::new("main")],
        );

        assert_eq!(diagnostic.id(), "E_PHP_RUNTIME_UNDEFINED_FUNCTION");
        assert_eq!(diagnostic.severity(), RuntimeSeverity::FatalError);
        assert_eq!(
            diagnostic.to_json(),
            "{\"id\":\"E_PHP_RUNTIME_UNDEFINED_FUNCTION\",\"severity\":\"fatal_error\",\"message\":\"undefined function missing\",\"span\":{\"file\":\"fixture.php\",\"start\":1,\"end\":8},\"stack\":[{\"function\":\"main\"}],\"php_reference\":\"error\",\"context\":{\"lookup_kind\":\"function\",\"normalized_name\":\"missing\",\"payload\":\"wordpress_bringup\",\"requested_name\":\"missing\",\"wordpress_error_class\":\"stdlib_builtin\"}}"
        );
    }

    #[test]
    fn undefined_function_runtime_diagnostic_has_shared_envelope() {
        let diagnostic = undefined_function(
            "missing",
            RuntimeSourceSpan {
                file: Some("fixture.php".to_owned()),
                start: 1,
                end: 8,
            },
            vec![
                RuntimeStackFrame::new("main"),
                RuntimeStackFrame::new("caller"),
            ],
        );

        let envelope = diagnostic.to_diagnostic_envelope();
        let json: serde_json::Value =
            serde_json::from_str(&envelope.compact_json().expect("json")).expect("parse json");

        assert_eq!(json["code"], "E_PHP_RUNTIME_UNDEFINED_FUNCTION");
        assert_eq!(json["layer"], "runtime");
        assert_eq!(json["phase"], "execute");
        assert_eq!(json["severity"], "fatal_error");
        assert_eq!(json["location"]["path"], "fixture.php");
        assert_eq!(json["context"]["php_reference"], "error");
        assert_eq!(json["context"]["wordpress_error_class"], "stdlib_builtin");
        assert_eq!(json["context"]["requested_name"], "missing");
        assert_eq!(json["context"]["normalized_name"], "missing");
        assert_eq!(json["context"]["lookup_kind"], "function");
        assert_eq!(json["context"]["stack"], "main > caller");
        assert_eq!(json["context"]["stack_depth"], "2");
        assert_eq!(json["php_visible"], true);
    }

    #[test]
    fn errors_helpers_cover_warning_and_division_by_zero() {
        let warning =
            undefined_variable_warning("missing", RuntimeSourceSpan::default(), Vec::new());
        assert_eq!(warning.id(), "E_PHP_RUNTIME_UNDEFINED_VARIABLE_WARNING");
        assert_eq!(warning.severity(), RuntimeSeverity::Warning);

        let division = division_by_zero_mvp(RuntimeSourceSpan::default(), Vec::new());
        assert_eq!(division.id(), "E_PHP_RUNTIME_DIVISION_BY_ZERO");
        assert_eq!(division.message(), "division by zero");
    }

    #[test]
    fn errors_helpers_cover_runtime_event_and_throwable_mapping() {
        let span = RuntimeSourceSpan::default();
        let value = value_error_mvp("bad value", span.clone(), Vec::new());
        assert_eq!(
            value.php_reference_or_inferred(),
            Some(PhpReferenceClassification::ValueError)
        );
        assert_eq!(value.event_kind(), RuntimeEventKind::CatchableException);

        let arity = argument_count_error_mvp("too few arguments", span.clone(), Vec::new());
        assert_eq!(
            arity
                .php_reference_or_inferred()
                .and_then(PhpReferenceClassification::throwable_class),
            Some("ArgumentCountError")
        );

        let unhandled_match = unhandled_match_error_mvp(span.clone(), Vec::new());
        assert_eq!(
            unhandled_match
                .php_reference_or_inferred()
                .and_then(PhpReferenceClassification::throwable_class),
            Some("UnhandledMatchError")
        );

        let array_warning = array_to_string_warning(span.clone(), Vec::new());
        assert_eq!(array_warning.event_kind(), RuntimeEventKind::Warning);

        let numeric_warning = leading_numeric_string_warning(span, Vec::new());
        assert_eq!(numeric_warning.message(), "A non-numeric value encountered");
    }

    #[test]
    fn errors_infer_php_reference_from_legacy_diagnostic_ids() {
        assert_eq!(
            PhpReferenceClassification::from_diagnostic_id("E_PHP_RUNTIME_BUILTIN_ARITY"),
            Some(PhpReferenceClassification::ArgumentCountError)
        );
        assert_eq!(
            PhpReferenceClassification::from_diagnostic_id("E_PHP_RUNTIME_BUILTIN_TYPE"),
            Some(PhpReferenceClassification::TypeError)
        );
        assert_eq!(
            PhpReferenceClassification::from_diagnostic_id("E_PHP_RUNTIME_NON_NUMERIC_STRING"),
            Some(PhpReferenceClassification::TypeError)
        );
        assert_eq!(
            PhpReferenceClassification::from_diagnostic_id("E_PHP_RUNTIME_BUILTIN_VALUE"),
            Some(PhpReferenceClassification::ValueError)
        );
        assert_eq!(
            PhpReferenceClassification::from_diagnostic_id("E_PHP_VM_UNHANDLED_MATCH"),
            Some(PhpReferenceClassification::UnhandledMatchError)
        );
    }

    #[test]
    fn exception_diagnostics_preserve_severity_and_source_mapping() {
        let span = RuntimeSourceSpan {
            file: Some("fixtures/runtime_semantics/errors/type-error-uncaught.php".to_owned()),
            start: 12,
            end: 21,
        };
        let warning = undefined_variable_warning("missing", span.clone(), Vec::new());
        let fatal = RuntimeDiagnostic::new(
            "E_PHP_VM_UNCAUGHT_EXCEPTION",
            RuntimeSeverity::FatalError,
            "Uncaught TypeError: bad".to_owned(),
            span.clone(),
            vec![RuntimeStackFrame::new("main")],
            None,
        );

        assert_eq!(warning.severity(), RuntimeSeverity::Warning);
        assert_eq!(fatal.severity(), RuntimeSeverity::FatalError);
        let json = fatal.to_json();
        assert!(json.contains("type-error-uncaught.php"));
        assert!(json.contains("\"start\":12"));
        assert!(json.contains("\"end\":21"));
    }

    #[test]
    fn vm_compile_payload_formats_method_visibility_override() {
        let payload = VmCompileDiagnostic::MethodVisibilityOverride {
            class_name: "child".to_owned(),
            method_name: "show".to_owned(),
            required_visibility: "public".to_owned(),
            parent_class_name: "base".to_owned(),
            weaker_suffix: String::new(),
        };

        assert_eq!(payload.id(), "E_PHP_VM_METHOD_VISIBILITY_OVERRIDE");
        assert_eq!(
            payload.php_fatal_message(),
            "Access level to child::show() must be public (as in class base)"
        );
        assert_eq!(
            payload.status_message(),
            "E_PHP_VM_METHOD_VISIBILITY_OVERRIDE: Access level to child::show() must be public (as in class base)"
        );
    }

    #[test]
    fn vm_compile_payload_formats_interface_method_missing() {
        let payload = VmCompileDiagnostic::InterfaceMethodMissing {
            class_name: "Derived".to_owned(),
            interface_name: "Contract".to_owned(),
            method_name: "run".to_owned(),
        };

        assert_eq!(payload.id(), "E_PHP_VM_INTERFACE_METHOD_MISSING");
        assert_eq!(
            payload.php_message(),
            "class Derived must implement Contract::run"
        );
        assert_eq!(
            payload.php_fatal_message(),
            "Class Derived contains 1 abstract method and must therefore be declared abstract or implement the remaining method (Contract::run)"
        );
    }

    #[test]
    fn runtime_diagnostic_carries_vm_compile_payload() {
        let payload = VmCompileDiagnostic::FinalClassExtend {
            class_name: "Child".to_owned(),
            parent_class_name: "Base".to_owned(),
        };
        let diagnostic = RuntimeDiagnostic::with_payload(
            payload.id(),
            RuntimeSeverity::FatalError,
            payload.status_message(),
            RuntimeSourceSpan::default(),
            Vec::new(),
            None,
            RuntimeDiagnosticPayload::VmCompile(payload.clone()),
        );

        assert_eq!(diagnostic.id(), "E_PHP_VM_FINAL_CLASS_EXTEND");
        assert_eq!(
            diagnostic.payload(),
            Some(&RuntimeDiagnosticPayload::VmCompile(payload))
        );
        let envelope = diagnostic.to_diagnostic_envelope();
        assert_eq!(envelope.context["payload"], "vm_compile");
        assert_eq!(envelope.context["class"], "Child");
        assert_eq!(envelope.context["parent_class"], "Base");
    }
}
