//! Deterministic internal builtin registry for the runtime VM.

mod context;
mod error;
mod generated;
pub(in crate::builtins) mod modules;
mod registry;
mod request_state;
mod signatures;

#[doc(hidden)]
pub use crate::context::{NativeInputKey, NativeInputSegment};
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
pub use modules::bcmath::{
    native_bcadd, native_bccomp, native_bcdiv, native_bcmod, native_bcmul, native_bcpow,
    native_bcpowmod, native_bcsqrt, native_bcsub,
};
#[doc(hidden)]
pub use modules::core::{
    NativeCookieOptions, NativePrintfScalar, baseline_count_recursive_value,
    build_native_cookie_header_value, native_parse_intval_string_base, native_random_fill,
    visit_native_printf_scalars,
};
#[doc(hidden)]
pub use modules::curl::{CurlNetworkTestOverride, set_curl_network_tests_override_for_tests};
pub use modules::fileinfo::validate_fileinfo_options;
#[doc(hidden)]
pub use modules::filesystem::{
    NativeGlobPublished, NativeStatRecord, native_basename, native_chdir_target, native_chmod,
    native_directory_entries, native_dirname, native_disk_space, native_file_exists,
    native_file_get_contents, native_file_lines_into, native_file_put_contents, native_filegroup,
    native_filemtime, native_fileowner, native_fileperms, native_filesize, native_filetype,
    native_glob_into, native_is_dir, native_is_file, native_is_link, native_is_readable,
    native_is_writable, native_mkdir, native_pathinfo_into, native_realpath, native_rename,
    native_rmdir, native_scandir_into, native_stat, native_symlink, native_tempnam, native_tmpfile,
    native_touch, native_unlink,
};
#[doc(hidden)]
pub use modules::filter::{
    FILTER_DEFAULT, FILTER_FORCE_ARRAY, FILTER_NULL_ON_FAILURE, FILTER_REQUIRE_ARRAY,
    FILTER_REQUIRE_SCALAR, NativeFilterResult, NativeFilterValue, native_filter_id,
    native_filter_input_source_index, native_filter_names, native_filter_scalar,
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
    NativeStructuredValuePublisher, decode_native_json_associative_into, exact_json_decode,
    exact_json_encode, exact_json_last_error, exact_json_last_error_msg, exact_json_validate,
    validate_native_json,
};
pub use modules::json_fast::{
    NATIVE_JSON_DIRECT_ENCODE_FLAGS, NATIVE_JSON_FORCE_OBJECT, NATIVE_JSON_HEX_AMP,
    NATIVE_JSON_HEX_APOS, NATIVE_JSON_HEX_QUOT, NATIVE_JSON_HEX_TAG,
    NATIVE_JSON_INVALID_UTF8_IGNORE, NATIVE_JSON_INVALID_UTF8_SUBSTITUTE,
    NATIVE_JSON_NUMERIC_CHECK, NATIVE_JSON_PRESERVE_ZERO_FRACTION, NATIVE_JSON_PRETTY_PRINT,
    NATIVE_JSON_UNESCAPED_LINE_TERMINATORS, NATIVE_JSON_UNESCAPED_SLASHES,
    NATIVE_JSON_UNESCAPED_UNICODE, append_json_default_string, append_json_string_with_flags,
    visit_json_string_with_flags,
};
#[doc(hidden)]
pub use modules::math::native_number_format;
#[doc(hidden)]
pub use modules::math::{
    NativeBaseConversion, NativeParsedBaseNumber, native_base_conversion,
    native_decimal_base_conversion, native_parse_base_digits, native_round_f64,
};
#[doc(hidden)]
pub use modules::mbstring::{
    native_mb_canonical_encoding, native_mb_check_encoding, native_mb_chr, native_mb_convert_case,
    native_mb_convert_encoding, native_mb_convert_simple_case, native_mb_detect_encoding,
    native_mb_encoding_aliases, native_mb_encoding_names, native_mb_first_char_case, native_mb_ord,
    native_mb_position, native_mb_strcut, native_mb_strimwidth, native_mb_strlen,
    native_mb_strwidth, native_mb_substr, native_mb_substr_count,
};
#[doc(hidden)]
pub use modules::msgpack::{
    pack_value as msgpack_pack_value, unpack_value as msgpack_unpack_value,
};
#[doc(hidden)]
pub use modules::pcre::{
    NativePregCallbackPlanResult, NativePregCapturePublisher, NativePregPublishedMatch,
    NativePregPublishedMatchAll, NativePregReplaceResult, exact_preg_filter, exact_preg_grep,
    exact_preg_last_error, exact_preg_last_error_msg, exact_preg_match, exact_preg_match_all,
    exact_preg_quote, exact_preg_replace, exact_preg_split, native_preg_callback_plan_into,
    native_preg_grep_into, native_preg_match_all_into, native_preg_match_into,
    native_preg_replace_many_into, native_preg_replace_scalar, native_preg_split_into,
};
#[doc(hidden)]
pub use modules::soap::{
    SoapParsedBody, build_soap_envelope, load_wsdl, parse_soap_response, parse_wsdl, soap_http_post,
};
#[doc(hidden)]
pub use modules::sockets::{
    NativeNetworkAddress, native_inet_ntop, native_inet_pton, native_ip2long, native_long2ip,
};
#[doc(hidden)]
pub use modules::streams::{native_stream_is_local, native_stream_resolve_include_path};
#[doc(hidden)]
pub use modules::strings::{
    NATIVE_HTML_ESCAPE_DEFAULT_FLAGS, NATIVE_PHP_QUERY_RFC3986, NativePackArgument,
    NativeUnpackKey, NativeUnpackValue, exact_printf, exact_sprintf, exact_vprintf, exact_vsprintf,
    native_addcslashes_into, native_addcslashes_output_length, native_base64_decode_into,
    native_base64_decode_output_length, native_base64_encode_into,
    native_base64_encode_output_length, native_bin2hex_into, native_bin2hex_output_length,
    native_convert_uudecode_into, native_convert_uudecode_output_length,
    native_convert_uuencode_into, native_convert_uuencode_output_length, native_crc32,
    native_hash_hmac_into, native_hash_hmac_output_length, native_hash_into,
    native_hash_output_length, native_hex2bin_into, native_hex2bin_output_length,
    native_html_entity_decode_into, native_html_entity_decode_output_length,
    native_html_escape_into, native_html_escape_output_length, native_md5_into,
    native_md5_output_length, native_natural_compare, native_parse_str_into, native_parse_url_into,
    native_quoted_printable_decode_into, native_quoted_printable_decode_output_length,
    native_quotemeta_into, native_quotemeta_output_length, native_sha1_into,
    native_sha1_output_length, native_str_pad_into, native_str_pad_output_length,
    native_string_search_slice, native_stripcslashes_into, native_stripcslashes_output_length,
    native_stripslashes_into, native_stripslashes_output_length, native_strpbrk, native_strrchr,
    native_strtr_into, native_substr_compare, native_substr_replace_into,
    native_substr_replace_output_length, native_ucwords_into, native_unpack_hex_into,
    native_url_decode_into, native_url_decode_output_length, native_url_encode_into,
    native_url_encode_output_length, native_version_compare, native_version_operator_matches,
    visit_native_pack, visit_native_unpack,
};
#[doc(hidden)]
pub use modules::zlib::{
    NativeZlibDecodePlan, ZLIB_ENCODING_DEFLATE, ZLIB_ENCODING_GZIP, ZLIB_ENCODING_RAW,
    native_zlib_decode, native_zlib_decode_auto, native_zlib_encode_into,
    native_zlib_encode_output_capacity,
};
pub use modules::{array_intrinsics, json_fast, string_intrinsics};
pub use registry::{
    BuiltinCompatibility, BuiltinEntry, BuiltinExecutionKind, BuiltinHandlerKind, BuiltinRegistry,
};
pub use request_state::{BuiltinRequestState, GcRequestState, JsonRequestState, PcreRequestState};
pub use signatures::{BuiltinOutcome, BuiltinResult, InternalFunction};

pub fn hash_algorithm_exists(algorithm: &str) -> bool {
    modules::hash::hash_algorithm_exists(algorithm)
}
