use crate::include::{IncludeCache, IncludeLoader};
use crate::inline_cache::InlineCacheMode;
use crate::quickening::QuickeningMode;
use crate::tiering::TieringOptions;
use php_runtime::RuntimeContext;
use std::path::PathBuf;
use std::sync::Arc;

/// VM execution options.
#[derive(Clone, Debug)]
pub struct VmOptions {
    /// Verify IR before dispatching it.
    pub verify_ir: bool,
    /// Maximum instruction dispatches before reporting a runtime error.
    pub max_steps: usize,
    /// Optional local include loader. When absent, include/require are disabled
    /// with deterministic runtime diagnostics.
    pub include_loader: Option<IncludeLoader>,
    /// Optional shared include cache for path resolution and compiled includes.
    pub include_cache: Option<Arc<IncludeCache>>,
    /// Deterministic runtime context used to seed CLI globals and superglobals.
    pub runtime_context: RuntimeContext,
    /// Capture deterministic instruction trace events.
    pub trace: bool,
    /// Capture deterministic runtime object, reference, COW, and suspension events.
    pub trace_runtime: bool,
    /// Collect performance VM/runtime counters in the execution result.
    pub collect_counters: bool,
    /// Optional dense-bytecode execution mode. The default keeps the rich-IR
    /// interpreter as the only execution path.
    pub execution_format: ExecutionFormat,
    /// Optional dense-bytecode superinstruction selection pass.
    pub superinstructions: SuperinstructionMode,
    /// Maintain request-local quickening metadata without changing semantics.
    pub quickening: QuickeningMode,
    /// Allocate request-local inline-cache slots without changing semantics.
    pub inline_caches: InlineCacheMode,
    /// Enable the experimental performance JIT tier for eligible hot leaf functions.
    pub jit: JitMode,
    /// Hot-call threshold requested by the CLI for JIT compilation.
    pub jit_threshold: u64,
    /// Process-local JIT blacklist policy.
    pub jit_blacklist: JitBlacklistMode,
    /// Optional diagnostic path for dumping Cranelift IR for compiled JIT functions.
    pub jit_dump_clif: Option<PathBuf>,
    /// Request-local adaptive tiering policy and stats configuration.
    pub tiering: TieringOptions,
    /// Use conservative fast paths for simple runtime type checks.
    pub typecheck_fast_paths: bool,
    /// Cache request-local internal builtin dispatch metadata.
    pub internal_function_dispatch_cache: bool,
}

impl Default for VmOptions {
    fn default() -> Self {
        Self {
            verify_ir: true,
            max_steps: 100_000,
            include_loader: None,
            include_cache: None,
            runtime_context: RuntimeContext::default(),
            trace: false,
            trace_runtime: false,
            collect_counters: false,
            execution_format: ExecutionFormat::Ir,
            superinstructions: SuperinstructionMode::Off,
            quickening: QuickeningMode::Off,
            inline_caches: InlineCacheMode::Off,
            jit: JitMode::Off,
            jit_threshold: TieringOptions::default().function_entry_threshold,
            jit_blacklist: JitBlacklistMode::On,
            jit_dump_clif: None,
            tiering: TieringOptions::default(),
            typecheck_fast_paths: true,
            internal_function_dispatch_cache: true,
        }
    }
}

/// Optional VM execution-format switch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExecutionFormat {
    /// Execute the current rich-IR interpreter only.
    #[default]
    Ir,
    /// Try dense bytecode first and safely fall back to rich IR if unsupported.
    Auto,
    /// Require dense bytecode; unsupported lowering or verification is an
    /// unsupported runtime result.
    Bytecode,
}

impl ExecutionFormat {
    /// Stable CLI/report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ir => "ir",
            Self::Auto => "auto",
            Self::Bytecode => "bytecode",
        }
    }

    #[must_use]
    pub(super) const fn attempts_bytecode(self) -> bool {
        matches!(self, Self::Auto | Self::Bytecode)
    }

    #[must_use]
    pub(super) const fn is_strict_bytecode(self) -> bool {
        matches!(self, Self::Bytecode)
    }
}

/// Optional dense-bytecode superinstruction selector.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SuperinstructionMode {
    /// Keep lowered dense bytecode unchanged.
    #[default]
    Off,
    /// Fuse supported adjacent dense bytecode patterns.
    On,
}

impl SuperinstructionMode {
    /// Stable CLI/report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }

    #[must_use]
    pub(super) const fn is_enabled(self) -> bool {
        matches!(self, Self::On)
    }
}

/// Experimental JIT switch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum JitMode {
    /// Keep all execution on the interpreter.
    #[default]
    Off,
    /// Accept JIT plumbing flags but keep execution on the interpreter.
    Noop,
    /// Select the Cranelift backend for reports without enabling PHP-code JIT execution yet.
    Cranelift,
}

impl JitMode {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Noop => "noop",
            Self::Cranelift => "cranelift",
        }
    }

    /// Returns true when this mode needs the Cranelift feature to have effect.
    #[must_use]
    pub const fn requires_cranelift(self) -> bool {
        matches!(self, Self::Cranelift)
    }
}

/// Process-local JIT blacklist switch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum JitBlacklistMode {
    /// Keep attempting eligible regions even after repeated failures.
    Off,
    /// Disable unstable regions after deterministic failure thresholds.
    #[default]
    On,
}

impl JitBlacklistMode {
    /// Stable CLI/report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }

    /// Returns true when unstable regions should be suppressed.
    #[must_use]
    pub const fn enabled(self) -> bool {
        matches!(self, Self::On)
    }
}
