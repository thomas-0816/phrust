//! Minimal VM bridge for the standard-library `php_std` builtin ABI.

use php_runtime::{OutputBuffer, RuntimeSourceSpan as VmRuntimeSourceSpan, Value};
use php_std::abi::{
    BuiltinFunction, CallArgument, CallContext, RequestContext, ReturnValue, call_builtin,
};

use crate::vm::VmResult;

/// Calls a `php_std` builtin through the standard-library ABI.
///
/// This is intentionally not wired into all VM builtin dispatch yet. It proves
/// the ABI boundary and `VmResult` conversion while existing runtime-semantics builtins
/// continue to use their current runtime path.
pub fn call_php_std_builtin(
    builtin: &impl BuiltinFunction,
    args: Vec<CallArgument>,
    source_span: VmRuntimeSourceSpan,
    output: &mut OutputBuffer,
    request: &RequestContext,
) -> VmResult {
    let mut context = CallContext::new(builtin.name(), args, source_span, output, request);

    match call_builtin(builtin, &mut context) {
        Ok(ReturnValue::Value(value)) => {
            let output = context.output().clone();
            let diagnostics = context.take_diagnostics();
            VmResult::success_with_diagnostics(output, Some(value), diagnostics)
        }
        Ok(ReturnValue::Void) => {
            let output = context.output().clone();
            let diagnostics = context.take_diagnostics();
            VmResult::success_with_diagnostics(output, Some(Value::Null), diagnostics)
        }
        Err(error) => {
            let output = context.output().clone();
            let mut diagnostics = context.take_diagnostics();
            let diagnostic = error.diagnostic().clone();
            let message = format!("{}: {}", diagnostic.id(), diagnostic.message());
            diagnostics.push(diagnostic);
            let mut result = VmResult::runtime_error_with_diagnostic(
                output,
                message,
                diagnostics
                    .pop()
                    .expect("fatal builtin diagnostic is appended above"),
            );
            result.diagnostics.splice(0..0, diagnostics);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::{PhpString, ReferenceCell, RuntimeContext};
    use php_std::abi::{BuiltinMetadata, BuiltinResult, RegisteredBuiltin};

    fn test_builtin_echo_like(context: &mut CallContext<'_>) -> BuiltinResult {
        let mut bytes = 0;
        for arg in context.args().to_vec() {
            if let Value::String(text) = arg.value() {
                bytes += text.as_bytes().len();
                context.output().write_php_string(text);
            } else {
                return Err(context.fatal("E_PHP_STD_TEST_EXPECTED_STRING", "expected string"));
            }
        }
        Ok(ReturnValue::Value(Value::Int(bytes as i64)))
    }

    fn test_builtin_fails_with_span(context: &mut CallContext<'_>) -> BuiltinResult {
        Err(context.fatal("E_PHP_STD_TEST_FAILURE", "test failure"))
    }

    fn test_builtin_request_scalar(context: &mut CallContext<'_>) -> BuiltinResult {
        Ok(ReturnValue::Value(Value::String(PhpString::from_test_str(
            context.request().cwd(),
        ))))
    }

    fn test_builtin_requires_one_int(context: &mut CallContext<'_>) -> BuiltinResult {
        match context.args() {
            [arg] => match arg.value() {
                Value::Int(value) => Ok(ReturnValue::Value(Value::Int(*value))),
                _ => Err(context.fatal("E_PHP_STD_TEST_TYPE", "expected int")),
            },
            _ => Err(context.fatal("E_PHP_STD_TEST_ARITY", "expected exactly one argument")),
        }
    }

    fn test_builtin_warns_and_continues(context: &mut CallContext<'_>) -> BuiltinResult {
        context.warning("E_PHP_STD_TEST_WARNING", "continuing after warning");
        Ok(ReturnValue::Value(Value::Bool(true)))
    }

    fn test_builtin_sets_reference(context: &mut CallContext<'_>) -> BuiltinResult {
        let Some(arg) = context.args().first() else {
            return Err(context.fatal("E_PHP_STD_TEST_ARITY", "expected reference"));
        };
        let Some(reference) = arg.reference() else {
            return Err(context.fatal("E_PHP_STD_TEST_BYREF", "expected reference"));
        };
        reference.set(Value::Int(42));
        Ok(ReturnValue::Void)
    }

    fn test_request(file: Option<&str>) -> RequestContext {
        RequestContext::from_runtime(
            &RuntimeContext::default().with_cwd("/tmp/phrust-std-abi"),
            file.map(str::to_owned),
        )
    }

    #[test]
    fn std_bridge_executes_test_builtin_with_output() {
        let builtin = RegisteredBuiltin::new(
            "__php_std_test_echo_like",
            test_builtin_echo_like,
            BuiltinMetadata {
                variadic: true,
                by_ref_params: &[],
            },
        );
        let result = call_php_std_builtin(
            &builtin,
            vec![CallArgument::by_value(Value::String(
                PhpString::from_test_str("hello"),
            ))],
            VmRuntimeSourceSpan {
                file: Some("fixture.php".to_owned()),
                start: 4,
                end: 12,
            },
            &mut OutputBuffer::new(),
            &test_request(Some("fixture.php")),
        );

        assert!(result.status.is_success(), "{:?}", result.status);
        assert_eq!(result.output.as_bytes(), b"hello");
        assert_eq!(result.return_value, Some(Value::Int(5)));
    }

    #[test]
    fn std_bridge_preserves_diagnostic_source_span() {
        let builtin = RegisteredBuiltin::new(
            "__php_std_test_fails",
            test_builtin_fails_with_span,
            BuiltinMetadata::default(),
        );
        let result = call_php_std_builtin(
            &builtin,
            Vec::new(),
            VmRuntimeSourceSpan {
                file: Some("fixture.php".to_owned()),
                start: 10,
                end: 20,
            },
            &mut OutputBuffer::new(),
            &test_request(Some("fixture.php")),
        );

        assert!(!result.status.is_success(), "{:?}", result.status);
        let span = result
            .diagnostics
            .first()
            .expect("bridge diagnostic")
            .source_span();
        assert_eq!(span.file.as_deref(), Some("fixture.php"));
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
    }

    #[test]
    fn std_bridge_uses_runtime_request_context_for_scalar_return() {
        let builtin = RegisteredBuiltin::new(
            "__php_std_test_request_scalar",
            test_builtin_request_scalar,
            BuiltinMetadata::default(),
        );
        let mut output = OutputBuffer::new();
        let request = test_request(Some("fixture.php"));
        let result = call_php_std_builtin(
            &builtin,
            Vec::new(),
            VmRuntimeSourceSpan {
                file: Some("fixture.php".to_owned()),
                start: 1,
                end: 2,
            },
            &mut output,
            &request,
        );

        assert!(result.status.is_success(), "{:?}", result.status);
        assert_eq!(
            result.return_value,
            Some(Value::String(PhpString::from_test_str(
                "/tmp/phrust-std-abi"
            )))
        );
    }

    #[test]
    fn std_bridge_preserves_arity_and_type_failures() {
        let builtin = RegisteredBuiltin::new(
            "__php_std_test_requires_one_int",
            test_builtin_requires_one_int,
            BuiltinMetadata::default(),
        );
        let span = VmRuntimeSourceSpan {
            file: Some("fixture.php".to_owned()),
            start: 30,
            end: 38,
        };

        let arity = call_php_std_builtin(
            &builtin,
            Vec::new(),
            span.clone(),
            &mut OutputBuffer::new(),
            &test_request(Some("fixture.php")),
        );
        assert!(!arity.status.is_success(), "{:?}", arity.status);
        assert_eq!(arity.diagnostics[0].id(), "E_PHP_STD_TEST_ARITY");
        assert_eq!(arity.diagnostics[0].source_span(), &span);

        let type_error = call_php_std_builtin(
            &builtin,
            vec![CallArgument::by_value(Value::String(
                PhpString::from_test_str("nope"),
            ))],
            span.clone(),
            &mut OutputBuffer::new(),
            &test_request(Some("fixture.php")),
        );
        assert!(!type_error.status.is_success(), "{:?}", type_error.status);
        assert_eq!(type_error.diagnostics[0].id(), "E_PHP_STD_TEST_TYPE");
        assert_eq!(type_error.diagnostics[0].source_span(), &span);
    }

    #[test]
    fn std_bridge_preserves_warning_and_continue_diagnostics() {
        let builtin = RegisteredBuiltin::new(
            "__php_std_test_warns_and_continues",
            test_builtin_warns_and_continues,
            BuiltinMetadata::default(),
        );
        let result = call_php_std_builtin(
            &builtin,
            Vec::new(),
            VmRuntimeSourceSpan {
                file: Some("fixture.php".to_owned()),
                start: 40,
                end: 48,
            },
            &mut OutputBuffer::new(),
            &test_request(Some("fixture.php")),
        );

        assert!(result.status.is_success(), "{:?}", result.status);
        assert_eq!(result.return_value, Some(Value::Bool(true)));
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].id(), "E_PHP_STD_TEST_WARNING");
    }

    #[test]
    fn std_bridge_preserves_by_reference_arguments() {
        let builtin = RegisteredBuiltin::new(
            "__php_std_test_sets_reference",
            test_builtin_sets_reference,
            BuiltinMetadata {
                variadic: false,
                by_ref_params: &[php_std::abi::ByRefParam {
                    name: "value",
                    index: 0,
                }],
            },
        );
        let reference = ReferenceCell::new(Value::Int(1));
        let result = call_php_std_builtin(
            &builtin,
            vec![CallArgument::by_reference(Value::Int(1), reference.clone())],
            VmRuntimeSourceSpan {
                file: Some("fixture.php".to_owned()),
                start: 50,
                end: 58,
            },
            &mut OutputBuffer::new(),
            &test_request(Some("fixture.php")),
        );

        assert!(result.status.is_success(), "{:?}", result.status);
        assert_eq!(result.return_value, Some(Value::Null));
        assert_eq!(reference.get(), Value::Int(42));
    }
}
