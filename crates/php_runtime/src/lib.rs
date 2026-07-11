//! Runtime boundary.
//!
//! This crate owns runtime values, output buffering, diagnostics, request
//! context, resources, selected standard-library state, and the VM-facing object
//! model. Downstream crates should import stable runtime types from [`api`].
//! Instrumentation and debug-only surfaces live under [`experimental`].
//!
//! The root re-exports remain as compatibility aliases while internal crates are
//! migrated. New downstream imports should use the explicit facades instead of
//! relying on the full crate root.
//!
//! Module ownership is grouped in `docs/runtime/module-boundaries.md`. New
//! top-level modules must be categorized there before they are exposed here.

pub mod array;
pub mod autoload;
#[cfg(feature = "full-runtime")]
pub mod builtins;
pub mod callable;
pub mod context;
pub mod convert;
pub mod datetime;
#[cfg(feature = "full-runtime")]
pub mod db;
pub mod diagnostic;
pub mod error_output;
#[cfg(feature = "full-runtime")]
pub mod extension;
pub mod fiber;
pub mod gc;
pub mod generator;
pub mod globals;
pub mod ini;
pub mod jit_array;
pub mod layout_stats;
pub mod numeric_string;
pub mod object;
pub mod output;
#[cfg(feature = "full-runtime")]
pub mod pcre;
#[cfg(feature = "full-runtime")]
pub mod phar;
pub mod reference;
pub mod resource;
pub mod serialization;
pub mod session;
pub mod source_span;
#[cfg(feature = "full-runtime")]
pub mod sqlite;
pub mod status;
pub mod string;
#[cfg(feature = "full-runtime")]
pub mod tokenizer;
pub mod types;
pub mod value;
#[cfg(feature = "full-runtime")]
pub mod xml;
#[cfg(feature = "full-runtime")]
pub mod xml_backend;

