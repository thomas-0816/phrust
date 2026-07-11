use crate::bytecode::BytecodeLayoutProfile;
use crate::include::{IncludeCache, IncludeCompiler, IncludeLoader};
use crate::inline_cache::InlineCacheMode;
use crate::quickening::{QuickeningMode, QuickeningSiteSnapshot};
use crate::tiering::TieringOptions;
use php_runtime::RuntimeContext;
use std::path::PathBuf;
use std::sync::Arc;

/// VM execution options.
#[derive(Clone, Debug)]
pub struct VmOptions {
    /// Verify IR before dispatching it.
    pub verify_ir: bool,
    /// Recompute immutable preparation and compare it with the cached image.
    /// Intended for slow validation/debug runs, never production timing.
    pub revalidate_prepared_unit: bool,
    /// Maximum instruction dispatches before reporting a runtime error.
    pub max_steps: usize,
    /// Optional local include loader. When absent, include/require are disabled
    /// with deterministic runtime diagnostics.
    pub include_loader: Option<IncludeLoader>,
    /// Optional shared include cache for path resolution and compiled includes.
    pub include_cache: Option<Arc<IncludeCache>>,
    /// Executor-owned compiler used by runtime include and eval operations.
    pub include_compiler: Option<Arc<dyn IncludeCompiler>>,
    /// Deterministic runtime context used to seed CLI globals and superglobals.
    pub runtime_context: RuntimeContext,
    /// Capture deterministic instruction trace events.
    pub trace: bool,
    /// Capture deterministic runtime object, reference, COW, and suspension events.
    pub trace_runtime: bool,
    /// Capture deterministic include/bootstrap trace events.
    pub trace_includes: bool,
    /// Collect performance VM/runtime counters in the execution result.
    pub collect_counters: bool,
    /// Collect request-profile wall-clock spans around include/call/builtin and
    /// selected operation-family boundaries. This is intentionally separate
    /// from aggregate counters so diagnostic counter runs do not pay timer and
    /// attribution-map costs unless the request profiler is explicitly enabled.
    pub collect_profile_spans: bool,
    /// Collect per-family clone/COW source attribution on top of counters.
    /// This pays per-event accounting on the hottest runtime paths and must
    /// stay an explicit opt-in; it has no effect without `collect_counters`.
    pub collect_layout_source_attribution: bool,
    /// Optional dense-bytecode execution mode. The default keeps the rich-IR
    /// interpreter as the only execution path.
    pub execution_format: ExecutionFormat,
    /// Allow include/require entry functions to use dense bytecode when the
    /// request execution format already attempts dense bytecode.
    pub dense_include_execution: DenseIncludeMode,
    /// Optional dense-bytecode superinstruction selection pass.
    pub superinstructions: SuperinstructionMode,
    /// Optional dense-bytecode jump-threading pass over trampoline blocks.
    pub dense_jump_threading: DenseJumpThreadingMode,
    /// Optional dense-bytecode block layout policy. The default preserves source
    /// block order.
    pub bytecode_layout: BytecodeLayoutMode,
    /// Request-local or CLI-supplied dense-bytecode block profile.
    pub bytecode_layout_profile: Option<BytecodeLayoutProfile>,
    /// Maintain request-local quickening metadata without changing semantics.
    pub quickening: QuickeningMode,
    /// Advisory quickening sites exported by a prior run. Seeded sites keep
    /// the full guard/fallback protocol, so stale seeds self-correct.
    pub quickening_seed: Vec<QuickeningSiteSnapshot>,
    /// Advisory monomorphic function-callsite IC sites exported by a prior
    /// run. Seeded entries keep the full lookup guard protocol (name, arity
    /// shape, and epoch validate at the callsite), so stale seeds invalidate
    /// back to generic resolution.
    pub callsite_seed: Vec<crate::inline_cache::FunctionCallSiteSnapshot>,
    /// Allocate request-local inline-cache slots without changing semantics.
    pub inline_caches: InlineCacheMode,
    /// Select the experimental performance JIT tier for eligible hot leaf functions.
    /// Unsupported builds or ineligible functions stay on managed VM paths.
    pub jit: JitMode,
    /// Per-VM override for the copy-and-patch native leaf tier. `None` follows
    /// the process default (`PHRUST_JIT_COPY_PATCH`, on by default). The tier
    /// runs before dense dispatch and tiering, so tests or harnesses isolating
    /// the Cranelift/interpreter paths must set `Some(false)` — the process env
    /// gate is latched once and cannot be toggled per VM.
    pub copy_patch_leaf_override: Option<bool>,
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
    /// Skip adaptive quickening/tiering setup for tiny units with at most this
    /// many IR instructions. `None` keeps adaptive setup always enabled.
    pub adaptive_tiny_unit_setup_threshold: Option<u32>,
    /// Runtime lever R3: move (instead of clone) a dense register operand when a
    /// conservative last-use analysis proves the read is the register's block-local
    /// last use. Default-on (set `false` to disable); when disabled the dense read
    /// path is byte-identical to the pre-lever engine and this analysis is never
    /// built. Preserves COW/reference semantics.
    pub last_use_moves: bool,
    /// Worker-stable symbol epochs: identical include replay keeps the
    /// lookup epoch constant across requests so slot-indexed inline caches
    /// with request-stable targets survive the request boundary. Read from
    /// `PHRUST_WORKER_SYMBOL_EPOCH` (off unless set to `1`).
    pub worker_symbol_epoch: bool,
    /// Runtime lever R4: allow request-local frame/register pooling to reuse a
    /// completed activation for a class-context call (a method/constructor/static
    /// call, or any call that carries `$this`/scope/called/declaring class) when
    /// that call clears every other reuse guard. Default-on (set `false` to
    /// disable); when disabled the `class_context` reuse block stays in place and
    /// the call path is byte-identical to the pre-lever engine. The reuse/reset
    /// path fully clears `$this` and all class-context frame state, so nothing
    /// leaks from the prior occupant, and teardown drops the prior occupant's
    /// values at the same PHP-observable moment as the fresh-frame path.
    pub reuse_class_context_frames: bool,
}

