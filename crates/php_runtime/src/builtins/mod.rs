//! Deterministic internal builtin registry for the runtime VM.

mod context;
mod error;
pub(in crate::builtins) mod modules;
mod registry;
mod signatures;

pub use context::{
    ApcuState, BuiltinContext, FilesystemRuntimeState, IconvEncodingState, RuntimeSourceSpan,
    StreamContextState, StrtokState,
};
pub use error::{BuiltinError, BuiltinErrorContext};
#[doc(hidden)]
pub use modules::curl::{CurlNetworkTestOverride, set_curl_network_tests_override_for_tests};
pub use registry::{BuiltinCompatibility, BuiltinEntry, BuiltinRegistry};
pub use signatures::{BuiltinResult, InternalFunction};
