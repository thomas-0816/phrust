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
//!
//! The `todo_runtime` module and `vm_skeleton_status()` export are historical
//! wiring-test compatibility markers. They are not the current VM architecture
//! and should not be used to infer execution coverage.

pub mod aliasing;
pub mod bytecode;
pub mod compiled_unit;
pub mod counters;
pub mod deopt;
pub mod fallback;
pub mod frame;
pub mod include;
pub mod inline_cache;
pub mod literal_pool;
pub mod persistent_feedback;
pub mod quickening;
pub mod region_profile;
pub mod std_builtins;
pub mod tiering;
pub mod todo_runtime;
pub mod vm;

/// Stable VM execution surface.
///
/// This facade is intended for the executor, CLI, server integration, tests, and
/// other crates that need to compile or execute already-lowered units without
/// depending on frame internals, quickening tables, deopt metadata, or feedback
/// stores.
pub mod api {
    pub use crate::compiled_unit::CompiledUnit;
    pub use crate::include::{
        IncludeLoader, IncludePathFileFingerprint, LoadedInclude, ResolvedIncludePath,
    };
    pub use crate::todo_runtime::{VmTodo, vm_skeleton_status};
    pub use crate::vm::{
        ExecutionFormat, JitBlacklistMode, JitMode, SuperinstructionMode, Vm, VmOptions, VmResult,
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
    pub use crate::bytecode::{DenseBytecodeUnit, DenseOpcode, DenseOperands};
    #[doc(hidden)]
    pub use crate::counters::{JitCompileDescriptor, VmCounters};
    #[doc(hidden)]
    pub use crate::deopt::{
        ControlStateMarker, DeoptMetadata, DeoptMetadataError, DeoptRegionMetadata,
        DeoptResumePoint, DeoptSideExitPoint, LiveIdentityMarker, LiveStateSnapshot,
        LiveValueClass, LiveValueSlot, VmDeoptReason,
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
        InlineCacheId, InlineCacheKind, InlineCacheMode, InlineCacheObservation, InlineCacheSlot,
        InlineCacheState, InlineCacheStats, InlineCacheTable, InvalidationEpoch,
        MethodCallCacheTarget, PropertyFetchCacheTarget,
    };
    #[doc(hidden)]
    pub use crate::literal_pool::{InternedLiteral, LiteralPool};
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
        QuickeningMode, QuickeningObservation, QuickeningSpecialization, QuickeningState,
        QuickeningTable,
    };
    #[doc(hidden)]
    pub use crate::region_profile::{
        BranchBias, BytecodeRange, PrivacyPolicy, RegionCandidate, RegionProfile, RegionTrace,
    };
    #[doc(hidden)]
    pub use crate::tiering::{ExecutionTier, TieringOptions, TieringState, TieringStats};
}

pub use aliasing::{AliasState, alias_transition_key, slot_alias_state, value_alias_state};
pub use bytecode::{DenseBytecodeUnit, DenseOpcode, DenseOperands};
pub use compiled_unit::CompiledUnit;
pub use counters::{JitCompileDescriptor, VmCounters};
pub use deopt::{
    ControlStateMarker, DeoptMetadata, DeoptMetadataError, DeoptRegionMetadata, DeoptResumePoint,
    DeoptSideExitPoint, LiveIdentityMarker, LiveStateSnapshot, LiveValueClass, LiveValueSlot,
    VmDeoptReason,
};
pub use fallback::{
    DEQUICKEN_AFTER_GUARD_MISSES, DISABLE_AFTER_GUARD_MISSES, FallbackProtocolEvent,
    FallbackProtocolStats,
};
pub use frame::{CallStack, Frame, RegisterFile};
pub use include::{IncludeLoader, IncludePathFileFingerprint, LoadedInclude, ResolvedIncludePath};
pub use inline_cache::{
    ClassConstantStaticPropertyCacheKind, ClassConstantStaticPropertyCacheTarget, InlineCacheId,
    InlineCacheKind, InlineCacheMode, InlineCacheObservation, InlineCacheSlot, InlineCacheState,
    InlineCacheStats, InlineCacheTable, InvalidationEpoch, MethodCallCacheTarget,
    PropertyFetchCacheTarget,
};
pub use literal_pool::{InternedLiteral, LiteralPool};
pub use persistent_feedback::{
    PERSISTENT_FEEDBACK_FORMAT_VERSION, PERSISTENT_FEEDBACK_STATS_SCHEMA_VERSION,
    PersistentArrayKeyShape, PersistentArrayLayout, PersistentBranchBias, PersistentCallsiteState,
    PersistentFeedbackContext, PersistentFeedbackEntry, PersistentFeedbackEpochs,
    PersistentFeedbackKey, PersistentFeedbackLoadReport, PersistentFeedbackPayload,
    PersistentFeedbackStats, PersistentFeedbackStore, PersistentGuardFailureSummary,
    PersistentIncludeAutoloadStability, PersistentObjectShapeObservation, PersistentScalarKind,
};
pub use quickening::{
    QuickeningMode, QuickeningObservation, QuickeningSpecialization, QuickeningState,
    QuickeningTable,
};
pub use region_profile::{
    BranchBias, BytecodeRange, PrivacyPolicy, RegionCandidate, RegionProfile, RegionTrace,
};
pub use tiering::{ExecutionTier, TieringOptions, TieringState, TieringStats};
pub use todo_runtime::{VmTodo, vm_skeleton_status};
pub use vm::{
    ExecutionFormat, JitBlacklistMode, JitMode, SuperinstructionMode, Vm, VmOptions, VmResult,
};

#[cfg(test)]
mod tests {
    use super::{VmTodo, vm_skeleton_status};

    #[test]
    fn exposes_vm_skeleton() {
        let todo = VmTodo::new("compiled units, frames, registers, and dispatch");
        assert_eq!(
            todo.area(),
            "compiled units, frames, registers, and dispatch"
        );
        assert_eq!(vm_skeleton_status(), "vm-skeleton");
        assert_eq!(php_ir::ir_skeleton_status(), "ir-core-model");
        assert_eq!(php_runtime::runtime_skeleton_status(), "runtime-skeleton");
        assert_eq!(
            php_testkit::reference_checkout_path(),
            "third_party/php-src"
        );
    }
}
