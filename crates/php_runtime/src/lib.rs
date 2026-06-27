//! Runtime boundary.
//!
//! This crate will own runtime values, output buffering, runtime diagnostics,
//! exit classification, tables, builtins, and controlled runtime context. At
//! runtime scalar it exposes the minimal scalar value and output model used by the
//! first VM slices.

pub mod array;
pub mod autoload;
pub mod builtins;
pub mod context;
pub mod convert;
pub mod datetime;
pub mod diagnostic;
pub mod error_output;
pub mod fiber;
pub mod gc;
pub mod generator;
pub mod globals;
pub mod ini;
pub mod jit_array;
pub mod numeric_string;
pub mod object;
pub mod output;
pub mod pcre;
pub mod reference;
pub mod resource;
pub mod serialization;
pub mod status;
pub mod string;
pub mod todo_runtime;
pub mod tokenizer;
pub mod types;
pub mod value;

pub use array::{
    ArrayEntry, ArrayKey, PhpArray, PhpArrayKind, PhpArrayPackedMetadata, WeakArrayHandle,
};
pub use autoload::AutoloadRegistry;
pub use builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinRegistry,
    BuiltinResult, InternalFunction, RuntimeSourceSpan, StrtokState,
};
pub use context::{
    ErrorReporting, ProcessCapability, RuntimeContext, RuntimeIniOptions, StrictTypesInfo,
};
pub use convert::{
    ArithmeticNumber, NumericValue, compare, equal, identical, reset_float_string_precision,
    set_float_string_precision, to_arithmetic_number, to_bool, to_float, to_int, to_number,
    to_string,
};
pub use diagnostic::{
    PhpReferenceClassification, RuntimeDiagnostic, RuntimeError, RuntimeSeverity,
    RuntimeStackFrame, division_by_zero_mvp, type_error_mvp, undefined_function,
    undefined_variable_warning, unsupported_feature,
};
pub use error_output::{
    PHP_E_DEPRECATED, PHP_E_ERROR, PHP_E_NOTICE, PHP_E_USER_DEPRECATED, PHP_E_USER_ERROR,
    PHP_E_USER_NOTICE, PHP_E_USER_WARNING, PHP_E_WARNING, PhpDiagnosticChannel,
    PhpDiagnosticDisplayOptions, PhpDiagnosticLocation, emit_php_diagnostic,
    error_reporting_allows_level, format_php_diagnostic_line,
};
pub use fiber::{FiberRef, FiberState};
pub use gc::{
    GcCollectResult, GcCollectedEntity, GcCycleCandidate, GcEntityId, GcEntityKind, GcNode, GcRoot,
    GcRootKind, GcSnapshot, GcTrackedHeap, scan_roots,
};
pub use generator::{GeneratorRef, GeneratorState};
pub use globals::GlobalSymbolTable;
pub use ini::{IniEntrySnapshot, IniRegistry};
pub use jit_array::{
    PHP_JIT_ARRAY_LAYOUT_VERSION, PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT, PHP_JIT_ARRAY_STATUS_FALLBACK,
    PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT, PHP_JIT_ARRAY_STATUS_OK, PhpJitArrayAbiError,
    php_jit_array_fetch_int_slow, php_jit_array_is_packed_ints, php_jit_array_layout_guard,
    php_jit_array_len,
};
pub use object::{
    AttributeEntry, ClassConstantEntry, ClassConstantFlags, ClassEntry, ClassEnumBackingType,
    ClassEnumCaseEntry, ClassFlags, ClassMethodEntry, ClassMethodFlags, ClassPropertyEntry,
    ClassPropertyFlags, ClassPropertyHooks, ObjectRef, RuntimeType, WeakObjectHandle,
};
pub use output::{OutputBuffer, OutputStats};
pub use pcre::{
    PREG_BACKTRACK_LIMIT_ERROR, PREG_BAD_UTF8_ERROR, PREG_BAD_UTF8_OFFSET_ERROR, PREG_GREP_INVERT,
    PREG_INTERNAL_ERROR, PREG_JIT_STACKLIMIT_ERROR, PREG_NO_ERROR, PREG_OFFSET_CAPTURE,
    PREG_PATTERN_ORDER, PREG_RECURSION_LIMIT_ERROR, PREG_SET_ORDER, PREG_SPLIT_DELIM_CAPTURE,
    PREG_SPLIT_NO_EMPTY, PREG_SPLIT_OFFSET_CAPTURE, PREG_UNMATCHED_AS_NULL, PcreCache,
};
pub use reference::{
    ReferenceCell, ReferencePlaceholder, Slot, TempValue, ValueSlot, WeakReferenceHandle,
};
pub use resource::{
    FilesystemCapabilities, ResourceId, ResourceKind, ResourceRef, ResourceTable, Stream,
    StreamFlags, StreamMetadata, StreamOpenError, StreamOpenMode, StreamWrapperRegistry,
};
pub use serialization::{SerializationError, UnserializeOptions, serialize, unserialize};
pub use status::{ExecutionStatus, ExitStatus};
pub use string::PhpString;
pub use todo_runtime::{RuntimeTodo, runtime_skeleton_status};
pub use types::{runtime_type_name, value_matches_runtime_type, value_type_name};
pub use value::{
    CallableMethodTarget, CallableValue, ClosureCaptureValue, ClosureDebugInfo, FloatValue, Value,
};

