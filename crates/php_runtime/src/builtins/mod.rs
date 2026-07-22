//! Deterministic internal builtin registry for the runtime VM.

mod context;
mod error;
mod generated;
pub(in crate::builtins) mod modules;
mod registry;
mod request_state;
mod signatures;

pub use crate::source_span::RuntimeSourceSpan;
pub use context::{
    ApcuState, BuiltinContext, CurlState, FilesystemRuntimeState, FtpOptionValue, FtpState,
    GettextState, IconvEncodingState, ImapConnectionConfig, ImapMailboxSnapshot, ImapState,
    JSON_ERROR_RECURSION, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_THROW_ON_ERROR, LdapSearchScope,
    LdapState, MbSubstituteCharacter, OpcacheState, OpenSslErrorState, PcntlState, ReadlineState,
    SYSVMSG_EAGAIN, SYSVMSG_EINVAL, SYSVMSG_IPC_NOWAIT, ShmopState, SoapState, SocketState,
    Ssh2FingerprintHash, Ssh2State, StreamContextState, StrtokState, SysvMessageQueueState,
    SysvSemaphoreError, SysvSemaphoreState, SysvSharedMemoryState,
};
pub(in crate::builtins) use context::{
    CurlEasyCollector, CurlMultiDone, CurlMultiRuntimeState, CurlMultiTransferState,
};
pub use error::{BuiltinError, BuiltinErrorContext};
#[doc(hidden)]
pub use modules::core::{NativePrintfScalar, format_native_printf_scalars};
#[doc(hidden)]
pub use modules::curl::{CurlNetworkTestOverride, set_curl_network_tests_override_for_tests};
pub use modules::fileinfo::validate_fileinfo_options;
#[doc(hidden)]
pub use modules::filesystem::{
    native_basename, native_dirname, native_file_exists, native_realpath,
};
#[doc(hidden)]
pub use modules::igbinary::{
    serialize_value as igbinary_serialize_value, unserialize_value as igbinary_unserialize_value,
};
pub use modules::intl::{
    NORMALIZER_FORM_C, NORMALIZER_FORM_D, NORMALIZER_FORM_KC, NORMALIZER_FORM_KD,
    is_normalized_string, normalize_string,
};
#[doc(hidden)]
pub use modules::json::{
    NativeJsonDecodedValue, decode_native_json_associative, exact_json_decode, exact_json_encode,
    exact_json_last_error, exact_json_last_error_msg, exact_json_validate, validate_native_json,
};
pub use modules::json_fast::append_json_default_string;
#[doc(hidden)]
pub use modules::msgpack::{
    pack_value as msgpack_pack_value, unpack_value as msgpack_unpack_value,
};
#[doc(hidden)]
pub use modules::pcre::{
    NativePregMatchAllResult, NativePregMatchResult, NativePregReplaceManyResult,
    NativePregReplaceResult, exact_preg_filter, exact_preg_grep, exact_preg_last_error,
    exact_preg_last_error_msg, exact_preg_match, exact_preg_match_all, exact_preg_quote,
    exact_preg_replace, exact_preg_split, native_preg_grep, native_preg_match,
    native_preg_match_all, native_preg_replace_many, native_preg_replace_scalar, native_preg_split,
};
#[doc(hidden)]
pub use modules::soap::{
    SoapParsedBody, build_soap_envelope, load_wsdl, parse_soap_response, parse_wsdl, soap_http_post,
};
#[doc(hidden)]
pub use modules::strings::{exact_printf, exact_sprintf, exact_vprintf, exact_vsprintf};
pub use modules::{array_intrinsics, json_fast, string_intrinsics};
pub use registry::{
    BuiltinCompatibility, BuiltinEntry, BuiltinExecutionKind, BuiltinHandlerKind, BuiltinRegistry,
};
pub use request_state::{BuiltinRequestState, JsonRequestState, PcreRequestState};
pub use signatures::{BuiltinOutcome, BuiltinResult, InternalFunction};

pub fn hash_algorithm_exists(algorithm: &str) -> bool {
    modules::hash::hash_algorithm_exists(algorithm)
}
