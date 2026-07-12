//! Interpreter VM boundary.
//!
//! This crate owns compiled units, frames, registers, dispatch, calls, control
//! flow, exceptions, includes, tracing, and VM results. Downstream execution
//! code should import [`api::Vm`], [`api::VmOptions`], [`api::VmResult`], and
//! related execution types through [`api`]. Performance tooling and experiments should import
//! quickening, inline-cache, JIT, tiering, persistent-feedback, and deopt
//! surfaces through [`experimental`].
//!
//! ```
//! use php_vm::api::{Vm, VmOptions};
//! use php_vm::experimental::InlineCacheTable;
//!
//! let _ = Vm::with_options(VmOptions::default());
//! let _ = InlineCacheTable::default();
//! ```
//!
//! Internal implementation modules are not public API:
//!
//! ```compile_fail
//! use php_vm::inline_cache::InlineCacheTable;
//! ```
//!
//! ```compile_fail
//! use php_vm::vm::Vm;
//! ```
//!
// The interpreter crate forbids unsafe entirely; native-tier unsafe lives
// in php_jit behind its own safety audit.
#![deny(unsafe_code)]

#[doc(hidden)]
mod aliasing;
#[doc(hidden)]
mod bytecode;
#[doc(hidden)]
mod compiled_unit;
/// Copy-and-patch native tier bridge (behind the default-on `jit-copy-patch`
/// feature; runtime kill switch `PHRUST_JIT_COPY_PATCH=0`).
#[cfg(feature = "jit-copy-patch")]
mod copy_patch_bridge;
#[doc(hidden)]
mod counters;
#[doc(hidden)]
mod deopt;
#[doc(hidden)]
mod dependency_units;
#[doc(hidden)]
mod error;
#[doc(hidden)]
mod exit_policy;
#[doc(hidden)]
mod fallback;
#[doc(hidden)]
mod frame;
/// Audited unchecked frame-slot access (ADR 0021); the only interpreter
/// module exempt from the crate's `unsafe` denial.
pub(crate) mod frame_memory;
#[doc(hidden)]
mod include;
#[doc(hidden)]
mod inline_cache;
#[doc(hidden)]
mod last_use;
#[doc(hidden)]
mod literal_pool;
#[doc(hidden)]
mod osr;
#[doc(hidden)]
mod persistent_feedback;
#[doc(hidden)]
mod quickening;
#[doc(hidden)]
mod region_profile;
#[doc(hidden)]
#[cfg(test)]
mod std_builtins;
#[cfg(test)]
mod test_include_compiler;
#[doc(hidden)]
mod tiering;
#[doc(hidden)]
mod vm;

/// Stable VM execution surface.
///
/// This facade is intended for the executor, CLI, server integration, tests, and
/// other crates that need to compile or execute already-lowered units without
/// depending on frame internals, quickening tables, deopt metadata, or feedback
/// stores.
pub mod api {
    pub use crate::bytecode::BytecodeLayoutProfile;
    pub use crate::compiled_unit::{
        CompiledClass, CompiledUnit, CompiledUnitBuildError, CompiledUnitLayoutStats,
    };
    pub use crate::counters::{
        BoundaryProfile, JitCompileDescriptor, MethodCallProfile, OperationProfile,
        PropertyFetchProfile, VmCounters,
    };
    pub use crate::error::{VmError, VmErrorSeverity};
    pub use crate::include::{
        CacheInstanceId, CompiledInclude, ComposerFingerprintTransition, DeploymentRootFingerprint,
        DeploymentRootMode, IncludeCache, IncludeCacheStats, IncludeCompiler,
        IncludeCompilerFingerprint, IncludeDependency, IncludeDirectoryVersion, IncludeLoader,
        IncludePathFileFingerprint, LoadedInclude, ResolvedIncludePath,
        SERVER_INCLUDE_REVALIDATION_INTERVAL, ValidatedIncludeSource,
        composer_autoload_map_fingerprint, include_directory_version,
        include_revalidation_interval_from_env, negative_include_cache_enabled,
    };
    pub use crate::inline_cache::{FunctionCallSiteSnapshot, InlineCacheMode};
    pub use crate::persistent_feedback::PersistentFeedbackEpochs;
    pub use crate::quickening::{QuickeningMode, QuickeningSiteKey, QuickeningSiteSnapshot};
    pub use crate::tiering::{TieringOptions, TieringStats};
    pub use crate::vm::{
        BytecodeLayoutMode, DenseIncludeMode, DenseJumpThreadingMode, ExecutionFormat,
        JitBlacklistMode, JitMode, SuperinstructionMode, Vm, VmOptions, VmResult, VmWorkerState,
    };
}

