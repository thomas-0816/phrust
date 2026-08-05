//! Shared SSA compilation policy for Generic and Optimizing native tiers.
//!
//! Both tiers use the same function-scoped value placement. Generic remains
//! conservative in its semantic choices and imported exact leaves, while
//! Optimizing may use publication assumptions and guards.

use super::CraneliftLoweringError;
use super::executable_region::DefinedRegionFunction;
use crate::region_ir::NativeCompilerTier;

type FragmentEmitter<'a> =
    dyn FnMut(NativeCompilationMode) -> Result<DefinedRegionFunction, CraneliftLoweringError> + 'a;

/// Stable compiler mode included in diagnostics and persistent identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativeCompilationMode {
    SsaGeneric,
    SsaOptimizing,
}

impl NativeCompilationMode {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::SsaGeneric => "ssa-generic",
            Self::SsaOptimizing => "ssa-optimizing",
        }
    }

    pub(super) const fn specialization(self) -> &'static str {
        match self {
            Self::SsaGeneric => super::native_linkage::GENERIC_FUNCTION_SPECIALIZATION,
            Self::SsaOptimizing => "ssa-optimizing-v1",
        }
    }

    pub(super) const fn streams_cfg_state_through_slots(self) -> bool {
        false
    }

    pub(super) const fn is_generic(self) -> bool {
        matches!(self, Self::SsaGeneric)
    }
}

/// Common fragment compiler contract. The optimizing implementation remains
/// an interface on this branch; the parallel hot-native work owns its policy.
pub(super) trait NativeFragmentCompiler {
    fn mode(&self) -> NativeCompilationMode;

    fn compile_fragment(
        &self,
        emit: &mut FragmentEmitter<'_>,
    ) -> Result<DefinedRegionFunction, CraneliftLoweringError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SsaGenericCompiler;

impl NativeFragmentCompiler for SsaGenericCompiler {
    fn mode(&self) -> NativeCompilationMode {
        NativeCompilationMode::SsaGeneric
    }

    fn compile_fragment(
        &self,
        emit: &mut FragmentEmitter<'_>,
    ) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
        emit(NativeCompilationMode::SsaGeneric)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SsaOptimizingCompiler;

impl NativeFragmentCompiler for SsaOptimizingCompiler {
    fn mode(&self) -> NativeCompilationMode {
        NativeCompilationMode::SsaOptimizing
    }

    fn compile_fragment(
        &self,
        emit: &mut FragmentEmitter<'_>,
    ) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
        emit(NativeCompilationMode::SsaOptimizing)
    }
}

static SSA_GENERIC_COMPILER: SsaGenericCompiler = SsaGenericCompiler;
static SSA_OPTIMIZING_COMPILER: SsaOptimizingCompiler = SsaOptimizingCompiler;

pub(super) fn compiler_for_tier(tier: NativeCompilerTier) -> &'static dyn NativeFragmentCompiler {
    match tier {
        NativeCompilerTier::Generic => &SSA_GENERIC_COMPILER,
        NativeCompilerTier::Optimizing => &SSA_OPTIMIZING_COMPILER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_select_distinct_stable_compilation_modes() {
        let generic = compiler_for_tier(NativeCompilerTier::Generic);
        let optimizing = compiler_for_tier(NativeCompilerTier::Optimizing);
        assert_eq!(generic.mode().as_str(), "ssa-generic");
        assert_eq!(optimizing.mode().as_str(), "ssa-optimizing");
        assert_ne!(
            generic.mode().specialization(),
            optimizing.mode().specialization()
        );
    }
}
