//! Deterministic internal builtin registry for the runtime VM.

mod context;
mod error;
pub(in crate::builtins) mod modules;
mod registry;
mod signatures;

pub use context::{
    ApcuState, BuiltinContext, FilesystemRuntimeState, FtpOptionValue, FtpState, GettextState,
    IconvEncodingState, ImapState, JSON_ERROR_RECURSION, JSON_PARTIAL_OUTPUT_ON_ERROR,
    JSON_THROW_ON_ERROR, LdapState, OpcacheState, OpenSslErrorState, PcntlState, ReadlineState,
    RuntimeSourceSpan, ShmopState, SoapState, SocketState, Ssh2State, StreamContextState,
    StrtokState, SysvMessageQueueState, SysvSemaphoreState, SysvSharedMemoryState,
};
pub use error::{BuiltinError, BuiltinErrorContext};
#[doc(hidden)]
#[cfg(not(target_family = "wasm"))]
pub use modules::curl::{CurlNetworkTestOverride, set_curl_network_tests_override_for_tests};
pub use modules::{array_intrinsics, json_fast, string_intrinsics};
pub use registry::{BuiltinCompatibility, BuiltinEntry, BuiltinRegistry};
pub use signatures::{BuiltinResult, InternalFunction};
