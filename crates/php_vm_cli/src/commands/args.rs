use super::*;

pub(super) struct CompileOptions<'a> {
    pub(super) path: &'a str,
    pub(super) json: bool,
    pub(super) opt_level: OptimizationLevel,
    pub(super) timings_json: Option<String>,
}

pub(super) fn parse_compile_args(args: &[String]) -> Result<CompileOptions<'_>, String> {
    let mut path = None;
    let mut json = false;
    let mut opt_level = OptimizationLevel::O0;
    let mut timings_json = std::env::var("PHRUST_TIMINGS_JSON")
        .ok()
        .filter(|value| !value.is_empty());
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--opt-level" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("compile --opt-level requires <level>".to_string());
                };
                opt_level = parse_optimization_level(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--opt-level=") => {
                opt_level = parse_optimization_level(value)?;
            }
            "--timings-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("compile --timings-json requires <path>".to_string());
                };
                timings_json = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--timings-json=") => {
                timings_json = Some(value.to_owned());
            }
            arg if path.is_none() => path = Some(arg),
            arg => return Err(format!("unexpected compile argument `{arg}`")),
        }
        index += 1;
    }
    let Some(path) = path else {
        return Err("compile requires <path.php>".to_string());
    };
    Ok(CompileOptions {
        path,
        json,
        opt_level,
        timings_json,
    })
}

