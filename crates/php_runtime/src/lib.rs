//! Runtime boundary.
//!
//! This crate owns runtime values, output buffering, diagnostics, request
//! context, resources, selected standard-library state, and the VM-facing object
//! model. Downstream crates should import stable runtime types from [`api`].
//! Instrumentation and debug-only surfaces live under [`experimental`].
//!
//! Module ownership is grouped in `docs/runtime/module-boundaries.md`. New
//! top-level modules must be categorized there before they are exposed here.
//!
//! ```
//! use php_runtime::api::{RuntimeContext, Value};
//! use php_runtime::experimental::layout_stats;
//!
//! let _ = RuntimeContext::default();
//! let _ = Value::Null;
//! let _ = layout_stats::RuntimeLayoutStats::default();
//! ```
//!
//! Internal implementation modules are not public API:
//!
//! ```compile_fail
//! use php_runtime::value::Value;
//! ```
//!
//! ```compile_fail
//! use php_runtime::xml_backend::BackendDocument;
//! ```

// Unsafe stays confined to the audited `runtime_memory` module (see
// docs/adr/0020 and docs/performance/runtime-memory-safety-audit.md);
// every other module in this crate remains forbidden from using it.
#![deny(unsafe_code)]

mod array;
mod autoload;
#[cfg(feature = "full-runtime")]
mod builtins;
mod callable;
mod context;
mod convert;
mod datetime;
#[cfg(feature = "full-runtime")]
mod db;
mod diagnostic;
mod error_output;
#[cfg(feature = "full-runtime")]
mod extension;
mod fiber;
mod gc;
mod generator;
mod globals;
mod ini;
mod layout_stats;
mod native_ops;
mod numeric_string;
mod object;
mod output;
#[cfg(any(feature = "full-runtime", feature = "pcre-core"))]
mod pcre;
#[cfg(feature = "full-runtime")]
mod phar;
mod reference;
mod request_state;
mod resource;
pub(crate) mod runtime_memory;
mod serialization;
mod session;
mod source_span;
#[cfg(feature = "full-runtime")]
mod sqlite;
mod status;
mod string;
#[cfg(feature = "full-runtime")]
mod tokenizer;
mod types;
mod value;
#[cfg(feature = "full-runtime")]
mod xml;
#[cfg(feature = "full-runtime")]
mod xml_backend;

/// Stable runtime surface for VM, executor, server, and standard-library code.
///
/// This facade is the preferred import path for runtime values, contexts,
/// diagnostics, resources, object metadata, builtin registration, and
/// PHP-visible status/output types. It intentionally excludes debug GC handles,
/// JIT ABI helpers, and measurement-only counters.
pub mod api {
    pub use crate::builtins::{
        NativeBaseConversion, NativeParsedBaseNumber, baseline_count_recursive_value,
        native_base_conversion, native_decimal_base_conversion, native_parse_base_digits,
        native_parse_intval_string_base, native_round_f64,
    };
    pub use crate::runtime_memory::{NativeZeroed, StableNativeArena, StableNativeArenaUsage};

    /// Stable date/time runtime operations consumed by the VM.
    pub mod datetime {
        pub use crate::datetime::*;
    }

    /// Stable Phar URI and archive operations consumed by the VM.
    #[cfg(feature = "full-runtime")]
    pub mod phar {
        pub use crate::phar::*;
    }

    /// Stable SQLite operations consumed by the VM.
    #[cfg(feature = "full-runtime")]
    pub mod sqlite {
        pub use crate::sqlite::*;
    }

    /// Stable tokenizer operations consumed by the VM and standard library.
    #[cfg(feature = "full-runtime")]
    pub mod tokenizer {
        pub use crate::tokenizer::*;
    }

    /// Stable XML/DOM/SimpleXML operations. Backend implementation types stay private.
    #[cfg(feature = "full-runtime")]
    pub mod xml {
        pub use crate::xml::*;
    }