impl Default for VmOptions {
    fn default() -> Self {
        Self {
            verify_ir: true,
            revalidate_prepared_unit: false,
            max_steps: 100_000,
            include_loader: None,
            include_cache: None,
            include_compiler: default_include_compiler(),
            runtime_context: RuntimeContext::default(),
            trace: false,
            trace_runtime: false,
            trace_includes: trace_includes_from_env(),
            collect_counters: false,
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
            execution_format: ExecutionFormat::Ir,
            dense_include_execution: DenseIncludeMode::Off,
            superinstructions: SuperinstructionMode::Off,
            dense_jump_threading: DenseJumpThreadingMode::Off,
            bytecode_layout: BytecodeLayoutMode::Source,
            bytecode_layout_profile: None,
            quickening: QuickeningMode::Off,
            quickening_seed: Vec::new(),
            callsite_seed: Vec::new(),
            inline_caches: InlineCacheMode::Off,
            jit: JitMode::Off,
            copy_patch_leaf_override: None,
            jit_threshold: TieringOptions::default().function_entry_threshold,
            jit_blacklist: JitBlacklistMode::On,
            jit_dump_clif: None,
            tiering: TieringOptions::default(),
            typecheck_fast_paths: true,
            internal_function_dispatch_cache: true,
            adaptive_tiny_unit_setup_threshold: None,
            last_use_moves: true,
            worker_symbol_epoch: worker_symbol_epoch_from_env(),
            reuse_class_context_frames: true,
        }
    }
}

#[cfg(not(test))]
fn default_include_compiler() -> Option<Arc<dyn IncludeCompiler>> {
    None
}

#[cfg(test)]
fn default_include_compiler() -> Option<Arc<dyn IncludeCompiler>> {
    Some(Arc::new(
        crate::test_include_compiler::TestIncludeCompiler::baseline(),
    ))
}

fn trace_includes_from_env() -> bool {
    std::env::var("PHRUST_TRACE_INCLUDES").is_ok_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Optional dense-bytecode execution for include/require entry functions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DenseIncludeMode {
    /// Includes execute through the rich-IR interpreter.
    #[default]
    Off,
    /// Includes try the same guarded dense-bytecode entry path as the main unit
    /// when the request execution format is `auto` or `bytecode`.
    Auto,
}

impl DenseIncludeMode {
    /// Stable CLI/report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Auto => "auto",
        }
    }

    #[must_use]
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Auto)
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

/// Optional dense-bytecode block layout mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BytecodeLayoutMode {
    /// Preserve dense blocks in source/control-flow lowering order.
    #[default]
    Source,
    /// Reorder dense block descriptors from a supplied local profile.
    Profiled,
}

impl BytecodeLayoutMode {
    /// Stable CLI/report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Profiled => "profiled",
        }
    }

    #[must_use]
    pub(super) const fn is_profiled(self) -> bool {
        matches!(self, Self::Profiled)
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

/// Optional dense jump-threading switch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DenseJumpThreadingMode {
    /// Keep lowered branch targets unchanged.
    #[default]
    Off,
    /// Thread explicit branch edges through bare-jump trampoline blocks.
    On,
}

impl DenseJumpThreadingMode {
    /// Stable CLI/report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }

    /// True when the pass may run.
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::On)
    }
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
    /// Select the Cranelift backend. Native entry is constrained to eligible
    /// experimental leaf functions when feature support and runtime guards allow
    /// it; otherwise execution remains on managed VM paths.
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

fn worker_symbol_epoch_from_env() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| {
        std::env::var("PHRUST_WORKER_SYMBOL_EPOCH").is_ok_and(|value| value.trim() == "1")
    })
}