pub(super) struct DumpIrOptions<'a> {
    pub(super) path: &'a str,
    pub(super) with_source: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct BytecodePatternReport {
    pub(super) blocks: u64,
    pub(super) instructions: u64,
    pub(super) pairs: BTreeMap<String, u64>,
    pub(super) triples: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct BaselineNativeStencilReport {
    pub(super) functions: u64,
    pub(super) blocks: u64,
    pub(super) instructions: u64,
    pub(super) stencilable_instructions: u64,
    pub(super) unsupported_instructions: u64,
    pub(super) helper_calls: u64,
    pub(super) deopt_slots: u64,
    pub(super) compile_cost_units: u64,
    pub(super) code_size_bytes_estimate: u64,
    pub(super) opcode_counts: BTreeMap<String, u64>,
    pub(super) unsupported_by_reason: BTreeMap<String, u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BaselineStencilClass {
    pub(super) helper_calls: u64,
    pub(super) deopt_slots: u64,
    pub(super) compile_cost_units: u64,
    pub(super) code_size_bytes_estimate: u64,
    pub(super) unsupported_reason: Option<&'static str>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct CopyPatchStencilReport {
    pub(super) functions: u64,
    pub(super) blocks: u64,
    pub(super) instructions: u64,
    pub(super) quickened_superinstructions: u64,
    pub(super) unsupported_instructions: u64,
    pub(super) patch_sites: u64,
    pub(super) helper_calls: u64,
    pub(super) live_state_slots: u64,
    pub(super) deopt_points: u64,
    pub(super) compile_cost_units: u64,
    pub(super) code_size_bytes_estimate: u64,
    pub(super) stencils: Vec<CopyPatchStencil>,
    pub(super) unsupported_by_reason: BTreeMap<String, u64>,
    pub(super) stencil_kinds: BTreeMap<String, u64>,
}

impl CopyPatchStencilReport {
    pub(super) fn work_to_compile_ratio(&self) -> String {
        if self.compile_cost_units == 0 {
            return "0.000".to_string();
        }
        format!(
            "{:.3}",
            self.stencils.len() as f64 / self.compile_cost_units as f64
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CopyPatchStencil {
    pub(super) function: u32,
    pub(super) block: u32,
    pub(super) instruction: u32,
    pub(super) opcode: &'static str,
    pub(super) kind: &'static str,
    pub(super) patch_sites: Vec<&'static str>,
    pub(super) guard_dependencies: Vec<&'static str>,
    pub(super) helper_calls: Vec<&'static str>,
    pub(super) live_state_requirements: Vec<&'static str>,
    pub(super) side_exit_target: &'static str,
    pub(super) code_size_bytes_estimate: u64,
    pub(super) compile_cost_units: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CopyPatchStencilClass {
    pub(super) kind: &'static str,
    pub(super) patch_sites: &'static [&'static str],
    pub(super) guard_dependencies: &'static [&'static str],
    pub(super) helper_calls: &'static [&'static str],
    pub(super) live_state_requirements: &'static [&'static str],
    pub(super) side_exit_target: &'static str,
    pub(super) code_size_bytes_estimate: u64,
    pub(super) compile_cost_units: u64,
    pub(super) unsupported_reason: Option<&'static str>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct MidTierPlanReport {
    pub(super) quickened_superinstructions: u64,
    pub(super) functions: Vec<MidTierFunctionPlan>,
    pub(super) eligible_functions: u64,
    pub(super) rejected_functions: u64,
    pub(super) candidate_optimizations: BTreeMap<String, u64>,
    pub(super) rejection_reasons: BTreeMap<String, u64>,
    pub(super) expected_guards: BTreeMap<String, u64>,
    pub(super) required_helpers: BTreeMap<String, u64>,
    pub(super) deopt_points: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct MidTierFunctionPlan {
    pub(super) function: u32,
    pub(super) instruction_count: u64,
    pub(super) classification: &'static str,
    pub(super) candidate_optimizations: Vec<&'static str>,
    pub(super) rejection_reasons: Vec<&'static str>,
    pub(super) expected_guards: Vec<&'static str>,
    pub(super) required_helpers: Vec<&'static str>,
    pub(super) deopt_points: u64,
}

pub(super) struct RunOptions<'a> {
    pub(super) path: &'a str,
    pub(super) script_args: Vec<String>,
    pub(super) env: Vec<(String, String)>,
    pub(super) stdin: Vec<u8>,
    pub(super) debug: bool,
    pub(super) debug_log: Option<String>,
    pub(super) error_format: DiagnosticOutputFormat,
    pub(super) trace: bool,
    pub(super) trace_runtime: bool,
    pub(super) trace_includes: bool,
    pub(super) counters_json: Option<String>,
    pub(super) timings_json: Option<String>,
    pub(super) region_profile_json: Option<String>,
    pub(super) bytecode_cache: BytecodeCacheOptions,
    pub(super) opt_level: OptimizationLevel,
    pub(super) include_opt_level: OptimizationLevel,
    pub(super) execution_format: ExecutionFormat,
    pub(super) superinstructions: SuperinstructionMode,
    pub(super) last_use_moves: bool,
    pub(super) reuse_class_context_frames: bool,
    pub(super) dense_jump_threading: DenseJumpThreadingMode,
    pub(super) bytecode_layout: BytecodeLayoutMode,
    pub(super) bytecode_layout_profile: Option<String>,
    pub(super) quickening: QuickeningMode,
    pub(super) inline_caches: InlineCacheMode,
    pub(super) jit: JitMode,
    pub(super) jit_explicit: bool,
    pub(super) jit_threshold: u64,
    pub(super) jit_blacklist: JitBlacklistMode,
    pub(super) jit_dump_clif: Option<String>,
    pub(super) jit_stats: JitStatsMode,
    pub(super) tiering: TieringOptions,
    pub(super) adaptive_tiny_unit_setup_threshold: Option<u32>,
    pub(super) tiering_stats_json: Option<String>,
    pub(super) persistent_feedback: PersistentFeedbackOptions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BytecodeCacheMode {
    Off,
    Read,
    Write,
    ReadWrite,
}

impl BytecodeCacheMode {
    pub(super) fn can_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    pub(super) fn can_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Read => "read",
            Self::Write => "write",
            Self::ReadWrite => "read-write",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BytecodeCacheOptions {
    pub(super) mode: BytecodeCacheMode,
    pub(super) dir: Option<PathBuf>,
    pub(super) stats: bool,
    pub(super) clear: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct PersistentFeedbackOptions {
    pub(super) read: Option<String>,
    pub(super) write: Option<String>,
    pub(super) stats_json: Option<String>,
}

impl Default for BytecodeCacheOptions {
    fn default() -> Self {
        Self {
            mode: default_bytecode_cache_mode(),
            dir: None,
            stats: false,
            clear: false,
        }
    }
}

/// The bytecode cache is on by default (like an opcache); the
/// `PHRUST_BYTECODE_CACHE` environment variable overrides the default and
/// the `--bytecode-cache` flag overrides both. Unrecognized values keep
/// the default so a typo cannot silently disable correctness-neutral
/// caching or invent a mode.
pub(super) fn default_bytecode_cache_mode() -> BytecodeCacheMode {
    match std::env::var("PHRUST_BYTECODE_CACHE").as_deref() {
        Ok("off") => BytecodeCacheMode::Off,
        Ok("read") => BytecodeCacheMode::Read,
        Ok("write") => BytecodeCacheMode::Write,
        Ok("read-write") => BytecodeCacheMode::ReadWrite,
        _ => BytecodeCacheMode::ReadWrite,
    }
}

#[derive(Clone, Debug)]
pub(super) struct BytecodeCacheContext {
    pub(super) fingerprint: CacheFingerprint,
    pub(super) cache_file: PathBuf,
}

#[derive(Clone, Debug)]
pub(super) struct BytecodeCacheStats {
    pub(super) mode: BytecodeCacheMode,
    pub(super) cache_file: Option<PathBuf>,
    pub(super) hit: bool,
    pub(super) miss: bool,
    pub(super) wrote: bool,
    pub(super) cleared: bool,
    pub(super) compile_error: bool,
    pub(super) load_error: Option<String>,
    pub(super) store_error: Option<String>,
}

impl BytecodeCacheStats {
    pub(super) fn new(mode: BytecodeCacheMode) -> Self {
        Self {
            mode,
            cache_file: None,
            hit: false,
            miss: false,
            wrote: false,
            cleared: false,
            compile_error: false,
            load_error: None,
            store_error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReportFormat {
    Markdown,
    Html,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum JitStatsMode {
    #[default]
    Off,
    Json,
}

impl JitStatsMode {
    pub(super) fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

pub(super) struct ReportOptions<'a> {
    pub(super) path: &'a str,
    pub(super) format: ReportFormat,
}

pub(super) fn parse_dump_ir_args(args: &[String]) -> Result<DumpIrOptions<'_>, String> {
    let mut path = None;
    let mut with_source = false;
    for arg in args {
        if arg == "--with-source" {
            with_source = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else {
            return Err(format!("unexpected dump-ir argument `{arg}`"));
        }
    }
    let Some(path) = path else {
        return Err("dump-ir requires <path.php>".to_string());
    };
    Ok(DumpIrOptions { path, with_source })
}

pub(super) fn parse_dump_bytecode_patterns_args(args: &[String]) -> Result<(&str, bool), String> {
    let mut path = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else {
            return Err(format!(
                "unexpected dump-bytecode-patterns argument `{arg}`"
            ));
        }
    }
    let Some(path) = path else {
        return Err("dump-bytecode-patterns requires <path.php>".to_string());
    };
    Ok((path, json))
}

pub(super) fn parse_dump_rule_selection_args(args: &[String]) -> Result<(&str, bool), String> {
    let mut path = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else {
            return Err(format!("unexpected dump-rule-selection argument `{arg}`"));
        }
    }
    let Some(path) = path else {
        return Err("dump-rule-selection requires <path.php>".to_string());
    };
    Ok((path, json))
}

pub(super) fn parse_dump_dependency_units_args(args: &[String]) -> Result<(&str, bool), String> {
    let mut path = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else {
            return Err(format!("unexpected dump-dependency-units argument `{arg}`"));
        }
    }
    let Some(path) = path else {
        return Err("dump-dependency-units requires <path.php>".to_string());
    };
    Ok((path, json))
}

pub(super) fn parse_dump_baseline_native_stencil_args(
    args: &[String],
) -> Result<(&str, bool), String> {
    let mut path = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else {
            return Err(format!(
                "unexpected dump-baseline-native-stencil argument `{arg}`"
            ));
        }
    }
    let Some(path) = path else {
        return Err("dump-baseline-native-stencil requires <path.php>".to_string());
    };
    Ok((path, json))
}

pub(super) fn parse_dump_copy_patch_stencils_args(args: &[String]) -> Result<(&str, bool), String> {
    let mut path = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else {
            return Err(format!(
                "unexpected dump-copy-patch-stencils argument `{arg}`"
            ));
        }
    }
    let Some(path) = path else {
        return Err("dump-copy-patch-stencils requires <path.php>".to_string());
    };
    Ok((path, json))
}

pub(super) fn parse_dump_mid_tier_plan_args(args: &[String]) -> Result<(&str, bool), String> {
    let mut path = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else {
            return Err(format!("unexpected dump-mid-tier-plan argument `{arg}`"));
        }
    }
    let Some(path) = path else {
        return Err("dump-mid-tier-plan requires <path.php>".to_string());
    };
    Ok((path, json))
}

pub(super) fn parse_run_args(args: &[String]) -> Result<RunOptions<'_>, String> {
    let Some(_) = args.first() else {
        return Err("run requires <path.php>".to_string());
    };

    let default_options = PhpExecutorOptions::managed_fast_runtime();
    let mut path = None;
    let mut env = Vec::new();
    let mut debug = debug_enabled_from_env("PHRUST_DEBUG");
    let mut debug_log = std::env::var("PHRUST_DEBUG_LOG")
        .ok()
        .filter(|value| !value.is_empty());
    let mut error_format = error_format_from_env("PHRUST_ERROR_FORMAT");
    let mut trace = false;
    let mut trace_runtime = false;
    let mut counters_json = None;
    let mut timings_json = std::env::var("PHRUST_TIMINGS_JSON")
        .ok()
        .filter(|value| !value.is_empty());
    let mut region_profile_json = None;
    let mut bytecode_cache = BytecodeCacheOptions::default();
    let mut opt_level = default_options.optimization_level;
    let mut include_opt_level = default_options.vm_options.include_optimization_level;
    let mut execution_format = default_options.vm_options.execution_format;
    let mut superinstructions = default_options.vm_options.superinstructions;
    let mut last_use_moves = default_options.vm_options.last_use_moves;
    let mut reuse_class_context_frames = default_options.vm_options.reuse_class_context_frames;
    let mut dense_jump_threading = default_options.vm_options.dense_jump_threading;
    let mut bytecode_layout = default_options.vm_options.bytecode_layout;
    let mut bytecode_layout_profile = None;
    let mut quickening = default_options.vm_options.quickening;
    let mut inline_caches = default_options.vm_options.inline_caches;
    let mut jit = default_options.vm_options.jit;
    let mut jit_explicit = false;
    let mut jit_threshold = default_options.vm_options.jit_threshold;
    let mut jit_blacklist = default_options.vm_options.jit_blacklist;
    let mut jit_dump_clif = None;
    let mut jit_stats = JitStatsMode::Off;
    let mut tiering = default_options.vm_options.tiering;
    let mut adaptive_tiny_unit_setup_threshold = default_options
        .vm_options
        .adaptive_tiny_unit_setup_threshold;
    let mut tiering_stats_json = None;
    let mut persistent_feedback = PersistentFeedbackOptions::default();
    let mut tiering_function_threshold_explicit = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--debug" => debug = true,
            "--debug-log" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --debug-log requires <path>".to_string());
                };
                debug_log = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--debug-log=") => {
                debug_log = Some(value.to_owned());
            }
            "--error-format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --error-format requires text or json".to_string());
                };
                error_format = parse_diagnostic_output_format(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--error-format=") => {
                error_format = parse_diagnostic_output_format(value)?;
            }
            "--trace" => trace = true,
            "--trace-runtime" => trace_runtime = true,
            "--engine-preset" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(format!(
                        "run --engine-preset requires {}",
                        EngineProfileName::accepted_values()
                    ));
                };
                let preset = parse_engine_preset(value)?;
                let profile_options = PhpExecutorOptions::for_profile(preset);
                // The baseline compatibility oracle stays cache-free; the
                // managed presets keep the default-on cache unless the
                // cache flags say otherwise.
                if preset == EngineProfileName::Baseline {
                    bytecode_cache.mode = BytecodeCacheMode::Off;
                }
                opt_level = profile_options.optimization_level;
                include_opt_level = profile_options.vm_options.include_optimization_level;
                execution_format = profile_options.vm_options.execution_format;
                superinstructions = profile_options.vm_options.superinstructions;
                dense_jump_threading = profile_options.vm_options.dense_jump_threading;
                bytecode_layout = profile_options.vm_options.bytecode_layout;
                quickening = profile_options.vm_options.quickening;
                inline_caches = profile_options.vm_options.inline_caches;
                jit = profile_options.vm_options.jit;
                jit_blacklist = profile_options.vm_options.jit_blacklist;
                tiering = profile_options.vm_options.tiering;
                adaptive_tiny_unit_setup_threshold = profile_options
                    .vm_options
                    .adaptive_tiny_unit_setup_threshold;
                jit_threshold = profile_options.vm_options.jit_threshold;
                tiering_function_threshold_explicit = false;
            }
            arg if let Some(value) = arg.strip_prefix("--engine-preset=") => {
                let preset = parse_engine_preset(value)?;
                let profile_options = PhpExecutorOptions::for_profile(preset);
                if preset == EngineProfileName::Baseline {
                    bytecode_cache.mode = BytecodeCacheMode::Off;
                }
                opt_level = profile_options.optimization_level;
                include_opt_level = profile_options.vm_options.include_optimization_level;
                execution_format = profile_options.vm_options.execution_format;
                superinstructions = profile_options.vm_options.superinstructions;
                dense_jump_threading = profile_options.vm_options.dense_jump_threading;
                bytecode_layout = profile_options.vm_options.bytecode_layout;
                quickening = profile_options.vm_options.quickening;
                inline_caches = profile_options.vm_options.inline_caches;
                jit = profile_options.vm_options.jit;
                jit_blacklist = profile_options.vm_options.jit_blacklist;
                tiering = profile_options.vm_options.tiering;
                adaptive_tiny_unit_setup_threshold = profile_options
                    .vm_options
                    .adaptive_tiny_unit_setup_threshold;
                jit_threshold = profile_options.vm_options.jit_threshold;
                tiering_function_threshold_explicit = false;
            }
            "--bytecode-cache" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(
                        "run --bytecode-cache requires off, read, write, or read-write".to_string(),
                    );
                };
                bytecode_cache.mode = parse_bytecode_cache_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--bytecode-cache=") => {
                bytecode_cache.mode = parse_bytecode_cache_mode(value)?;
            }
            "--bytecode-cache-dir" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --bytecode-cache-dir requires <path>".to_string());
                };
                bytecode_cache.dir = Some(PathBuf::from(value));
            }
            arg if let Some(value) = arg.strip_prefix("--bytecode-cache-dir=") => {
                bytecode_cache.dir = Some(PathBuf::from(value));
            }
            "--bytecode-cache-stats" => bytecode_cache.stats = true,
            "--clear-bytecode-cache" => bytecode_cache.clear = true,
            "--opt-level" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --opt-level requires <level>".to_string());
                };
                opt_level = parse_optimization_level(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--opt-level=") => {
                opt_level = parse_optimization_level(value)?;
            }
            "--exec-format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --exec-format requires ir, auto, or bytecode".to_string());
                };
                execution_format = parse_execution_format(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--exec-format=") => {
                execution_format = parse_execution_format(value)?;
            }
            "--dense-jump-threading" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --dense-jump-threading requires off or on".to_string());
                };
                dense_jump_threading = parse_dense_jump_threading_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--dense-jump-threading=") => {
                dense_jump_threading = parse_dense_jump_threading_mode(value)?;
            }
            "--superinstructions" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --superinstructions requires off or on".to_string());
                };
                superinstructions = parse_superinstruction_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--superinstructions=") => {
                superinstructions = parse_superinstruction_mode(value)?;
            }
            "--last-use-moves" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --last-use-moves requires off or on".to_string());
                };
                last_use_moves = parse_last_use_moves_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--last-use-moves=") => {
                last_use_moves = parse_last_use_moves_mode(value)?;
            }
            "--reuse-class-context-frames" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --reuse-class-context-frames requires off or on".to_string());
                };
                reuse_class_context_frames = parse_reuse_class_context_frames_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--reuse-class-context-frames=") => {
                reuse_class_context_frames = parse_reuse_class_context_frames_mode(value)?;
            }
            "--bytecode-layout" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --bytecode-layout requires source or profiled".to_string());
                };
                bytecode_layout = parse_bytecode_layout_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--bytecode-layout=") => {
                bytecode_layout = parse_bytecode_layout_mode(value)?;
            }
            "--bytecode-layout-profile" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --bytecode-layout-profile requires <path>".to_string());
                };
                bytecode_layout_profile = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--bytecode-layout-profile=") => {
                bytecode_layout_profile = Some(value.to_owned());
            }
            "--quickening" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --quickening requires off or on".to_string());
                };
                quickening = parse_quickening_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--quickening=") => {
                quickening = parse_quickening_mode(value)?;
            }
            "--inline-caches" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --inline-caches requires off or on".to_string());
                };
                inline_caches = parse_inline_cache_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--inline-caches=") => {
                inline_caches = parse_inline_cache_mode(value)?;
            }
            "--jit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit requires off, noop, or cranelift".to_string());
                };
                jit = parse_jit_mode(value)?;
                jit_explicit = true;
            }
            arg if let Some(value) = arg.strip_prefix("--jit=") => {
                jit = parse_jit_mode(value)?;
                jit_explicit = true;
            }
            "--jit-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-threshold requires <count>".to_string());
                };
                jit_threshold = parse_u64_option(value, "jit-threshold")?;
                if !tiering_function_threshold_explicit {
                    tiering.function_entry_threshold = jit_threshold;
                }
            }
            arg if let Some(value) = arg.strip_prefix("--jit-threshold=") => {
                jit_threshold = parse_u64_option(value, "jit-threshold")?;
                if !tiering_function_threshold_explicit {
                    tiering.function_entry_threshold = jit_threshold;
                }
            }
            "--jit-max-compile-us" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-max-compile-us requires <microseconds>".to_string());
                };
                tiering.jit_max_compile_us = parse_u64_option(value, "jit-max-compile-us")?;
            }
            arg if let Some(value) = arg.strip_prefix("--jit-max-compile-us=") => {
                tiering.jit_max_compile_us = parse_u64_option(value, "jit-max-compile-us")?;
            }
            "--jit-max-functions" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-max-functions requires <count>".to_string());
                };
                tiering.jit_max_functions = parse_u64_option(value, "jit-max-functions")?;
            }
            arg if let Some(value) = arg.strip_prefix("--jit-max-functions=") => {
                tiering.jit_max_functions = parse_u64_option(value, "jit-max-functions")?;
            }
            "--jit-eager" => {
                tiering.jit_eager = true;
                jit_threshold = 1;
                if !tiering_function_threshold_explicit {
                    tiering.function_entry_threshold = 1;
                }
            }
            "--jit-blacklist" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-blacklist requires off or on".to_string());
                };
                jit_blacklist = parse_jit_blacklist_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--jit-blacklist=") => {
                jit_blacklist = parse_jit_blacklist_mode(value)?;
            }
            "--jit-dump-clif" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-dump-clif requires <path>".to_string());
                };
                jit_dump_clif = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--jit-dump-clif=") => {
                jit_dump_clif = Some(value.to_owned());
            }
            "--jit-stats" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-stats requires json".to_string());
                };
                jit_stats = parse_jit_stats_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--jit-stats=") => {
                jit_stats = parse_jit_stats_mode(value)?;
            }
            "--tiering" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering requires off or on".to_string());
                };
                tiering.enabled = parse_on_off(value, "tiering")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering=") => {
                tiering.enabled = parse_on_off(value, "tiering")?;
            }
            "--tiering-function-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering-function-threshold requires <count>".to_string());
                };
                tiering_function_threshold_explicit = true;
                tiering.function_entry_threshold =
                    parse_u64_option(value, "tiering-function-threshold")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-function-threshold=") => {
                tiering_function_threshold_explicit = true;
                tiering.function_entry_threshold =
                    parse_u64_option(value, "tiering-function-threshold")?;
            }
            "--tiering-loop-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering-loop-threshold requires <count>".to_string());
                };
                tiering.loop_backedge_threshold =
                    parse_u64_option(value, "tiering-loop-threshold")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-loop-threshold=") => {
                tiering.loop_backedge_threshold =
                    parse_u64_option(value, "tiering-loop-threshold")?;
            }
            "--tiering-ic-stability-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering-ic-stability-threshold requires <score>".to_string());
                };
                tiering.ic_stability_threshold =
                    parse_i64_option(value, "tiering-ic-stability-threshold")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-ic-stability-threshold=") => {
                tiering.ic_stability_threshold =
                    parse_i64_option(value, "tiering-ic-stability-threshold")?;
            }
            "--tiering-guard-failure-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(
                        "run --tiering-guard-failure-threshold requires <count>".to_string()
                    );
                };
                tiering.guard_failure_threshold =
                    parse_u64_option(value, "tiering-guard-failure-threshold")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-guard-failure-threshold=") => {
                tiering.guard_failure_threshold =
                    parse_u64_option(value, "tiering-guard-failure-threshold")?;
            }
            "--tiering-stats-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering-stats-json requires <path>".to_string());
                };
                tiering_stats_json = Some(value.clone());
                tiering.collect_stats = true;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-stats-json=") => {
                tiering_stats_json = Some(value.to_owned());
                tiering.collect_stats = true;
            }
            "--persistent-feedback-read" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --persistent-feedback-read requires <path>".to_string());
                };
                persistent_feedback.read = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--persistent-feedback-read=") => {
                persistent_feedback.read = Some(value.to_owned());
            }
            "--persistent-feedback-write" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --persistent-feedback-write requires <path>".to_string());
                };
                persistent_feedback.write = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--persistent-feedback-write=") => {
                persistent_feedback.write = Some(value.to_owned());
            }
            "--persistent-feedback-stats-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --persistent-feedback-stats-json requires <path>".to_string());
                };
                persistent_feedback.stats_json = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--persistent-feedback-stats-json=") => {
                persistent_feedback.stats_json = Some(value.to_owned());
            }
            "--counters-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --counters-json requires <path>".to_string());
                };
                counters_json = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--counters-json=") => {
                counters_json = Some(value.to_owned());
            }
            "--timings-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --timings-json requires <path>".to_string());
                };
                timings_json = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--timings-json=") => {
                timings_json = Some(value.to_owned());
            }
            "--region-profile-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --region-profile-json requires <path>".to_string());
                };
                region_profile_json = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--region-profile-json=") => {
                region_profile_json = Some(value.to_owned());
            }
            "--env" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --env requires KEY=VALUE".to_string());
                };
                env.push(parse_env_assignment(value)?);
            }
            arg if let Some(value) = arg.strip_prefix("--env=") => {
                env.push(parse_env_assignment(value)?);
            }
            "--" => {
                let Some(path) = path else {
                    return Err("run requires <path.php> before `--`".to_string());
                };
                let trace_includes = trace_includes_enabled(&env);
                return Ok(RunOptions {
                    path,
                    script_args: args[index + 1..].to_vec(),
                    env,
                    stdin: Vec::new(),
                    debug,
                    debug_log,
                    error_format,
                    trace,
                    trace_runtime,
                    trace_includes,
                    counters_json,
                    timings_json,
                    region_profile_json,
                    bytecode_cache,
                    opt_level,
                    include_opt_level,
                    execution_format,
                    superinstructions,
                    last_use_moves,
                    reuse_class_context_frames,
                    dense_jump_threading,
                    bytecode_layout,
                    bytecode_layout_profile,
                    quickening,
                    inline_caches,
                    jit,
                    jit_explicit,
                    jit_threshold,
                    jit_blacklist,
                    jit_dump_clif,
                    jit_stats,
                    tiering,
                    adaptive_tiny_unit_setup_threshold,
                    tiering_stats_json,
                    persistent_feedback,
                });
            }
            arg if path.is_none() => path = Some(arg),
            unexpected => {
                return Err(format!(
                    "unexpected run argument `{unexpected}`; pass script arguments after `--`"
                ));
            }
        }
        index += 1;
    }
    let Some(path) = path else {
        return Err("run requires <path.php>".to_string());
    };
    let trace_includes = trace_includes_enabled(&env);
    Ok(RunOptions {
        path,
        script_args: Vec::new(),
        env,
        stdin: Vec::new(),
        debug,
        debug_log,
        error_format,
        trace,
        trace_runtime,
        trace_includes,
        counters_json,
        timings_json,
        region_profile_json,
        bytecode_cache,
        opt_level,
        include_opt_level,
        execution_format,
        superinstructions,
        last_use_moves,
        reuse_class_context_frames,
        dense_jump_threading,
        bytecode_layout,
        bytecode_layout_profile,
        quickening,
        inline_caches,
        jit,
        jit_explicit,
        jit_threshold,
        jit_blacklist,
        jit_dump_clif,
        jit_stats,
        tiering,
        adaptive_tiny_unit_setup_threshold,
        tiering_stats_json,
        persistent_feedback,
    })
}

