//! Interpreter VM boundary.
//!
//! This crate will own compiled units, frames, registers, dispatch, calls,
//! control flow, exceptions, includes, tracing, and VM results. The current layer keeps
//! it as a compile-tested skeleton only.

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

pub use aliasing::{AliasState, alias_transition_key, slot_alias_state, value_alias_state};
pub use bytecode::{DenseBytecodeUnit, DenseOpcode};
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
