//! Native PHP execution coordinator.
//!
//! This crate owns outer request state, mandatory Cranelift compilation,
//! native function publication, runtime helpers, caches, diagnostics, and
//! result assembly. It contains no opcode execution loop.
#![deny(unsafe_code)]

#[doc(hidden)]
mod compiled_unit;
#[doc(hidden)]
mod counters;
#[doc(hidden)]
mod dependency_units;
#[doc(hidden)]
mod error;
#[doc(hidden)]
mod include;
#[doc(hidden)]
mod inline_cache;
#[doc(hidden)]
mod tiering;
#[doc(hidden)]
mod vm;

/// Stable native execution surface.
pub mod api {
    pub use crate::compiled_unit::{
        CompiledClass, CompiledUnit, CompiledUnitBuildError, CompiledUnitLayoutStats,
    };
    pub use crate::counters::{
        BoundaryProfile, BoundaryWorkSnapshot, JitCompileDescriptor, MethodCallProfile,
        OperationProfile, PropertyFetchProfile, VmCounters,
    };
    pub use crate::error::{VmError, VmErrorSeverity};
    pub use crate::include::{
        CacheInstanceId, CompilationDependencyRequest, CompilationDependencyResolver,
        CompiledInclude, ComposerFingerprintTransition, DeploymentRootFingerprint,
        DeploymentRootMode, IncludeCache, IncludeCacheStats, IncludeCompiler,
        IncludeCompilerFingerprint, IncludeDependency, IncludeDirectoryVersion, IncludeLoader,
        IncludePathFileFingerprint, LoadedCompilationDependency, LoadedInclude,
        ResolvedCompilationDependency, ResolvedIncludePath, SERVER_INCLUDE_REVALIDATION_INTERVAL,
        ValidatedIncludeSource, composer_autoload_map_fingerprint, include_directory_version,
        include_revalidation_interval_from_env, negative_include_cache_enabled,
    };
    pub use crate::inline_cache::{FunctionCallSiteSnapshot, InlineCacheMode};
    pub use crate::tiering::{TieringOptions, TieringStats};
    pub use crate::vm::{
        JitBlacklistMode, NativeOptimizationPolicy, Vm, VmOptions, VmResult, VmWorkerState,
    };
}

/// Instrumentation and native optimization metadata used by tooling.
pub mod experimental {
    #[must_use]
    pub fn cranelift_code_cache_generation() -> u64 {
        php_jit::cranelift_code_manager_stats().code_generations as u64
    }

    pub use crate::dependency_units::{
        DependencyEdge, DependencyEdgeKind, DependencyGraph, DependencyPlannerInputs,
        DependencySpan, DependencyUnit, DependencyUnitId, DependencyUnitKind, DependencyUnitReport,
        FileFingerprint, InvalidationReason, ObservedIncludeTarget, ObservedLookup,
        plan_dependency_units, plan_dependency_units_with_inputs,
    };
    pub use crate::inline_cache::{
        CallReferenceMask, ClassConstantStaticPropertyCacheKind,
        ClassConstantStaticPropertyCacheTarget, ClassRelationCache, ClassRelationCacheEntry,
        ClassRelationCacheKey, ClassRelationCacheLookup, ClassRelationCacheTarget,
        ClassRelationEpochs, ClassRelationKind, FunctionCallCacheTarget, FunctionCallShape,
        InlineCacheId, InlineCacheKind, InlineCacheMode, InlineCacheObservation, InlineCacheSlot,
        InlineCacheState, InlineCacheStats, InlineCacheTable, InvalidationEpoch,
        MethodCallCacheTarget, MethodCallGuardMetadata, MethodCallResolvedTarget, MethodCallShape,
        PropertyAssignCacheTarget, PropertyAssignLayoutMetadata, PropertyAssignResolvedTarget,
        PropertyFetchCacheTarget, PropertyFetchLayoutMetadata, PropertyFetchResolvedTarget,
    };
    pub use crate::tiering::{ExecutionTier, TieringOptions, TieringStats};
}