pub(super) fn trace_includes_enabled(env: &[(String, String)]) -> bool {
    if let Some(value) = env
        .iter()
        .find_map(|(key, value)| (key == "PHRUST_TRACE_INCLUDES").then_some(value.as_str()))
    {
        return trace_includes_value_enabled(value);
    }
    std::env::var("PHRUST_TRACE_INCLUDES")
        .ok()
        .is_some_and(|value| trace_includes_value_enabled(&value))
}

pub(super) fn trace_includes_value_enabled(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(super) fn parse_on_off(value: &str, flag: &str) -> Result<bool, String> {
    match value {
        "off" => Ok(false),
        "on" => Ok(true),
        _ => Err(format!(
            "unsupported {flag} mode `{value}`; expected off or on"
        )),
    }
}

pub(super) fn parse_engine_preset(value: &str) -> Result<EngineProfileName, String> {
    EngineProfileName::parse(value).map_err(|error| error.to_string())
}

pub(super) fn parse_jit_blacklist_mode(value: &str) -> Result<JitBlacklistMode, String> {
    Ok(if parse_on_off(value, "jit-blacklist")? {
        JitBlacklistMode::On
    } else {
        JitBlacklistMode::Off
    })
}

pub(super) fn parse_u64_option(value: &str, flag: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("run --{flag} requires a non-negative integer"))
}

