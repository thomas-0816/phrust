//! Runtime diagnostics shared by the VM and CLI.

use crate::builtins::RuntimeSourceSpan;

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
}

/// Optional PHP-reference classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpReferenceClassification {
    /// PHP warning.
    Warning,
    /// PHP `TypeError`.
    TypeError,
    /// PHP `DivisionByZeroError`.
    DivisionByZeroError,
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
            Self::TypeError => "type_error",
            Self::DivisionByZeroError => "division_by_zero_error",
            Self::Error => "error",
            Self::FatalError => "fatal_error",
            Self::Unsupported => "unsupported",
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
}

/// Additional structured payload attached to selected runtime diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeDiagnosticPayload {
    /// VM compile diagnostic payload.
    VmCompile(VmCompileDiagnostic),
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

    /// Additional typed diagnostic payload.
    #[must_use]
    pub const fn payload(&self) -> Option<&RuntimeDiagnosticPayload> {
        self.payload.as_ref()
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
        out.push('}');
        out
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

/// Undefined function helper.
#[must_use]
pub fn undefined_function(
    name: impl AsRef<str>,
    source_span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNDEFINED_FUNCTION",
        RuntimeSeverity::FatalError,
        format!("undefined function {}", name.as_ref()),
        source_span,
        stack_trace,
        Some(PhpReferenceClassification::Error),
    )
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
        RuntimeDiagnostic, RuntimeDiagnosticPayload, RuntimeSeverity, RuntimeStackFrame,
        VmCompileDiagnostic, division_by_zero_mvp, undefined_function, undefined_variable_warning,
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
            "{\"id\":\"E_PHP_RUNTIME_UNDEFINED_FUNCTION\",\"severity\":\"fatal_error\",\"message\":\"undefined function missing\",\"span\":{\"file\":\"fixture.php\",\"start\":1,\"end\":8},\"stack\":[{\"function\":\"main\"}],\"php_reference\":\"error\"}"
        );
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
    }
}
