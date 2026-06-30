use php_optimizer::OptimizationLevel;
use php_vm::api::{
    BytecodeLayoutMode, ExecutionFormat, InlineCacheMode, JitBlacklistMode, JitMode,
    QuickeningMode, SuperinstructionMode, TieringOptions,
};
use std::{fmt, str::FromStr};

use crate::PhpExecutorOptions;

/// Canonical product engine profile shared by CLI and server entry points.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EngineProfileName {
    /// Compatibility and semantic-debug mode: optimized frontend and adaptive VM
    /// features are disabled.
    Baseline,
    /// Safe fast product default: optimizer, interpreter fast paths, and the
    /// guarded native tier are enabled when backend support is available.
    #[default]
    Default,
    /// Developer profile that keeps the native-oriented settings explicit.
    ExperimentalJit,
}

impl EngineProfileName {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Default => "default",
            Self::ExperimentalJit => "experimental-jit",
        }
    }

    #[must_use]
    pub const fn accepted_values() -> &'static str {
        "baseline, default, fast, or experimental-jit"
    }

    pub fn parse(value: &str) -> Result<Self, ParseEngineProfileError> {
        value.parse()
    }
}

impl fmt::Display for EngineProfileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EngineProfileName {
    type Err = ParseEngineProfileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "baseline" => Ok(Self::Baseline),
            "default" | "fast" => Ok(Self::Default),
            "experimental-jit" => Ok(Self::ExperimentalJit),
            _ => Err(ParseEngineProfileError {
                value: value.to_string(),
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseEngineProfileError {
    value: String,
}

impl fmt::Display for ParseEngineProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported engine preset `{}`; expected {}",
            self.value,
            EngineProfileName::accepted_values()
        )
    }
}

impl std::error::Error for ParseEngineProfileError {}

/// Resolved compile and VM options for one profile.
#[derive(Clone, Debug)]
pub struct EngineProfile {
    pub name: EngineProfileName,
    pub optimization_level: OptimizationLevel,
    pub vm_options: php_vm::api::VmOptions,
}

impl EngineProfile {
    #[must_use]
    pub fn new(name: EngineProfileName) -> Self {
        let mut vm_options = php_vm::api::VmOptions::default();
        let optimization_level = match name {
            EngineProfileName::Baseline => {
                vm_options.execution_format = ExecutionFormat::Ir;
                vm_options.superinstructions = SuperinstructionMode::Off;
                vm_options.bytecode_layout = BytecodeLayoutMode::Source;
                vm_options.quickening = QuickeningMode::Off;
                vm_options.inline_caches = InlineCacheMode::Off;
                vm_options.jit = JitMode::Off;
                vm_options.jit_blacklist = JitBlacklistMode::On;
                vm_options.tiering.enabled = false;
                vm_options.jit_threshold = vm_options.tiering.function_entry_threshold;
                OptimizationLevel::O0
            }
            EngineProfileName::Default => {
                vm_options.execution_format = ExecutionFormat::Auto;
                vm_options.superinstructions = SuperinstructionMode::On;
                vm_options.bytecode_layout = BytecodeLayoutMode::Source;
                vm_options.quickening = QuickeningMode::On;
                vm_options.inline_caches = InlineCacheMode::On;
                vm_options.jit = JitMode::Cranelift;
                vm_options.jit_blacklist = JitBlacklistMode::On;
                vm_options.tiering = TieringOptions::default();
                vm_options.jit_threshold = vm_options.tiering.function_entry_threshold;
                OptimizationLevel::O2
            }
            EngineProfileName::ExperimentalJit => {
                vm_options.execution_format = ExecutionFormat::Auto;
                vm_options.superinstructions = SuperinstructionMode::On;
                vm_options.bytecode_layout = BytecodeLayoutMode::Source;
                vm_options.quickening = QuickeningMode::On;
                vm_options.inline_caches = InlineCacheMode::On;
                vm_options.jit = JitMode::Cranelift;
                vm_options.jit_blacklist = JitBlacklistMode::On;
                vm_options.tiering = TieringOptions::default();
                vm_options.jit_threshold = vm_options.tiering.function_entry_threshold;
                OptimizationLevel::O2
            }
        };
        vm_options.include_optimization_level = optimization_level;
        Self {
            name,
            optimization_level,
            vm_options,
        }
    }
}