pub(super) fn parse_i64_option(value: &str, flag: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .map_err(|_| format!("run --{flag} requires an integer"))
}

pub(super) fn parse_quickening_mode(value: &str) -> Result<QuickeningMode, String> {
    match value {
        "off" => Ok(QuickeningMode::Off),
        "on" => Ok(QuickeningMode::On),
        _ => Err(format!(
            "unsupported quickening mode `{value}`; expected off or on"
        )),
    }
}

pub(super) fn parse_execution_format(value: &str) -> Result<ExecutionFormat, String> {
    match value {
        "ir" => Ok(ExecutionFormat::Ir),
        "auto" => Ok(ExecutionFormat::Auto),
        "bytecode" => Ok(ExecutionFormat::Bytecode),
        _ => Err(format!(
            "unsupported exec-format mode `{value}`; expected ir, auto, or bytecode"
        )),
    }
}

pub(super) fn parse_superinstruction_mode(value: &str) -> Result<SuperinstructionMode, String> {
    match value {
        "off" => Ok(SuperinstructionMode::Off),
        "on" => Ok(SuperinstructionMode::On),
        _ => Err(format!(
            "unsupported superinstructions mode `{value}`; expected off or on"
        )),
    }
}

pub(super) fn parse_last_use_moves_mode(value: &str) -> Result<bool, String> {
    match value {
        "off" => Ok(false),
        "on" => Ok(true),
        _ => Err(format!(
            "unsupported last-use-moves mode `{value}`; expected off or on"
        )),
    }
}