/// Unstable VM instrumentation and optimization surface.
///
/// These exports are intentionally public for local performance gates,
/// benchmarks, and experimental tooling. They are not part of the stable
/// execution API.
pub mod experimental {
    #[doc(hidden)]
    pub use crate::aliasing::{
        AliasState, alias_transition_key, slot_alias_state, value_alias_state,
    };
    #[doc(hidden)]
    pub use crate::bytecode::{
        BytecodeLayoutProfile, BytecodeLayoutReport, DenseBytecodeUnit, DenseOpcode, DenseOperands,
        dense_block_key,
    };
    #[doc(hidden)]
    pub use crate::counters::{
        BoundaryProfile, JitCompileDescriptor, OperationProfile, VmCounters,
    };
    #[doc(hidden)]
    pub use crate::deopt::{
        ControlStateMarker, DeoptMetadata, DeoptMetadataError, DeoptPrecisionCounters,
        DeoptRegionMetadata, DeoptResumePoint, DeoptSideExitPoint, ExitId, GuardId, GuardKind,
        GuardRecord, GuardedTier, LiveIdentityMarker, LiveStateSnapshot, LiveValueClass,
        LiveValueSlot, MaterializedLiveState, MaterializedResumeRecord, ResumePoint, ResumeTable,
        ResumeTableError, SharedExit, SideExitPolicy, SnapshotEntry, SnapshotId, SnapshotRecord,
        SnapshotRejection, SnapshotStateFamily, VmDeoptReason,
    };
    #[doc(hidden)]
    pub use crate::dependency_units::{
        DependencyEdge, DependencyEdgeKind, DependencyGraph, DependencyPlannerInputs,
        DependencySpan, DependencyUnit, DependencyUnitId, DependencyUnitKind, DependencyUnitReport,
        FileFingerprint, InvalidationReason, ObservedIncludeTarget, ObservedLookup,
        plan_dependency_units, plan_dependency_units_with_inputs,
    };
    #[doc(hidden)]
    pub use crate::exit_policy::{
        ExitCounterKey, ExitCounterSite, ExitCounterTable, ExitPolicyDecision, ExitPolicyState,
        ExitPolicyThresholds, ExitSiteLocation,
    };
    #[doc(hidden)]
    pub use crate::fallback::{
        DEQUICKEN_AFTER_GUARD_MISSES, DISABLE_AFTER_GUARD_MISSES, FallbackProtocolEvent,
        FallbackProtocolStats,
    };
    #[doc(hidden)]
    pub use crate::frame::{CallStack, Frame, RegisterFile};
    #[doc(hidden)]
    pub use crate::inline_cache::FunctionCallSiteSnapshot;
    #[doc(hidden)]
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
    #[doc(hidden)]
    pub use crate::literal_pool::{InternedLiteral, LiteralPool};
    #[doc(hidden)]
    pub use crate::osr::{
        OsrEntry, OsrEntryId, OsrEntryMap, OsrEntryReport, OsrLiveSlot, OsrLoopStateAnnotations,
        OsrRefCowSafety, OsrTargetLocation, OsrUnsupportedStateKind, OsrValueClass, OsrVmSlot,
        analyze_dense_osr_entries, analyze_dense_osr_entries_with_annotations,
    };
    #[doc(hidden)]
    pub use crate::persistent_feedback::{
        PERSISTENT_FEEDBACK_FORMAT_VERSION, PERSISTENT_FEEDBACK_STATS_SCHEMA_VERSION,
        PersistentArrayKeyShape, PersistentArrayLayout, PersistentBranchBias,
        PersistentCallsiteState, PersistentFeedbackContext, PersistentFeedbackEntry,
        PersistentFeedbackEpochValidation, PersistentFeedbackEpochs, PersistentFeedbackKey,
        PersistentFeedbackLoadReport, PersistentFeedbackPayload, PersistentFeedbackStats,
        PersistentFeedbackStore, PersistentGuardFailureSummary, PersistentIncludeAutoloadStability,
        PersistentObjectShapeObservation, PersistentScalarKind,
    };
    #[doc(hidden)]
    pub use crate::quickening::{
        QuickeningMode, QuickeningObservation, QuickeningSiteKey, QuickeningSiteSnapshot,
        QuickeningSpecialization, QuickeningState, QuickeningTable,
    };
    #[doc(hidden)]
    pub use crate::region_profile::{
        BranchBias, BytecodeRange, PrivacyPolicy, RegionCandidate, RegionProfile, RegionTrace,
    };
    #[doc(hidden)]
    pub use crate::tiering::{ExecutionTier, TieringOptions, TieringState, TieringStats};
}

// Internal aliases keep implementation modules concise without widening the
// public crate surface. External callers use `api` or `experimental`.
#[doc(hidden)]
pub(crate) use counters::VmCounters;
#[doc(hidden)]
pub(crate) use deopt::{GuardKind, GuardedTier};
#[doc(hidden)]
pub(crate) use exit_policy::{ExitCounterKey, ExitCounterTable, ExitPolicyThresholds};
#[doc(hidden)]
pub(crate) use fallback::{
    DEQUICKEN_AFTER_GUARD_MISSES, DISABLE_AFTER_GUARD_MISSES, FallbackProtocolStats,
};
#[doc(hidden)]
pub(crate) use inline_cache::{InlineCacheKind, InlineCacheObservation};
#[doc(hidden)]
pub(crate) use quickening::{QuickeningMode, QuickeningObservation, QuickeningSpecialization};
#[doc(hidden)]
pub(crate) use vm::JitMode;
