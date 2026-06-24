//! Phase 4 interpreter VM boundary.
//!
//! This crate will own compiled units, frames, registers, dispatch, calls,
//! control flow, exceptions, includes, tracing, and VM results. Prompt 01 keeps
//! it as a compile-tested skeleton only.

pub mod compiled_unit;
pub mod counters;
pub mod fallback;
pub mod frame;
pub mod include;
pub mod inline_cache;
pub mod literal_pool;
pub mod quickening;
pub mod std_builtins;
pub mod tiering;
pub mod todo_phase4;
pub mod vm;

pub use compiled_unit::CompiledUnit;
pub use counters::{JitCompileDescriptor, VmCounters};
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
pub use quickening::{
    QuickeningMode, QuickeningObservation, QuickeningSpecialization, QuickeningState,
    QuickeningTable,
};
pub use tiering::{ExecutionTier, TieringOptions, TieringState, TieringStats};
pub use todo_phase4::{Phase4VmTodo, vm_skeleton_status};
pub use vm::{JitBlacklistMode, JitMode, Vm, VmOptions, VmResult};

#[cfg(test)]
mod tests {
    use super::{Phase4VmTodo, vm_skeleton_status};

    #[test]
    fn exposes_prompt01_vm_skeleton() {
        let todo = Phase4VmTodo::new("compiled units, frames, registers, and dispatch");
        assert_eq!(
            todo.area(),
            "compiled units, frames, registers, and dispatch"
        );
        assert_eq!(vm_skeleton_status(), "phase4-vm-skeleton");
        assert_eq!(php_ir::ir_skeleton_status(), "phase4-ir-core-model");
        assert_eq!(
            php_runtime::runtime_skeleton_status(),
            "phase4-runtime-skeleton"
        );
        assert_eq!(
            php_testkit::reference_checkout_path(),
            "third_party/php-src"
        );
    }
}
