//! Local include/require resolution, caching, compilation port, and metadata.
//! Ownership flows from source/diagnostics through resolver/compiler and caches to the facade.
//! Frontend, lowering, and optimizer integration stays in `php_executor`.

mod cache;
mod cache_freshness;
mod compile_coordinator;
mod compiled_cache;
mod compiler;
mod diagnostics;
mod metadata;
mod metrics;
mod resolution_cache;
mod resolver;
mod source;

pub use cache::{
    CacheInstanceId, IncludeCache, SERVER_INCLUDE_REVALIDATION_INTERVAL,
    include_revalidation_interval_from_env,
};
pub use compiler::{CompiledInclude, IncludeCompiler, IncludeCompilerFingerprint};
pub use metadata::{
    ComposerFingerprintTransition, DeploymentRootFingerprint, DeploymentRootMode,
    composer_autoload_map_fingerprint,
};
pub use metrics::IncludeCacheStats;
pub use resolver::{
    CompilationDependencyRequest, CompilationDependencyResolver, IncludeLoader,
    LoadedCompilationDependency, ResolvedCompilationDependency, ResolvedIncludePath,
    negative_include_cache_enabled,
};
pub use source::{
    IncludeDependency, IncludeDirectoryVersion, IncludePathFileFingerprint, LoadedInclude,
    ValidatedIncludeSource, fnv1a_64, include_directory_version,
};
