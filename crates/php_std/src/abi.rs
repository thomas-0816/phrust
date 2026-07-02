//! Safe Rust ABI used by the VM to call standard-library standard-library builtins.

use php_runtime::api::{
    OutputBuffer, ProcessCapability, ReferenceCell, RuntimeContext, RuntimeDiagnostic,
    RuntimeRequestMode, RuntimeSeverity, RuntimeSourceSpan, Value,
};

/// Result returned by a standard-library builtin.
pub type BuiltinResult = Result<ReturnValue, BuiltinRuntimeError>;

/// Function pointer form for registered builtins.
pub type BuiltinHandler = fn(&mut CallContext<'_>) -> BuiltinResult;

/// Trait form for test and adapter builtins.
pub trait BuiltinFunction {
    /// Stable normalized PHP function name.
    fn name(&self) -> &'static str;

    /// Invokes the builtin through the standard-library ABI.
    fn call(&self, context: &mut CallContext<'_>) -> BuiltinResult;
}

/// Registered ABI entry.
#[derive(Clone, Copy)]
pub struct RegisteredBuiltin {
    name: &'static str,
    handler: BuiltinHandler,
    metadata: BuiltinMetadata,
}

impl RegisteredBuiltin {
    /// Creates a registered builtin entry.
    #[must_use]
    pub const fn new(
        name: &'static str,
        handler: BuiltinHandler,
        metadata: BuiltinMetadata,
    ) -> Self {
        Self {
            name,
            handler,
            metadata,
        }
    }

    /// Stable normalized PHP function name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// ABI metadata.
    #[must_use]
    pub const fn metadata(&self) -> BuiltinMetadata {
        self.metadata
    }
}

impl BuiltinFunction for RegisteredBuiltin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn call(&self, context: &mut CallContext<'_>) -> BuiltinResult {
        (self.handler)(context)
    }
}

/// Metadata needed before full arginfo lowering exists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuiltinMetadata {
    /// Whether the function accepts a variadic tail.
    pub variadic: bool,
    /// By-reference parameter names and positions.
    pub by_ref_params: &'static [ByRefParam],
}

/// By-reference parameter metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByRefParam {
    /// Stable parameter name without `$`.
    pub name: &'static str,
    /// Zero-based positional index.
    pub index: usize,
}

/// One call argument.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallArgument {
    name: Option<String>,
    value: Value,
    reference: Option<ReferenceCell>,
}

impl CallArgument {
    /// Creates a by-value argument.
    #[must_use]
    pub fn by_value(value: Value) -> Self {
        Self {
            name: None,
            value,
            reference: None,
        }
    }

    /// Creates a named by-value argument.
    #[must_use]
    pub fn named(name: impl Into<String>, value: Value) -> Self {
        Self {
            name: Some(name.into()),
            value,
            reference: None,
        }
    }

    /// Creates a by-reference argument.
    #[must_use]
    pub fn by_reference(value: Value, reference: ReferenceCell) -> Self {
        Self {
            name: None,
            value,
            reference: Some(reference),
        }
    }

    /// Optional named-argument name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Argument value snapshot.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Optional by-reference cell hook.
    #[must_use]
    pub fn reference(&self) -> Option<ReferenceCell> {
        self.reference.clone()
    }
}

/// Request context visible to builtins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestContext {
    sapi: &'static str,
    cwd: String,
    file: Option<String>,
    capabilities: CapabilitySet,
}

impl RequestContext {
    /// Creates a CLI request context.
    #[must_use]
    pub fn cli(cwd: impl Into<String>, file: Option<String>) -> Self {
        Self {
            sapi: "cli",
            cwd: cwd.into(),
            file,
            capabilities: CapabilitySet::default(),
        }
    }

    /// Creates a request context from the VM runtime request state.
    #[must_use]
    pub fn from_runtime(runtime: &RuntimeContext, file: Option<String>) -> Self {
        Self {
            sapi: match &runtime.request_mode {
                RuntimeRequestMode::Cli => "cli",
                RuntimeRequestMode::Http(_) => "http",
            },
            cwd: runtime.cwd.to_string_lossy().into_owned(),
            file,
            capabilities: CapabilitySet::from_runtime(runtime),
        }
    }

    /// SAPI name exposed to builtins.
    #[must_use]
    pub const fn sapi(&self) -> &'static str {
        self.sapi
    }

    /// Deterministic current working directory.
    #[must_use]
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Current source file, when available.
    #[must_use]
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    /// Capability view.
    #[must_use]
    pub const fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }
}

/// Default-off host capabilities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapabilitySet {
    /// Process/shell execution capability.
    pub process: bool,
    /// Network stream capability.
    pub network: bool,
    /// Host filesystem writes outside test tempdirs.
    pub host_writes: bool,
}

impl CapabilitySet {
    /// Creates ABI capability metadata from the deterministic runtime context.
    #[must_use]
    pub fn from_runtime(runtime: &RuntimeContext) -> Self {
        Self {
            process: !matches!(runtime.process, ProcessCapability::Disabled),
            network: false,
            host_writes: runtime.filesystem.first_allowed_root().is_some(),
        }
    }
}