pub(super) fn parse_reuse_class_context_frames_mode(value: &str) -> Result<bool, String> {
    match value {
        "off" => Ok(false),
        "on" => Ok(true),
        _ => Err(format!(
            "unsupported reuse-class-context-frames mode `{value}`; expected off or on"
        )),
    }
}

fn parse_dense_jump_threading_mode(value: &str) -> Result<DenseJumpThreadingMode, String> {
    match value {
        "off" => Ok(DenseJumpThreadingMode::Off),
        "on" => Ok(DenseJumpThreadingMode::On),
        _ => Err(format!(
            "run --dense-jump-threading has unsupported mode `{value}` (accepted: off, on)"
        )),
    }
}

pub(super) fn parse_bytecode_layout_mode(value: &str) -> Result<BytecodeLayoutMode, String> {
    match value {
        "source" => Ok(BytecodeLayoutMode::Source),
        "profiled" => Ok(BytecodeLayoutMode::Profiled),
        _ => Err(format!(
            "unsupported bytecode-layout mode `{value}`; expected source or profiled"
        )),
    }
}

pub(super) fn parse_inline_cache_mode(value: &str) -> Result<InlineCacheMode, String> {
    match value {
        "off" => Ok(InlineCacheMode::Off),
        "on" => Ok(InlineCacheMode::On),
        _ => Err(format!(
            "unsupported inline-cache mode `{value}`; expected off or on"
        )),
    }
}