/// Stable runtime surface for VM, executor, server, and standard-library code.
///
/// This facade is the preferred import path for runtime values, contexts,
/// diagnostics, resources, object metadata, builtin registration, and
/// PHP-visible status/output types. It intentionally excludes debug GC handles,
/// JIT ABI helpers, and measurement-only counters.
pub mod api {
    pub use crate::array::{
        ArrayEntry, ArrayKey, PackedArrayValues, PhpArray, PhpArrayElementSummary,
        PhpArrayKeyKindSummary, PhpArrayKind, PhpArrayPackedIntReductionError,
        PhpArrayPackedMetadata, PhpArrayShapeKind, PhpArrayShapeLookup,
        PhpArrayShapeLookupFallback, PhpArrayShapeMetadata, PhpArrayValueMut, PhpArrayWriteIntent,
    };
    pub use crate::autoload::AutoloadRegistry;
    #[cfg(feature = "full-runtime")]
    pub use crate::builtins::{
        ApcuState, BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError,
        BuiltinErrorContext, BuiltinRegistry, BuiltinResult, FilesystemRuntimeState,
        FtpOptionValue, FtpState, GettextState, IconvEncodingState, ImapState, InternalFunction,
        LdapState, MbSubstituteCharacter, OpcacheState, OpenSslErrorState, PcntlState,
        ReadlineState, SYSVMSG_EAGAIN, SYSVMSG_EINVAL, SYSVMSG_IPC_NOWAIT, ShmopState, SoapState,
        SocketState, Ssh2State, StreamContextState, StrtokState, SysvMessageQueueState,
        SysvSemaphoreState, SysvSharedMemoryState,
    };
    pub use crate::callable::{
        CallableMethodTarget, CallableValue, ClosureCaptureValue, ClosureContext, ClosureDebugInfo,
        ClosureDebugParameter, ClosurePayload,
    };
    pub use crate::context::{
        ErrorReporting, ProcessCapability, RuntimeContext, RuntimeHttpHeader,
        RuntimeHttpRequestContext, RuntimeHttpResponseState, RuntimeIniOptions, RuntimeInputFilter,
        RuntimeRequestMode, RuntimeUploadedFile, SessionLoadCallback, StrictTypesInfo,
        UploadRegistry, UploadRegistryEntry, parse_cookie_header, parse_form_urlencoded_body,
        parse_query_string, parse_query_string_with_separators,
    };
    pub use crate::convert::{
        ArithmeticNumber, NumericValue, compare, compare_php, equal, equal_php, identical,
        identical_php, reset_float_string_precision, set_float_string_precision,
        to_arithmetic_number, to_arithmetic_number_php, to_array_php, to_bool, to_bool_php,
        to_float, to_float_php, to_int, to_int_php, to_number, to_number_php, to_object_php,
        to_string, to_string_php,
    };
    #[cfg(feature = "full-runtime")]
    pub use crate::db::mysql::{
        MYSQL_TEST_DSN_ENV, MYSQLI_ASSOC, MYSQLI_BOTH, MYSQLI_NUM, MYSQLI_REPORT_ERROR,
        MYSQLI_REPORT_INDEX, MYSQLI_REPORT_OFF, MYSQLI_REPORT_STRICT, MYSQLI_SQLITE_COMPAT_ENV,
        MYSQLI_STORE_RESULT, MYSQLI_USE_RESULT, MYSQLND_CLIENT_INFO, MYSQLND_CLIENT_VERSION,
        MysqlCell, MysqlConnectOptions, MysqlConnection, MysqlError, MysqlErrorKind,
        MysqlQueryResult, MysqlRow, MysqlState,
    };
    #[cfg(feature = "full-runtime")]
    pub use crate::db::postgres::{
        PGSQL_ASSOC, PGSQL_BOTH, PGSQL_NUM, POSTGRES_TEST_DSN_ENV, PostgresConnectOptions,
        PostgresConnection, PostgresError, PostgresErrorKind, PostgresField, PostgresQueryResult,
        PostgresRow, PostgresState,
    };
    pub use crate::diagnostic::{
        JsonDiagnosticContext, PhpReferenceClassification, RuntimeBringupDiagnosticContext,
        RuntimeDiagnostic, RuntimeDiagnosticPayload, RuntimeError, RuntimeEventKind,
        RuntimeSeverity, RuntimeStackFrame, TokenizerParseDiagnosticContext, VmCompileDiagnostic,
        argument_count_error_mvp, array_to_string_warning, division_by_zero_mvp,
        leading_numeric_string_warning, non_numeric_string_type_error, type_error_mvp,
        undefined_function, undefined_global_variable_warning, undefined_variable_warning,
        unhandled_match_error_mvp, unsupported_feature, value_error_mvp,
    };
    pub use crate::error_output::{
        PHP_E_DEPRECATED, PHP_E_ERROR, PHP_E_NOTICE, PHP_E_USER_DEPRECATED, PHP_E_USER_ERROR,
        PHP_E_USER_NOTICE, PHP_E_USER_WARNING, PHP_E_WARNING, PhpDiagnosticChannel,
        PhpDiagnosticDisplayOptions, PhpDiagnosticLocation, emit_php_diagnostic,
        error_reporting_allows_level, format_php_diagnostic_line,
    };
    #[cfg(feature = "full-runtime")]
    pub use crate::extension::{
        ExtensionCapability, ExtensionConstant, ExtensionDescriptor, ExtensionModule,
        ExtensionStateFactory, ExtensionType,
    };
    pub use crate::fiber::{FiberRef, FiberState};
    pub use crate::generator::{GeneratorCallContext, GeneratorRef, GeneratorState};
    pub use crate::globals::GlobalSymbolTable;
    pub use crate::ini::{IniEntrySnapshot, IniRegistry};
    pub use crate::object::{
        AttributeEntry, ClassConstantEntry, ClassConstantFlags, ClassEntry, ClassEnumBackingType,
        ClassEnumCaseEntry, ClassFlags, ClassMethodEntry, ClassMethodFlags, ClassPropertyEntry,
        ClassPropertyFlags, ClassPropertyHooks, ObjectRef, RuntimeType, display_class_name,
        normalize_class_name,
    };
    pub use crate::output::{OutputBuffer, OutputStats};
    #[cfg(feature = "full-runtime")]
    pub use crate::pcre::{
        PREG_BACKTRACK_LIMIT_ERROR, PREG_BAD_UTF8_ERROR, PREG_BAD_UTF8_OFFSET_ERROR,
        PREG_GREP_INVERT, PREG_INTERNAL_ERROR, PREG_JIT_STACKLIMIT_ERROR, PREG_NO_ERROR,
        PREG_OFFSET_CAPTURE, PREG_PATTERN_ORDER, PREG_RECURSION_LIMIT_ERROR, PREG_SET_ORDER,
        PREG_SPLIT_DELIM_CAPTURE, PREG_SPLIT_NO_EMPTY, PREG_SPLIT_OFFSET_CAPTURE,
        PREG_UNMATCHED_AS_NULL, PcreCache,
    };
    #[cfg(feature = "full-runtime")]
    pub use crate::phar::{PharArchive, PharEntry, PharError, PharUri};
    pub use crate::reference::{
        Lvalue, LvalueError, LvalueKind, ReferenceCell, ReferencePlaceholder, Slot, TempValue,
        ValueSlot,
    };
    pub use crate::resource::{
        FilesystemCapabilities, ResourceId, ResourceKind, ResourceRef, ResourceTable, Stream,
        StreamFilterMode, StreamFlags, StreamMetadata, StreamOpenError, StreamOpenMode,
        StreamSeekWhence, StreamWrapperRegistry,
    };
    pub use crate::serialization::{
        SerializationError, UnserializeOptions, serialize, unserialize, unserialize_prefix,
    };
    pub use crate::session::{
        PHP_SESSION_ACTIVE, PHP_SESSION_DISABLED, PHP_SESSION_NONE, SessionState,
    };
    pub use crate::source_span::RuntimeSourceSpan;
    #[cfg(feature = "full-runtime")]
    pub use crate::sqlite::{
        SQLITE3_ASSOC, SQLITE3_BLOB, SQLITE3_BOTH, SQLITE3_DETERMINISTIC, SQLITE3_FLOAT,
        SQLITE3_INTEGER, SQLITE3_NULL, SQLITE3_NUM, SQLITE3_OPEN_CREATE, SQLITE3_OPEN_READONLY,
        SQLITE3_OPEN_READWRITE, SQLITE3_TEXT, SqliteState,
    };
    pub use crate::status::{ExecutionStatus, ExitStatus};
    pub use crate::string::{PhpString, SymbolId};
    #[cfg(feature = "full-runtime")]
    pub use crate::tokenizer;
    pub use crate::types::{runtime_type_name, value_matches_runtime_type, value_type_name};
    pub use crate::value::{FloatValue, Value};
}

