//! Interpreter VM boundary.
//!
//! This crate owns compiled units, frames, registers, dispatch, calls, control
//! flow, exceptions, includes, tracing, and VM results. Downstream execution
//! code should import [`Vm`], [`VmOptions`], [`VmResult`], and related execution
//! types through [`api`]. Performance tooling and experiments should import
//! quickening, inline-cache, JIT, tiering, persistent-feedback, and deopt
//! surfaces through [`experimental`].
//!
//! Root re-exports remain as compatibility aliases while local crates migrate to
//! the explicit facades.

#[doc(hidden)]
pub mod aliasing;
#[doc(hidden)]
pub mod bytecode;
#[doc(hidden)]
pub mod compiled_unit;
#[doc(hidden)]
pub mod counters;
#[doc(hidden)]
pub mod deopt;
#[doc(hidden)]
pub mod dependency_units;
#[doc(hidden)]
pub mod error;
#[doc(hidden)]
pub mod exit_policy;
#[doc(hidden)]
pub mod fallback;
#[doc(hidden)]
pub mod frame;
#[doc(hidden)]
pub mod include;
#[doc(hidden)]
pub mod inline_cache;
#[doc(hidden)]
pub mod literal_pool;
#[doc(hidden)]
pub mod osr;
#[doc(hidden)]
pub mod persistent_feedback;
#[doc(hidden)]
pub mod quickening;
#[doc(hidden)]
pub mod region_profile;
#[doc(hidden)]
pub mod std_builtins;
#[doc(hidden)]
pub mod tiering;
#[doc(hidden)]
pub mod vm;

