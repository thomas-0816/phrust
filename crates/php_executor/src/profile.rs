use php_optimizer::OptimizationLevel;
use php_vm::api::{InlineCacheMode, NativeBlacklistMode, NativeOptimizationPolicy, TieringOptions};
use std::{fmt, str::FromStr};

use crate::PhpExecutorOptions;

/// Canonical mandatory-native profile shared by CLI and server entry points.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EngineProfileName {
    Baseline,
    #[default]
    Default,
}

impl EngineProfileName {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Default => "default",
        }
    }

    #[must_use]
    pub const fn accepted_values() -> &'static str {
        "baseline or default"
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
            "default" => Ok(Self::Default),
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

#[derive(Clone, Debug)]
pub struct EngineProfile {
    pub name: EngineProfileName,
    pub optimization_level: OptimizationLevel,
    pub include_optimization_level: OptimizationLevel,
    pub vm_options: php_vm::api::VmOptions,
}

impl EngineProfile {
    #[must_use]
    pub fn new(name: EngineProfileName) -> Self {
        let mut vm_options = php_vm::api::VmOptions::default();
        let optimization_level = match name {
            EngineProfileName::Baseline => {
                vm_options.inline_caches = InlineCacheMode::Off;
                vm_options.native_optimization = NativeOptimizationPolicy::Baseline;
                vm_options.native_blacklist = NativeBlacklistMode::On;
                vm_options.tiering.enabled = true;
                vm_options.tiering.native_eager = true;
                OptimizationLevel::O0
            }
            EngineProfileName::Default => {
                vm_options.inline_caches = InlineCacheMode::On;
                // Optimizing lowering is bounded by the native fragment plan;
                // large PHP functions never enter whole-function Cranelift
                // optimization or register allocation.
                vm_options.native_optimization = NativeOptimizationPolicy::Optimizing;
                vm_options.native_blacklist = NativeBlacklistMode::On;
                vm_options.tiering = TieringOptions::default();
                OptimizationLevel::O2
            }
        };
        vm_options.native_threshold = vm_options.tiering.function_entry_threshold;
        let include_optimization_level = match name {
            EngineProfileName::Default => OptimizationLevel::O0,
            EngineProfileName::Baseline => optimization_level,
        };
        Self {
            name,
            optimization_level,
            include_optimization_level,
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
    pub fn default_native_runtime() -> Self {
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
            include_optimization_level: profile.include_optimization_level,
            vm_options: profile.vm_options,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_default_uses_bounded_native_optimization() {
        let baseline = PhpExecutorOptions::baseline_oracle();
        let optimized = PhpExecutorOptions::default_native_runtime();
        assert_eq!(
            baseline.vm_options.native_optimization,
            NativeOptimizationPolicy::Baseline
        );
        assert_eq!(
            optimized.vm_options.native_optimization,
            NativeOptimizationPolicy::Optimizing
        );
    }
}