impl From<EngineProfileName> for EngineProfile {
    fn from(name: EngineProfileName) -> Self {
        Self::new(name)
    }
}

impl PhpExecutorOptions {
    #[must_use]
    pub fn managed_fast_runtime() -> Self {
        Self::for_profile(EngineProfileName::Default)
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn baseline_oracle() -> Self {
        Self::for_profile(EngineProfileName::Baseline)
    }

    #[must_use]
    pub fn for_profile(name: EngineProfileName) -> Self {
        let profile = EngineProfile::new(name);
        Self {
            optimization_level: profile.optimization_level,
            vm_options: profile.vm_options,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_vm::api::{
        BytecodeLayoutMode, ExecutionFormat, InlineCacheMode, JitBlacklistMode, JitMode,
        QuickeningMode, SuperinstructionMode,
    };

    #[test]
    fn parses_canonical_profiles_and_fast_alias() {
        assert_eq!(
            EngineProfileName::parse("baseline").unwrap(),
            EngineProfileName::Baseline
        );
        assert_eq!(
            EngineProfileName::parse("default").unwrap(),
            EngineProfileName::Default
        );
        assert_eq!(
            EngineProfileName::parse("fast").unwrap(),
            EngineProfileName::Default
        );
        assert_eq!(
            EngineProfileName::parse("experimental-jit").unwrap(),
            EngineProfileName::ExperimentalJit
        );
    }

    #[test]
    fn default_profile_is_managed_fast_runtime_with_native_tier() {
        let options = PhpExecutorOptions::managed_fast_runtime();

        assert_eq!(options.optimization_level, OptimizationLevel::O2);
        assert_eq!(
            options.vm_options.include_optimization_level,
            OptimizationLevel::O2
        );
        assert_eq!(options.vm_options.execution_format, ExecutionFormat::Auto);
        assert_eq!(
            options.vm_options.superinstructions,
            SuperinstructionMode::On
        );
        assert_eq!(
            options.vm_options.bytecode_layout,
            BytecodeLayoutMode::Source
        );
        assert_eq!(options.vm_options.quickening, QuickeningMode::On);
        assert_eq!(options.vm_options.inline_caches, InlineCacheMode::On);
        assert_eq!(options.vm_options.jit, JitMode::Cranelift);
        assert_eq!(options.vm_options.jit_blacklist, JitBlacklistMode::On);
        assert!(options.vm_options.tiering.enabled);
        assert!(options.vm_options.typecheck_fast_paths);
        assert!(options.vm_options.internal_function_dispatch_cache);
    }

    #[test]
    fn baseline_profile_keeps_adaptive_features_off() {
        let options = PhpExecutorOptions::baseline_oracle();

        assert_eq!(options.optimization_level, OptimizationLevel::O0);
        assert_eq!(
            options.vm_options.include_optimization_level,
            OptimizationLevel::O0
        );
        assert_eq!(options.vm_options.execution_format, ExecutionFormat::Ir);
        assert_eq!(
            options.vm_options.superinstructions,
            SuperinstructionMode::Off
        );
        assert_eq!(options.vm_options.quickening, QuickeningMode::Off);
        assert_eq!(options.vm_options.inline_caches, InlineCacheMode::Off);
        assert_eq!(options.vm_options.jit, JitMode::Off);
        assert!(!options.vm_options.tiering.enabled);
    }

    #[test]
    fn experimental_jit_profile_is_explicit_native_opt_in() {
        let options = PhpExecutorOptions::for_profile(EngineProfileName::ExperimentalJit);

        assert_eq!(options.optimization_level, OptimizationLevel::O2);
        assert_eq!(options.vm_options.execution_format, ExecutionFormat::Auto);
        assert_eq!(
            options.vm_options.superinstructions,
            SuperinstructionMode::On
        );
        assert_eq!(options.vm_options.quickening, QuickeningMode::On);
        assert_eq!(options.vm_options.inline_caches, InlineCacheMode::On);
        assert_eq!(options.vm_options.jit, JitMode::Cranelift);
        assert!(options.vm_options.tiering.enabled);
    }
}