/// Stable VM execution surface.
///
/// This facade is intended for the executor, CLI, server integration, tests, and
/// other crates that need to compile or execute already-lowered units without
/// depending on frame internals, quickening tables, deopt metadata, or feedback
/// stores.
pub mod api {
    pub use crate::bytecode::BytecodeLayoutProfile;
    pub use crate::compiled_unit::CompiledUnit;
    pub use crate::counters::{
        JitCompileDescriptor, MethodCallProfile, PropertyFetchProfile, VmCounters,
    };
    pub use crate::error::{VmError, VmErrorSeverity};
    pub use crate::include::{
        IncludeCache, IncludeCacheStats, IncludeLoader, IncludePathFileFingerprint, LoadedInclude,
        ResolvedIncludePath,
    };
    pub use crate::inline_cache::InlineCacheMode;
    pub use crate::quickening::{QuickeningMode, QuickeningSiteKey, QuickeningSiteSnapshot};
    pub use crate::tiering::{TieringOptions, TieringStats};
    pub use crate::vm::{
        BytecodeLayoutMode, DenseIncludeMode, DenseJumpThreadingMode, ExecutionFormat,
        JitBlacklistMode, JitMode, SuperinstructionMode, Vm, VmOptions, VmResult,
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
    pub use crate::counters::{JitCompileDescriptor, VmCounters};
    #[doc(hidden)]
    pub use crate::deopt::{
        ControlStateMarker, DeoptMetadata, DeoptMetadataError, DeoptRegionMetadata,
        DeoptResumePoint, DeoptSideExitPoint, ExitId, GuardId, GuardKind, GuardRecord, GuardedTier,
        LiveIdentityMarker, LiveStateSnapshot, LiveValueClass, LiveValueSlot,
        MaterializedLiveState, MaterializedResumeRecord, ResumePoint, ResumeTable,
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
    pub use crate::inline_cache::{
        ClassConstantStaticPropertyCacheKind, ClassConstantStaticPropertyCacheTarget,
        ClassRelationCache, ClassRelationCacheEntry, ClassRelationCacheKey,
        ClassRelationCacheLookup, ClassRelationCacheTarget, ClassRelationEpochs, ClassRelationKind,
        InlineCacheId, InlineCacheKind, InlineCacheMode, InlineCacheObservation, InlineCacheSlot,
        InlineCacheState, InlineCacheStats, InlineCacheTable, InvalidationEpoch,
        MethodCallCacheTarget, PropertyFetchCacheTarget,
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
        PersistentFeedbackEpochs, PersistentFeedbackKey, PersistentFeedbackLoadReport,
        PersistentFeedbackPayload, PersistentFeedbackStats, PersistentFeedbackStore,
        PersistentGuardFailureSummary, PersistentIncludeAutoloadStability,
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

// Compatibility aliases for older local consumers. New code should import from
// `php_vm::api` for execution or `php_vm::experimental` for optimization and
// instrumentation internals.
#[doc(hidden)]
pub use aliasing::{AliasState, alias_transition_key, slot_alias_state, value_alias_state};
#[doc(hidden)]
pub use bytecode::{
    BytecodeLayoutProfile, BytecodeLayoutReport, DenseBytecodeUnit, DenseOpcode, DenseOperands,
    dense_block_key,
};
#[doc(hidden)]
pub use compiled_unit::CompiledUnit;
#[doc(hidden)]
pub use counters::{JitCompileDescriptor, VmCounters};
#[doc(hidden)]
pub use deopt::{
    ControlStateMarker, DeoptMetadata, DeoptMetadataError, DeoptRegionMetadata, DeoptResumePoint,
    DeoptSideExitPoint, ExitId, GuardId, GuardKind, GuardRecord, GuardedTier, LiveIdentityMarker,
    LiveStateSnapshot, LiveValueClass, LiveValueSlot, MaterializedLiveState,
    MaterializedResumeRecord, ResumePoint, ResumeTable, ResumeTableError, SharedExit,
    SideExitPolicy, SnapshotEntry, SnapshotId, SnapshotRecord, SnapshotRejection,
    SnapshotStateFamily, VmDeoptReason,
};
#[doc(hidden)]
pub use error::{VmError, VmErrorSeverity};
#[doc(hidden)]
pub use exit_policy::{
    ExitCounterKey, ExitCounterSite, ExitCounterTable, ExitPolicyDecision, ExitPolicyState,
    ExitPolicyThresholds, ExitSiteLocation,
};
#[doc(hidden)]
pub use fallback::{
    DEQUICKEN_AFTER_GUARD_MISSES, DISABLE_AFTER_GUARD_MISSES, FallbackProtocolEvent,
    FallbackProtocolStats,
};
#[doc(hidden)]
pub use frame::{CallStack, Frame, RegisterFile};
#[doc(hidden)]
pub use include::{
    IncludeCache, IncludeCacheStats, IncludeLoader, IncludePathFileFingerprint, LoadedInclude,
    ResolvedIncludePath,
};
#[doc(hidden)]
pub use inline_cache::{
    ClassConstantStaticPropertyCacheKind, ClassConstantStaticPropertyCacheTarget,
    ClassRelationCache, ClassRelationCacheEntry, ClassRelationCacheKey, ClassRelationCacheLookup,
    ClassRelationCacheTarget, ClassRelationEpochs, ClassRelationKind, InlineCacheId,
    InlineCacheKind, InlineCacheMode, InlineCacheObservation, InlineCacheSlot, InlineCacheState,
    InlineCacheStats, InlineCacheTable, InvalidationEpoch, MethodCallCacheTarget,
    PropertyFetchCacheTarget,
};
#[doc(hidden)]
pub use literal_pool::{InternedLiteral, LiteralPool};
#[doc(hidden)]
pub use osr::{
    OsrEntry, OsrEntryId, OsrEntryMap, OsrEntryReport, OsrLiveSlot, OsrLoopStateAnnotations,
    OsrRefCowSafety, OsrTargetLocation, OsrUnsupportedStateKind, OsrValueClass, OsrVmSlot,
    analyze_dense_osr_entries, analyze_dense_osr_entries_with_annotations,
};
#[doc(hidden)]
pub use persistent_feedback::{
    PERSISTENT_FEEDBACK_FORMAT_VERSION, PERSISTENT_FEEDBACK_STATS_SCHEMA_VERSION,
    PersistentArrayKeyShape, PersistentArrayLayout, PersistentBranchBias, PersistentCallsiteState,
    PersistentFeedbackContext, PersistentFeedbackEntry, PersistentFeedbackEpochs,
    PersistentFeedbackKey, PersistentFeedbackLoadReport, PersistentFeedbackPayload,
    PersistentFeedbackStats, PersistentFeedbackStore, PersistentGuardFailureSummary,
    PersistentIncludeAutoloadStability, PersistentObjectShapeObservation, PersistentScalarKind,
};
#[doc(hidden)]
pub use quickening::{
    QuickeningMode, QuickeningObservation, QuickeningSiteKey, QuickeningSiteSnapshot,
    QuickeningSpecialization, QuickeningState, QuickeningTable,
};
#[doc(hidden)]
pub use region_profile::{
    BranchBias, BytecodeRange, PrivacyPolicy, RegionCandidate, RegionProfile, RegionTrace,
};
#[doc(hidden)]
pub use tiering::{ExecutionTier, TieringOptions, TieringState, TieringStats};
#[doc(hidden)]
pub use vm::{
    BytecodeLayoutMode, DenseIncludeMode, DenseJumpThreadingMode, ExecutionFormat,
    JitBlacklistMode, JitMode, SuperinstructionMode, Vm, VmOptions, VmResult,
};
