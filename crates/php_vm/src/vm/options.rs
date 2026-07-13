use crate::include::{IncludeCache, IncludeCompiler, IncludeLoader};
use crate::inline_cache::InlineCacheMode;
use crate::tiering::TieringOptions;
use php_runtime::api::RuntimeContext;
use std::path::PathBuf;
use std::sync::Arc;

/// Options for the native PHP execution coordinator.
#[derive(Clone, Debug)]
pub struct VmOptions {
    /// Verify authoritative IR before native compilation.
    pub verify_ir: bool,
    /// Recompute immutable preparation and compare it with the cached image.
    pub revalidate_prepared_unit: bool,
    /// Optional local include loader.
    pub include_loader: Option<IncludeLoader>,
    /// Optional shared include cache for path resolution and compiled includes.
    pub include_cache: Option<Arc<IncludeCache>>,
    /// Executor-owned compiler used by native include and eval operations.
    pub include_compiler: Option<Arc<dyn IncludeCompiler>>,
    /// Deterministic runtime context used to seed request state.
    pub runtime_context: RuntimeContext,
    /// Capture deterministic native execution trace events.
    pub trace: bool,
    /// Capture deterministic runtime object and suspension events.
    pub trace_runtime: bool,
    /// Capture deterministic include/bootstrap trace events.
    pub trace_includes: bool,
    /// Collect native/runtime counters in the execution result.
    pub collect_counters: bool,
    /// Collect request-profile wall-clock spans.
    pub collect_profile_spans: bool,
    /// Collect per-family clone/COW source attribution.
    pub collect_layout_source_attribution: bool,
    /// Allocate request-local inline-cache slots for native call sites.
    pub inline_caches: InlineCacheMode,
    /// Select baseline or optimizing Cranelift compilation.
    pub native_optimization: NativeOptimizationPolicy,
    /// Native compilation threshold requested by frontends.
    pub jit_threshold: u64,
    /// Process-local native-version blacklist policy.
    pub jit_blacklist: JitBlacklistMode,
    /// Optional diagnostic path for dumping Cranelift IR.
    pub jit_dump_clif: Option<PathBuf>,
    /// Native tier compilation budgets and publication policy.
    pub tiering: TieringOptions,
}

impl Default for VmOptions {
    fn default() -> Self {
        let tiering = TieringOptions::default();
        Self {
            verify_ir: true,
            revalidate_prepared_unit: false,
            include_loader: None,
            include_cache: None,
            include_compiler: None,
            runtime_context: RuntimeContext::default(),
            trace: false,
            trace_runtime: false,
            trace_includes: trace_includes_from_env(),
            collect_counters: false,
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
            inline_caches: InlineCacheMode::Off,
            native_optimization: NativeOptimizationPolicy::Baseline,
            jit_threshold: tiering.function_entry_threshold,
            jit_blacklist: JitBlacklistMode::On,
            jit_dump_clif: None,
            tiering,
        }
    }
}

/// Optimization policy for the mandatory Cranelift compiler.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NativeOptimizationPolicy {
    #[default]
    Baseline,
    Optimizing,
}

impl NativeOptimizationPolicy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Optimizing => "optimizing",
        }
    }

    #[must_use]
    pub const fn is_optimizing(self) -> bool {
        matches!(self, Self::Optimizing)
    }
}

/// Process-local native-version blacklist switch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum JitBlacklistMode {
    Off,
    #[default]
    On,
}

impl JitBlacklistMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }

    #[must_use]
    pub const fn enabled(self) -> bool {
        matches!(self, Self::On)
    }
}

fn trace_includes_from_env() -> bool {
    std::env::var("PHRUST_TRACE_INCLUDES").is_ok_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}