/// Mutable call context provided by the VM.
pub struct CallContext<'a> {
    function_name: &'static str,
    args: Vec<CallArgument>,
    source_span: RuntimeSourceSpan,
    output: &'a mut OutputBuffer,
    request: &'a RequestContext,
    diagnostics: Vec<RuntimeDiagnostic>,
}

impl<'a> CallContext<'a> {
    /// Creates a call context.
    pub fn new(
        function_name: &'static str,
        args: Vec<CallArgument>,
        source_span: RuntimeSourceSpan,
        output: &'a mut OutputBuffer,
        request: &'a RequestContext,
    ) -> Self {
        Self {
            function_name,
            args,
            source_span,
            output,
            request,
            diagnostics: Vec::new(),
        }
    }

    /// Stable function name.
    #[must_use]
    pub const fn function_name(&self) -> &'static str {
        self.function_name
    }

    /// Arguments in VM call order.
    #[must_use]
    pub fn args(&self) -> &[CallArgument] {
        &self.args
    }

    /// Source span for diagnostics.
    #[must_use]
    pub const fn source_span(&self) -> &RuntimeSourceSpan {
        &self.source_span
    }

    /// Output buffer for echo-like builtins.
    pub fn output(&mut self) -> &mut OutputBuffer {
        self.output
    }

    /// Request context.
    #[must_use]
    pub const fn request(&self) -> &RequestContext {
        self.request
    }

    /// Non-fatal diagnostics emitted by the builtin.
    #[must_use]
    pub fn diagnostics(&self) -> &[RuntimeDiagnostic] {
        &self.diagnostics
    }

    /// Drains non-fatal diagnostics for VM result conversion.
    #[must_use]
    pub fn take_diagnostics(&mut self) -> Vec<RuntimeDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Emits a warning diagnostic using the call source span.
    pub fn warning(&mut self, id: &'static str, message: impl Into<String>) {
        self.diagnostics.push(RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Warning,
            message.into(),
            self.source_span.clone(),
            Vec::new(),
            None,
        ));
    }

    /// Creates a fatal diagnostic using the call source span.
    #[must_use]
    pub fn fatal(&self, id: &'static str, message: impl Into<String>) -> BuiltinRuntimeError {
        BuiltinRuntimeError::new(RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::FatalError,
            message.into(),
            self.source_span.clone(),
            Vec::new(),
            None,
        ))
    }
}

/// Builtin return value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReturnValue {
    /// PHP value return.
    Value(Value),
    /// No explicit value; maps to PHP null in the VM bridge.
    Void,
}

/// Runtime error produced by a builtin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltinRuntimeError {
    diagnostic: Box<RuntimeDiagnostic>,
}

impl BuiltinRuntimeError {
    /// Creates an error from a runtime diagnostic.
    #[must_use]
    pub fn new(diagnostic: RuntimeDiagnostic) -> Self {
        Self {
            diagnostic: Box::new(diagnostic),
        }
    }

    /// Diagnostic to surface through the VM.
    #[must_use]
    pub fn diagnostic(&self) -> &RuntimeDiagnostic {
        &self.diagnostic
    }
}

/// Calls a registered builtin.
pub fn call_builtin(
    builtin: &impl BuiltinFunction,
    context: &mut CallContext<'_>,
) -> BuiltinResult {
    builtin.call(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::api::PhpString;

    fn test_builtin_echo_like(context: &mut CallContext<'_>) -> BuiltinResult {
        for arg in context.args().to_vec() {
            if let Value::String(text) = arg.value() {
                context.output().write_php_string(text);
            } else {
                return Err(context.fatal("E_PHP_STD_TEST_EXPECTED_STRING", "expected string"));
            }
        }
        Ok(ReturnValue::Value(
            Value::Int(context.output().len() as i64),
        ))
    }

    #[test]
    fn builtin_call_context_carries_output_and_source_span() {
        let builtin = RegisteredBuiltin::new(
            "__php_std_test_echo_like",
            test_builtin_echo_like,
            BuiltinMetadata {
                variadic: true,
                by_ref_params: &[],
            },
        );
        let mut output = OutputBuffer::new();
        let request = RequestContext::cli("/tmp/phrust", Some("fixture.php".to_owned()));
        let span = RuntimeSourceSpan {
            file: Some("fixture.php".to_owned()),
            start: 7,
            end: 19,
        };
        let mut context = CallContext::new(
            builtin.name(),
            vec![CallArgument::by_value(Value::String(
                PhpString::from_test_str("abc"),
            ))],
            span.clone(),
            &mut output,
            &request,
        );

        let result = call_builtin(&builtin, &mut context).expect("builtin result");
        assert_eq!(result, ReturnValue::Value(Value::Int(3)));
        assert_eq!(context.output().as_bytes(), b"abc");
        assert_eq!(context.source_span(), &span);
        assert_eq!(context.request().sapi(), "cli");
    }

    #[test]
    fn by_ref_and_variadic_metadata_are_modelable() {
        const BY_REF: &[ByRefParam] = &[ByRefParam {
            name: "value",
            index: 0,
        }];
        let metadata = BuiltinMetadata {
            variadic: true,
            by_ref_params: BY_REF,
        };

        assert!(metadata.variadic);
        assert_eq!(metadata.by_ref_params[0].name, "value");
        assert_eq!(metadata.by_ref_params[0].index, 0);
    }
}