/// Debug and test runtime surface.
///
/// These exports are public so local tests and VM diagnostics can inspect graph
/// shape. They are not PHP-visible APIs and are not compatibility promises for
/// downstream crates.
pub mod debug {
    #[doc(hidden)]
    pub use crate::array::WeakArrayHandle;
    #[doc(hidden)]
    #[cfg(feature = "full-runtime")]
    pub use crate::builtins::CurlNetworkTestOverride;
    #[doc(hidden)]
    #[cfg(feature = "full-runtime")]
    pub use crate::builtins::set_curl_network_tests_override_for_tests;
    #[doc(hidden)]
    pub use crate::gc::{
        GcCollectResult, GcCollectedEntity, GcCycleCandidate, GcEntityId, GcEntityKind, GcNode,
        GcRoot, GcRootKind, GcSnapshot, GcTrackedHeap, scan_roots,
    };
    #[doc(hidden)]
    pub use crate::object::WeakObjectHandle;
    #[doc(hidden)]
    pub use crate::reference::WeakReferenceHandle;
}

/// Unstable runtime instrumentation, debug, and ABI helper surface.
///
/// These exports are public because local performance tooling, tests, and JIT
/// experiments consume them. They are not a compatibility promise for
/// downstream crates.
pub mod experimental {
    #[doc(hidden)]
    pub use crate::debug::*;
    #[doc(hidden)]
    pub use crate::jit_array::{
        PHP_JIT_ARRAY_LAYOUT_VERSION, PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT,
        PHP_JIT_ARRAY_STATUS_FALLBACK, PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT, PHP_JIT_ARRAY_STATUS_OK,
        PhpJitArrayAbiError, php_jit_array_fetch_int_slow, php_jit_array_is_packed_ints,
        php_jit_array_layout_guard, php_jit_array_len,
    };
    #[doc(hidden)]
    pub use crate::layout_stats;
    #[doc(hidden)]
    pub use crate::numeric_string;
}