#[cfg(test)]
mod tests {
    use super::{
        CallableValue, ExecutionStatus, ExitStatus, OutputBuffer, PhpString, RuntimeTodo, Value,
        runtime_skeleton_status,
    };

    #[test]
    fn exposes_runtime_skeleton() {
        let todo = RuntimeTodo::new("values, diagnostics, output, and context");
        assert_eq!(todo.area(), "values, diagnostics, output, and context");
        assert_eq!(runtime_skeleton_status(), "runtime-skeleton");
        assert_eq!(
            php_testkit::reference_checkout_path(),
            "third_party/php-src"
        );
    }

    #[test]
    fn value_clone_preserves_scalar_payloads() {
        let values = vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(42),
            Value::float(1.5),
            Value::string(vec![b'a', 0xff, b'z']),
            Value::Uninitialized,
        ];

        for value in values {
            assert_eq!(value.clone(), value);
        }
    }

    #[test]
    fn value_php_string_is_byte_exact_and_roundtrips() {
        let bytes = vec![0x66, 0x6f, 0x80, 0xff, 0x00];
        let string = PhpString::from_bytes(bytes.clone());

        assert_eq!(string.as_bytes(), bytes.as_slice());
        assert_eq!(string.clone().into_bytes(), bytes);
        assert_eq!(PhpString::from_test_str("abc").as_bytes(), b"abc");
        assert_eq!(string.len(), 5);
    }

    #[test]
    fn value_output_buffer_writes_bytes_and_test_strings() {
        let mut output = OutputBuffer::new();
        output.write_test_str("hi");
        output.write_bytes([0, 0xff]);
        output.write_php_string(&PhpString::from_bytes(vec![b'!']));

        assert_eq!(output.as_bytes(), &[b'h', b'i', 0, 0xff, b'!']);
        assert!(output.to_string_lossy().starts_with("hi"));
        assert_eq!(output.clone().into_bytes(), vec![b'h', b'i', 0, 0xff, b'!']);
        output.clear();
        assert_eq!(output.as_bytes(), b"");
    }

    #[test]
    fn value_exit_statuses_are_stable_and_queryable() {
        assert!(ExecutionStatus::success().is_success());
        assert_eq!(ExitStatus::Success.to_string(), "success");

        let statuses = [
            (
                ExecutionStatus::compile_error("parse failed"),
                ExitStatus::CompileError,
                "compile_error: parse failed",
            ),
            (
                ExecutionStatus::runtime_error("bad register"),
                ExitStatus::RuntimeError,
                "runtime_error: bad register",
            ),
            (
                ExecutionStatus::unsupported("eval"),
                ExitStatus::Unsupported,
                "unsupported: eval",
            ),
            (
                ExecutionStatus::fatal("engine invariant"),
                ExitStatus::Fatal,
                "fatal: engine invariant",
            ),
        ];

        for (status, expected, display) in statuses {
            assert_eq!(status.exit_status(), expected);
            assert_eq!(status.to_string(), display);
            assert!(!status.is_success());
        }
    }

    #[test]
    fn value_display_and_debug_are_available_but_not_var_dump() {
        assert_eq!(Value::Null.to_string(), "null");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Int(7).to_string(), "7");
        assert_eq!(Value::string(b"bytes".to_vec()).to_string(), "bytes");
        assert!(format!("{:?}", Value::Uninitialized).contains("Uninitialized"));
    }

    #[test]
    fn callable_values_cover_creation_variants() {
        let user = Value::user_function_callable("foo");
        let builtin = Value::internal_builtin_callable("trim");
        let method = Value::method_callable_placeholder("C::m");
        let unresolved = Value::unresolved_callable("$dynamic");
        let closure = Value::closure(
            7,
            vec![crate::ClosureCaptureValue::by_value(
                "x".to_owned(),
                Value::Int(3),
            )],
        );

        assert!(matches!(
            user,
            Value::Callable(CallableValue::UserFunction { ref name }) if name == "foo"
        ));
        assert!(format!("{builtin:?}").contains("internal_builtin"));
        assert!(format!("{method:?}").contains("method_placeholder"));
        assert!(format!("{unresolved:?}").contains("unresolved_dynamic"));
        assert!(matches!(
            closure.as_closure(),
            Some((7, captures, None, None, None, None, None)) if captures.len() == 1
        ));
    }
}