pub(super) fn parse_jit_mode(value: &str) -> Result<JitMode, String> {
    match value {
        "off" => Ok(JitMode::Off),
        "noop" => Ok(JitMode::Noop),
        "cranelift" => Ok(JitMode::Cranelift),
        _ => Err(format!(
            "unsupported jit mode `{value}`; expected off, noop, or cranelift"
        )),
    }
}

pub(super) fn parse_jit_stats_mode(value: &str) -> Result<JitStatsMode, String> {
    match value {
        "json" => Ok(JitStatsMode::Json),
        _ => Err(format!(
            "unsupported jit stats mode `{value}`; expected json"
        )),
    }
}

pub(super) fn parse_bytecode_cache_mode(value: &str) -> Result<BytecodeCacheMode, String> {
    match value {
        "off" => Ok(BytecodeCacheMode::Off),
        "read" => Ok(BytecodeCacheMode::Read),
        "write" => Ok(BytecodeCacheMode::Write),
        "read-write" => Ok(BytecodeCacheMode::ReadWrite),
        _ => Err(format!(
            "unsupported bytecode cache mode `{value}`; expected off, read, write, or read-write"
        )),
    }
}

pub(super) fn parse_optimization_level(value: &str) -> Result<OptimizationLevel, String> {
    value
        .parse()
        .map_err(|error: php_optimizer::ParseOptimizationLevelError| error.to_string())
}
