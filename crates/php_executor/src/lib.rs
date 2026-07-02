//! Transport-independent PHP execution facade.
//!
//! `php_executor` is the canonical in-process compile/execute owner for the VM
//! CLI compatibility path and the integrated HTTP server. It owns source
//! analysis, IR lowering, optimization, VM invocation, request include-loader
//! construction, PHP diagnostic rendering, and the process-local compiled-script
//! cache used by the server.
//!
//! The crate intentionally does not own HTTP routing, CLI argument parsing, disk
//! bytecode artifact caching, or debug/report commands that need direct access
//! to frontend or VM internals.

mod cache;
mod diagnostics;
mod engine_compat;
mod executor;
mod input;
mod pipeline;
mod profile;
mod request;

pub use cache::{
    CompiledScriptCache, CompiledScriptCacheLookup, CompiledScriptCacheStats, PhpScriptCacheInput,
};
pub use diagnostics::{render_diagnostic_envelope, usage_diagnostic, write_diagnostic_envelope};
pub use engine_compat::{CliIniOptions, EngineInput, execute_php, read_script};
pub use executor::{CompiledPhpScript, PhpExecutor};
pub use input::{
    PhpCompileInput, PhpExecutionError, PhpExecutionInput, PhpExecutionOutput, PhpExecutionStatus,
    PhpExecutorOptions, PhpRequestExecutionInput,
};
pub use php_optimizer::OptimizationLevel;
pub use php_vm::api::{IncludeCache, IncludeCacheStats, IncludeLoader, VmOptions};
pub use pipeline::CompilePhaseTimings;
pub use profile::{EngineProfile, EngineProfileName, ParseEngineProfileError};