pub use array::{
    ArrayEntry, ArrayKey, PackedArrayValues, PhpArray, PhpArrayElementSummary,
    PhpArrayKeyKindSummary, PhpArrayKind, PhpArrayPackedIntReductionError, PhpArrayPackedMetadata,
    PhpArrayShapeKind, PhpArrayShapeLookup, PhpArrayShapeLookupFallback, PhpArrayShapeMetadata,
    PhpArrayValueMut, PhpArrayWriteIntent, WeakArrayHandle,
};
pub use autoload::AutoloadRegistry;
#[cfg(feature = "full-runtime")]
pub use builtins::{
    ApcuState, BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError,
    BuiltinErrorContext, BuiltinRegistry, BuiltinResult, FilesystemRuntimeState, FtpOptionValue,
    FtpState, GettextState, IconvEncodingState, ImapState, InternalFunction, JSON_ERROR_RECURSION,
    JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_THROW_ON_ERROR, LdapState, MbSubstituteCharacter,
    OpcacheState, OpenSslErrorState, PcntlState, ReadlineState, ShmopState, SoapState, SocketState,
    Ssh2State, StreamContextState, StrtokState, SysvMessageQueueState, SysvSemaphoreState,
    SysvSharedMemoryState,
};
pub use callable::{
    CallableMethodTarget, CallableValue, ClosureCaptureValue, ClosureContext, ClosureDebugInfo,
    ClosureDebugParameter, ClosurePayload,
};
pub use context::{
    ErrorReporting, ProcessCapability, RuntimeContext, RuntimeHttpHeader,
    RuntimeHttpRequestContext, RuntimeHttpResponseState, RuntimeIniOptions, RuntimeInputFilter,
    RuntimeRequestMode, RuntimeUploadedFile, SessionLoadCallback, StrictTypesInfo, UploadRegistry,
    UploadRegistryEntry, parse_cookie_header, parse_form_urlencoded_body, parse_query_string,
    parse_query_string_with_separators,
};
pub use convert::{
    ArithmeticNumber, NumericValue, compare, compare_php, equal, equal_php, identical,
    identical_php, reset_float_string_precision, set_float_string_precision, to_arithmetic_number,
    to_arithmetic_number_php, to_array_php, to_bool, to_bool_php, to_float, to_float_php, to_int,
    to_int_php, to_number, to_number_php, to_object_php, to_string, to_string_php,
};
#[cfg(feature = "full-runtime")]
pub use db::mysql::{
    MYSQL_TEST_DSN_ENV, MYSQLI_ASSOC, MYSQLI_BOTH, MYSQLI_NUM, MYSQLI_REPORT_ERROR,
    MYSQLI_REPORT_INDEX, MYSQLI_REPORT_OFF, MYSQLI_REPORT_STRICT, MYSQLI_SQLITE_COMPAT_ENV,
    MYSQLI_STORE_RESULT, MYSQLI_USE_RESULT, MYSQLND_CLIENT_INFO, MYSQLND_CLIENT_VERSION, MysqlCell,
    MysqlConnectOptions, MysqlConnection, MysqlError, MysqlErrorKind, MysqlQueryResult, MysqlRow,
    MysqlState,
};
#[cfg(feature = "full-runtime")]
pub use db::postgres::{
    PGSQL_ASSOC, PGSQL_BOTH, PGSQL_NUM, POSTGRES_TEST_DSN_ENV, PostgresConnectOptions,
    PostgresConnection, PostgresError, PostgresErrorKind, PostgresField, PostgresQueryResult,
    PostgresRow, PostgresState,
};
pub use diagnostic::{
    JsonDiagnosticContext, PhpReferenceClassification, RuntimeBringupDiagnosticContext,
    RuntimeDiagnostic, RuntimeDiagnosticPayload, RuntimeError, RuntimeEventKind, RuntimeSeverity,
    RuntimeStackFrame, TokenizerParseDiagnosticContext, VmCompileDiagnostic,
    argument_count_error_mvp, array_to_string_warning, division_by_zero_mvp,
    leading_numeric_string_warning, non_numeric_string_type_error, type_error_mvp,
    undefined_function, undefined_global_variable_warning, undefined_variable_warning,
    unhandled_match_error_mvp, unsupported_feature, value_error_mvp,
};
pub use error_output::{
    PHP_E_DEPRECATED, PHP_E_ERROR, PHP_E_NOTICE, PHP_E_USER_DEPRECATED, PHP_E_USER_ERROR,
    PHP_E_USER_NOTICE, PHP_E_USER_WARNING, PHP_E_WARNING, PhpDiagnosticChannel,
    PhpDiagnosticDisplayOptions, PhpDiagnosticLocation, emit_php_diagnostic,
    error_reporting_allows_level, format_php_diagnostic_line,
};
#[cfg(feature = "full-runtime")]
pub use extension::{
    ExtensionCapability, ExtensionConstant, ExtensionDescriptor, ExtensionModule,
    ExtensionStateFactory, ExtensionType,
};
pub use fiber::{FiberRef, FiberState};
pub use gc::{
    GcCollectResult, GcCollectedEntity, GcCycleCandidate, GcEntityId, GcEntityKind, GcNode, GcRoot,
    GcRootKind, GcSnapshot, GcTrackedHeap, scan_roots,
};
pub use generator::{GeneratorCallContext, GeneratorRef, GeneratorState};
pub use globals::GlobalSymbolTable;
pub use ini::{IniEntrySnapshot, IniRegistry};
pub use jit_array::{
    PHP_JIT_ARRAY_LAYOUT_VERSION, PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT, PHP_JIT_ARRAY_STATUS_FALLBACK,
    PHP_JIT_ARRAY_STATUS_KEY_MISS_EXIT, PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT, PHP_JIT_ARRAY_STATUS_OK,
    PhpJitArrayAbiError, php_jit_array_fetch_int_slow, php_jit_array_is_packed_ints,
    php_jit_array_layout_guard, php_jit_array_len, php_jit_record_array_lookup,
};
pub use object::{
    AttributeEntry, ClassConstantEntry, ClassConstantFlags, ClassEntry, ClassEnumBackingType,
    ClassEnumCaseEntry, ClassFlags, ClassMethodEntry, ClassMethodFlags, ClassPropertyEntry,
    ClassPropertyFlags, ClassPropertyHooks, ObjectRef, RuntimeType, WeakObjectHandle,
    display_class_name, normalize_class_name,
};
pub use output::{OutputBuffer, OutputStats};
#[cfg(feature = "full-runtime")]
pub use pcre::{
    PREG_BACKTRACK_LIMIT_ERROR, PREG_BAD_UTF8_ERROR, PREG_BAD_UTF8_OFFSET_ERROR, PREG_GREP_INVERT,
    PREG_INTERNAL_ERROR, PREG_JIT_STACKLIMIT_ERROR, PREG_NO_ERROR, PREG_OFFSET_CAPTURE,
    PREG_PATTERN_ORDER, PREG_RECURSION_LIMIT_ERROR, PREG_SET_ORDER, PREG_SPLIT_DELIM_CAPTURE,
    PREG_SPLIT_NO_EMPTY, PREG_SPLIT_OFFSET_CAPTURE, PREG_UNMATCHED_AS_NULL, PcreCache,
};
#[cfg(feature = "full-runtime")]
pub use phar::{PharArchive, PharEntry, PharError, PharUri};
pub use reference::{
    Lvalue, LvalueError, LvalueKind, ReferenceCell, ReferencePlaceholder, Slot, TempValue,
    ValueSlot, WeakReferenceHandle,
};
pub use resource::{
    FilesystemCapabilities, ResourceId, ResourceKind, ResourceRef, ResourceTable, Stream,
    StreamFilterMode, StreamFlags, StreamMetadata, StreamOpenError, StreamOpenMode,
    StreamSeekWhence, StreamWrapperRegistry,
};
pub use serialization::{
    SerializationError, UnserializeOptions, serialize, serialize_object_properties,
    serialize_with_precision, unserialize, unserialize_prefix,
};
pub use session::{PHP_SESSION_ACTIVE, PHP_SESSION_DISABLED, PHP_SESSION_NONE, SessionState};
pub use source_span::RuntimeSourceSpan;
#[cfg(feature = "full-runtime")]
pub use sqlite::{
    SQLITE3_ASSOC, SQLITE3_BLOB, SQLITE3_BOTH, SQLITE3_DETERMINISTIC, SQLITE3_FLOAT,
    SQLITE3_INTEGER, SQLITE3_NULL, SQLITE3_NUM, SQLITE3_OPEN_CREATE, SQLITE3_OPEN_READONLY,
    SQLITE3_OPEN_READWRITE, SQLITE3_TEXT, SqliteState,
};
pub use status::{ExecutionStatus, ExitStatus};
pub use string::{PhpString, SymbolId};
pub use types::{runtime_type_name, value_matches_runtime_type, value_type_name};
pub use value::{FloatValue, Value};

#[cfg(test)]
mod tests {
    use super::{CallableValue, ExecutionStatus, ExitStatus, OutputBuffer, PhpString, Value};

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
        let closure = Value::closure(crate::ClosurePayload::new(
            7,
            vec![crate::ClosureCaptureValue::by_value(
                "x".to_owned(),
                Value::Int(3),
            )],
        ));

        assert!(matches!(
            user.as_callable(),
            Some(CallableValue::UserFunction { name }) if name == "foo"
        ));
        assert!(format!("{builtin:?}").contains("internal_builtin"));
        assert!(format!("{method:?}").contains("method_placeholder"));
        assert!(format!("{unresolved:?}").contains("unresolved_dynamic"));
        assert!(matches!(
            closure.as_closure(),
            Some(payload) if payload.function == 7 && payload.captures.len() == 1
        ));
    }
}