    pub use crate::array::{
        ArrayEntry, ArrayKey, PHP_ARRAY_APPEND_OVERFLOW_MESSAGE, PackedArrayValues, PhpArray,
        PhpArrayAppendError, PhpArrayElementSummary, PhpArrayKeyKindSummary, PhpArrayKind,
        PhpArrayPackedIntReductionError, PhpArrayPackedMetadata, PhpArrayShapeKind,
        PhpArrayShapeLookup, PhpArrayShapeLookupFallback, PhpArrayShapeMetadata, PhpArrayValueMut,
        PhpArrayWriteIntent,
    };
    pub use crate::autoload::AutoloadRegistry;
    #[cfg(feature = "full-runtime")]
    pub use crate::builtins::{
        ApcuState, BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError,
        BuiltinErrorContext, BuiltinExecutionKind, BuiltinHandlerKind, BuiltinOutcome,
        BuiltinRegistry, BuiltinRequestState, BuiltinResult, CurlState, FILTER_DEFAULT,
        FILTER_FORCE_ARRAY, FILTER_NULL_ON_FAILURE, FILTER_REQUIRE_ARRAY, FILTER_REQUIRE_SCALAR,
        FilesystemRuntimeState, FtpOptionValue, FtpState, GcRequestState, GettextState,
        IconvEncodingState, ImapConnectionConfig, ImapMailboxSnapshot, ImapState, InternalFunction,
        JSON_ERROR_RECURSION, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_THROW_ON_ERROR, JsonRequestState,
        LdapSearchScope, LdapState, MbSubstituteCharacter, NATIVE_HTML_ESCAPE_DEFAULT_FLAGS,
        NATIVE_JSON_DIRECT_ENCODE_FLAGS, NATIVE_JSON_FORCE_OBJECT, NATIVE_JSON_HEX_AMP,
        NATIVE_JSON_HEX_APOS, NATIVE_JSON_HEX_QUOT, NATIVE_JSON_HEX_TAG,
        NATIVE_JSON_INVALID_UTF8_IGNORE, NATIVE_JSON_INVALID_UTF8_SUBSTITUTE,
        NATIVE_JSON_NUMERIC_CHECK, NATIVE_JSON_PRESERVE_ZERO_FRACTION, NATIVE_JSON_PRETTY_PRINT,
        NATIVE_JSON_UNESCAPED_LINE_TERMINATORS, NATIVE_JSON_UNESCAPED_SLASHES,
        NATIVE_JSON_UNESCAPED_UNICODE, NATIVE_PHP_QUERY_RFC3986, NORMALIZER_FORM_C,
        NORMALIZER_FORM_D, NORMALIZER_FORM_KC, NORMALIZER_FORM_KD, NativeCookieOptions,
        NativeFilterResult, NativeFilterValue, NativeGlobPublished, NativeInputKey,
        NativeInputSegment, NativeNetworkAddress, NativePackArgument, NativePregCallbackPlanResult,
        NativePregCapturePublisher, NativePregPublishedMatch, NativePregPublishedMatchAll,
        NativePregReplaceResult, NativePrintfScalar, NativeStatRecord,
        NativeStructuredValuePublisher, NativeUnpackKey, NativeUnpackValue, NativeZlibDecodePlan,
        OpcacheState, OpenSslErrorState, PcntlState, PcreRequestState, ReadlineState,
        SYSVMSG_EAGAIN, SYSVMSG_EINVAL, SYSVMSG_IPC_NOWAIT, ShmopState, SoapParsedBody, SoapState,
        SocketState, Ssh2FingerprintHash, Ssh2State, StreamContextState, StrtokState,
        SysvMessageQueueState, SysvSemaphoreError, SysvSemaphoreState, SysvSharedMemoryState,
        ZLIB_ENCODING_DEFLATE, ZLIB_ENCODING_GZIP, ZLIB_ENCODING_RAW, append_json_default_string,
        append_json_string_with_flags, build_native_cookie_header_value, build_soap_envelope,
        decode_native_json_associative_into, exact_json_decode, exact_json_encode,
        exact_json_last_error, exact_json_last_error_msg, exact_json_validate, exact_preg_filter,
        exact_preg_grep, exact_preg_last_error, exact_preg_last_error_msg, exact_preg_match,
        exact_preg_match_all, exact_preg_quote, exact_preg_replace, exact_preg_split, exact_printf,
        exact_sprintf, exact_vprintf, exact_vsprintf, hash_algorithm_exists,
        igbinary_serialize_value, igbinary_unserialize_value, is_normalized_string, load_wsdl,
        msgpack_pack_value, msgpack_unpack_value, native_addcslashes_into,
        native_addcslashes_output_length, native_base64_decode_into,
        native_base64_decode_output_length, native_base64_encode_into,
        native_base64_encode_output_length, native_basename, native_bcadd, native_bccomp,
        native_bcdiv, native_bcmod, native_bcmul, native_bcpow, native_bcpowmod, native_bcsqrt,
        native_bcsub, native_bin2hex_into, native_bin2hex_output_length, native_chdir_target,
        native_chmod, native_convert_uudecode_into, native_convert_uudecode_output_length,
        native_convert_uuencode_into, native_convert_uuencode_output_length, native_crc32,
        native_directory_entries, native_dirname, native_disk_space, native_file_exists,
        native_file_get_contents, native_file_lines_into, native_file_put_contents,
        native_filegroup, native_filemtime, native_fileowner, native_fileperms, native_filesize,
        native_filetype, native_filter_id, native_filter_input_source_index, native_filter_names,
        native_filter_scalar, native_glob_into, native_hash_hmac_into,
        native_hash_hmac_output_length, native_hash_into, native_hash_output_length,
        native_hex2bin_into, native_hex2bin_output_length, native_html_entity_decode_into,
        native_html_entity_decode_output_length, native_html_escape_into,
        native_html_escape_output_length, native_inet_ntop, native_inet_pton, native_ip2long,
        native_is_dir, native_is_file, native_is_link, native_is_readable, native_is_writable,
        native_long2ip, native_mb_canonical_encoding, native_mb_check_encoding, native_mb_chr,
        native_mb_convert_case, native_mb_convert_encoding, native_mb_convert_simple_case,
        native_mb_detect_encoding, native_mb_encoding_aliases, native_mb_encoding_names,
        native_mb_first_char_case, native_mb_ord, native_mb_position, native_mb_strcut,
        native_mb_strimwidth, native_mb_strlen, native_mb_strwidth, native_mb_substr,
        native_mb_substr_count, native_md5_into, native_md5_output_length, native_mkdir,
        native_natural_compare, native_number_format, native_parse_str_into, native_parse_url_into,
        native_pathinfo_into, native_preg_callback_plan_into, native_preg_grep_into,
        native_preg_match_all_into, native_preg_match_into, native_preg_replace_many_into,
        native_preg_replace_scalar, native_preg_split_into, native_quoted_printable_decode_into,
        native_quoted_printable_decode_output_length, native_quotemeta_into,
        native_quotemeta_output_length, native_random_fill, native_realpath, native_rename,
        native_rmdir, native_scandir_into, native_sha1_into, native_sha1_output_length,
        native_stat, native_str_pad_into, native_str_pad_output_length, native_stream_is_local,
        native_stream_resolve_include_path, native_string_search_slice, native_stripcslashes_into,
        native_stripcslashes_output_length, native_stripslashes_into,
        native_stripslashes_output_length, native_strpbrk, native_strrchr, native_strtr_into,
        native_substr_compare, native_substr_replace_into, native_substr_replace_output_length,
        native_symlink, native_tempnam, native_tmpfile, native_touch, native_ucwords_into,
        native_unlink, native_unpack_hex_into, native_url_decode_into,
        native_url_decode_output_length, native_url_encode_into, native_url_encode_output_length,
        native_version_compare, native_version_operator_matches, native_zlib_decode,
        native_zlib_decode_auto, native_zlib_encode_into, native_zlib_encode_output_capacity,
        normalize_string, parse_soap_response, parse_wsdl, soap_http_post,
        validate_fileinfo_options, validate_native_json, visit_json_string_with_flags,
        visit_native_pack, visit_native_printf_scalars, visit_native_unpack,
    };
    pub use crate::callable::{
        CallableMethodTarget, CallableValue, ClosureCaptureValue, ClosureContext, ClosureDebugInfo,
        ClosureDebugParameter, ClosurePayload,
    };
    pub use crate::context::{
        ErrorReporting, ProcessCapability, RuntimeContext, RuntimeHttpHeader,
        RuntimeHttpRequestContext, RuntimeHttpResponseState, RuntimeIniOptions, RuntimeInputFilter,
        RuntimeRequestMode, RuntimeUploadedFile, SessionIdGenerateCallback, SessionLoadCallback,
        StrictTypesInfo, UploadRegistry, UploadRegistryEntry, parse_cookie_header,
        parse_form_urlencoded_body, parse_query_string, parse_query_string_with_separators,
    };
    pub use crate::convert::{
        ArithmeticNumber, NumericValue, PHP_FLOAT_STRING_BUFFER_CAPACITY, compare, compare_php,
        equal, equal_php, float_fits_int, float_to_php_string, float_to_php_string_bytes,
        identical, identical_php, native_bytes_to_number, php_float_to_int,
        reset_float_string_precision, set_float_string_precision, to_arithmetic_number,
        to_arithmetic_number_php, to_array_php, to_bool, to_bool_php, to_float, to_float_php,
        to_int, to_int_php, to_number, to_number_php, to_object_php, to_string, to_string_php,
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
        IncludeFailureDiagnosticContext, JsonDiagnosticContext, PhpReferenceClassification,
        RuntimeBringupDiagnosticContext, RuntimeDiagnostic, RuntimeDiagnosticPayload, RuntimeError,
        RuntimeEventKind, RuntimeSeverity, RuntimeStackFrame, TokenizerParseDiagnosticContext,
        VmCompileDiagnostic, argument_count_error_mvp, array_to_string_warning,
        division_by_zero_mvp, leading_numeric_string_warning, non_numeric_string_type_error,
        type_error_mvp, undefined_function, undefined_global_variable_warning,
        undefined_variable_warning, unhandled_match_error_mvp, unsupported_feature,
        value_error_mvp,
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
    pub use crate::native_ops::{
        JIT_HELPER_ECHO_VALUE, JIT_HELPER_SCALAR_BINARY, JIT_HELPER_SCALAR_CAST,
        JIT_HELPER_SCALAR_COMPARE, JIT_HELPER_SCALAR_UNARY, JitHelperId, NATIVE_OPERATION_ABI_HASH,
        NATIVE_OPERATION_ABI_VERSION, NATIVE_OPERATION_REGISTRY, NativeAbiType, NativeBinaryOp,
        NativeCastOp, NativeCompareOp, NativeOperationContext, NativeOperationDescriptor,
        NativeOperationFamily, NativeOperationStatus, NativeOwnership, NativeUnaryOp,
        baseline_binary, baseline_cast, baseline_compare, baseline_unary, lookup_native_operation,
        native_echo, native_operation_registry_is_stable,
    };
    pub use crate::numeric_string::array_key_integer_bytes;
    pub use crate::object::{
        AttributeEntry, ClassConstantEntry, ClassConstantFlags, ClassEntry, ClassEnumBackingType,
        ClassEnumCaseEntry, ClassFlags, ClassMethodEntry, ClassMethodFlags, ClassPropertyEntry,
        ClassPropertyFlags, ClassPropertyHooks, NativeDeclaredPropertySlot,
        NativeDynamicPropertyCell, NativeDynamicPropertySlots, ObjectRef, RuntimeType,
        display_class_name, normalize_class_name,
    };
    pub use crate::output::{OutputBuffer, OutputStats};
    #[cfg(feature = "full-runtime")]
    pub use crate::pcre::{
        PREG_BACKTRACK_LIMIT_ERROR, PREG_BAD_UTF8_ERROR, PREG_BAD_UTF8_OFFSET_ERROR,
        PREG_GREP_INVERT, PREG_INTERNAL_ERROR, PREG_JIT_STACKLIMIT_ERROR, PREG_NO_ERROR,
        PREG_OFFSET_CAPTURE, PREG_PATTERN_ORDER, PREG_RECURSION_LIMIT_ERROR, PREG_SET_ORDER,
        PREG_SPLIT_DELIM_CAPTURE, PREG_SPLIT_NO_EMPTY, PREG_SPLIT_OFFSET_CAPTURE,
        PREG_UNMATCHED_AS_NULL, PcreMatchLimits, preg_quote,
    };
    #[cfg(feature = "full-runtime")]
    pub use crate::phar::{PharArchive, PharEntry, PharError, PharUri};
    pub use crate::resource::{
        FilesystemCapabilities, ResourceId, ResourceKind, ResourceRef, ResourceTable, Stream,
        StreamFilterMode, StreamFlags, StreamMetadata, StreamOpenError, StreamOpenMode,
        StreamSeekWhence, StreamWrapperRegistry,
    };
    pub use crate::serialization::{
        SerializationError, UnserializeOptions, serialize, unserialize, unserialize_prefix,
    };
    pub use crate::session::{
        NativeSessionControlState, PHP_SESSION_ACTIVE, PHP_SESSION_DISABLED, PHP_SESSION_NONE,
        SessionState, native_session_name_is_valid,
    };
    pub use crate::source_span::RuntimeSourceSpan;
    pub use crate::status::{ExecutionStatus, ExitStatus};
    pub use crate::string::{PhpString, SymbolId};
    pub use crate::types::{runtime_type_name, value_matches_runtime_type, value_type_name};
    pub use crate::value::{FloatValue, Value};
    pub use crate::{
        datetime::normalize_timezone_identifier,
        reference::{
            Lvalue, LvalueError, LvalueKind, ReferenceCell, ReferencePlaceholder, Slot, TempValue,
            ValueSlot,
        },
        request_state::{
            ErasedExtensionStateSlot, ExtensionStateLayout, ExtensionStateLayoutBuilder,
            ExtensionStateLayoutError, ExtensionStateSlot, RequestState,
        },
    };
    #[cfg(feature = "full-runtime")]
    pub use crate::{
        sqlite::{
            SQLITE3_ASSOC, SQLITE3_BLOB, SQLITE3_BOTH, SQLITE3_DETERMINISTIC, SQLITE3_FLOAT,
            SQLITE3_INTEGER, SQLITE3_NULL, SQLITE3_NUM, SQLITE3_OPEN_CREATE, SQLITE3_OPEN_READONLY,
            SQLITE3_OPEN_READWRITE, SQLITE3_TEXT, SqliteState,
        },
        tokenizer::{TokenizerToken, native_tokenize_default_into, token_name_for_id, tokenize},
    };
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
    /// Runtime builtin fast paths coupled to VM optimization strategies.
    #[cfg(feature = "full-runtime")]
    pub mod builtin_intrinsics {
        pub use crate::builtins::{array_intrinsics, json_fast, string_intrinsics};
    }

    /// PCRE compiler/cache backend coupled to the current VM integration.
    #[cfg(any(feature = "full-runtime", feature = "pcre-core"))]
    pub mod pcre {
        pub use crate::pcre::*;
    }

    /// Runtime string interner instrumentation.
    pub mod string {
        pub use crate::string::symbol_interner_footprint;
    }

    #[doc(hidden)]
    pub use crate::debug::*;
    #[doc(hidden)]
    pub mod layout_stats {
        pub use crate::layout_stats::*;
    }
    #[doc(hidden)]
    pub mod numeric_string {
        pub use crate::numeric_string::*;
    }
    #[doc(hidden)]
    pub mod native_reference {
        pub use crate::reference::{
            NATIVE_REFERENCE_ARRAY_KEY_INT, NATIVE_REFERENCE_ARRAY_KEY_STRING,
            NATIVE_REFERENCE_ARRAY_VALUE_FALSE, NATIVE_REFERENCE_ARRAY_VALUE_INT,
            NATIVE_REFERENCE_ARRAY_VALUE_NULL, NATIVE_REFERENCE_ARRAY_VALUE_STRING,
            NATIVE_REFERENCE_ARRAY_VALUE_TRUE, NATIVE_REFERENCE_ARRAY_VALUE_UNINITIALIZED,
            NATIVE_REFERENCE_ARRAY_VALUE_UNSUPPORTED, NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION,
            NATIVE_REFERENCE_ARRAY_VIEW_EMPTY, NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED,
            NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION, NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_FALSE,
            NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_INT, NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_NULL,
            NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_TRUE,
            NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_UNINITIALIZED, NATIVE_REFERENCE_SCALAR_VIEW_EMPTY,
            NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED, NativeReferenceArrayEntry,
            NativeReferenceArrayView, NativeReferenceScalarView,
        };
    }
}

pub(crate) use crate::{
    reference::{Lvalue, LvalueKind, ReferenceCell, Slot, WeakReferenceHandle},
    request_state::{
        ExtensionStateLayout, ExtensionStateLayoutBuilder, ExtensionStateSlot, RequestState,
    },
};
pub(crate) use array::{ArrayKey, PackedArrayValues, PhpArray, WeakArrayHandle};
#[cfg(feature = "full-runtime")]
pub(crate) use builtins::{BuiltinError, FtpOptionValue};
pub(crate) use callable::{
    CallableMethodTarget, CallableValue, ClosureCaptureValue, ClosureDebugParameter, ClosurePayload,
};
pub(crate) use context::{
    RuntimeHttpResponseState, RuntimeIniOptions, RuntimeInputFilter, SessionIdGenerateCallback,
    SessionLoadCallback, UploadRegistry, parse_query_string_with_separators,
};
pub(crate) use convert::{
    NumericValue, compare, equal, identical, to_bool, to_float, to_int, to_number, to_string,
};
#[cfg(feature = "full-runtime")]
pub(crate) use db::mysql::{
    MYSQL_TEST_DSN_ENV, MYSQLI_ASSOC, MYSQLI_BOTH, MYSQLI_NUM, MYSQLI_REPORT_ERROR,
    MYSQLI_REPORT_OFF, MYSQLI_REPORT_STRICT, MYSQLI_SQLITE_COMPAT_ENV, MYSQLI_STORE_RESULT,
    MYSQLI_USE_RESULT, MYSQLND_CLIENT_INFO, MYSQLND_CLIENT_VERSION, MysqlConnectOptions,
    MysqlError, MysqlState,
};
#[cfg(feature = "full-runtime")]
pub(crate) use db::postgres::{
    PGSQL_ASSOC, PGSQL_BOTH, PGSQL_NUM, PostgresConnectOptions, PostgresError, PostgresField,
    PostgresState,
};
pub(crate) use diagnostic::{
    PhpReferenceClassification, RuntimeBringupDiagnosticContext, RuntimeDiagnostic,
    RuntimeDiagnosticPayload, RuntimeSeverity,
};
pub(crate) use error_output::{
    PHP_E_DEPRECATED, PHP_E_NOTICE, PHP_E_WARNING, PhpDiagnosticChannel,
    PhpDiagnosticDisplayOptions, PhpDiagnosticLocation, emit_php_diagnostic,
};
pub(crate) use fiber::FiberRef;
pub(crate) use generator::GeneratorRef;
pub(crate) use ini::IniRegistry;
pub(crate) use object::{
    ClassEntry, ClassFlags, ClassMethodEntry, ClassMethodFlags, ObjectRef, RuntimeType,
    WeakObjectHandle, display_class_name, normalize_class_name,
};
pub(crate) use output::OutputBuffer;
#[cfg(feature = "full-runtime")]
pub(crate) use pcre::PcreCache;
pub(crate) use resource::{
    FilesystemCapabilities, ResourceKind, ResourceRef, ResourceTable, StreamFilterMode,
    StreamFlags, StreamMetadata, StreamSeekWhence, StreamWrapperRegistry,
};
pub(crate) use serialization::{
    UnserializeOptions, serialize, serialize_with_precision, unserialize, unserialize_prefix,
};
pub(crate) use session::{PHP_SESSION_ACTIVE, PHP_SESSION_NONE, SessionState};
pub(crate) use source_span::RuntimeSourceSpan;
pub(crate) use string::PhpString;
pub(crate) use types::value_type_name;
pub(crate) use value::{FloatValue, Value};

#[cfg(test)]
mod tests {
    use crate::api::{CallableValue, ExecutionStatus, ExitStatus, OutputBuffer, PhpString, Value};

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
