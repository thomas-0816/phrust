use crate::diagnostics::{
    execution_output_from_vm, frontend_diagnostic_envelopes, render_frontend_diagnostics,
};
use crate::include_compiler::ExecutorIncludeCompiler;
use crate::input::{
    PhpCompileInput, PhpExecutionError, PhpExecutionInput, PhpExecutionOutput, PhpExecutionStatus,
    PhpExecutorOptions, PhpRequestExecutionInput,
};
use crate::pipeline::{
    CompilePhaseTimings, CompileTimingCollector, Pipeline, apply_optimization, compile_source,
};
use crate::request::include_loader_for_request;
use php_runtime::api::{FilesystemCapabilities, RuntimeHttpResponseState};
use php_source::SourceText;
use php_vm::api::{CompiledUnit, Vm, VmOptions, VmWorkerState};

/// Transport-independent PHP executor.
#[derive(Clone, Debug, Default)]
pub struct PhpExecutor {
    options: PhpExecutorOptions,
    worker_state: VmWorkerState,
}

impl PhpExecutor {
    /// Creates an executor with default VM and optimizer options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an executor with explicit defaults.
    #[must_use]
    pub fn with_options(options: PhpExecutorOptions) -> Self {
        let worker_state = VmWorkerState::new(options.vm_options.tiering.clone());
        Self {
            options,
            worker_state,
        }
    }

    /// Replaces request policy while retaining engine-owned worker caches.
    ///
    /// Persistent feedback seeds vary by compiled script and request, but must
    /// not evict JIT handles, tiering hotness, or other worker-stable caches.
    pub fn reconfigure(&mut self, options: PhpExecutorOptions) {
        self.options = options;
    }

    /// Compiles source into a reusable artifact.
    pub fn compile_source(
        &self,
        input: PhpCompileInput,
    ) -> Result<CompiledPhpScript, PhpExecutionError> {
        self.compile_source_internal(input, CompileTimingCollector::disabled())
            .map(|result| result.compiled)
    }

    /// Compiles source into a reusable artifact and returns internal phase timings.
    pub fn compile_source_with_timings(
        &self,
        input: PhpCompileInput,
    ) -> Result<(CompiledPhpScript, CompilePhaseTimings), PhpExecutionError> {
        let result = self.compile_source_internal(input, CompileTimingCollector::enabled())?;
        let timings = result.timings.ok_or_else(|| {
            PhpExecutionError::Engine("enabled compile timing collector returned no report".into())
        })?;
        Ok((result.compiled, timings))
    }

    fn compile_source_internal(
        &self,
        input: PhpCompileInput,
        mut timings: CompileTimingCollector,
    ) -> Result<CompilationResult, PhpExecutionError> {
        let mut pipeline = compile_source(&input.source, &input.source_path, &mut timings)
            .map_err(PhpExecutionError::Engine)?;
        apply_optimization(
            &mut pipeline,
            input
                .optimization_level
                .unwrap_or(self.options.optimization_level),
            &mut timings,
        )
        .map_err(PhpExecutionError::Engine)?;
        if !pipeline.ok() {
            let diagnostics_text =
                render_frontend_diagnostics(&pipeline).map_err(PhpExecutionError::Engine)?;
            return Err(PhpExecutionError::Compile(Box::new(PhpExecutionOutput {
                stdout: Vec::new(),
                diagnostics_text,
                diagnostics: frontend_diagnostic_envelopes(&pipeline),
                status: PhpExecutionStatus::CompileError,
                runtime_diagnostics: Vec::new(),
                http_response: RuntimeHttpResponseState::default(),
                upload_registry: Default::default(),
                session: Default::default(),
                return_value: None,
                trace: Vec::new(),
                counters: None,
                tiering_stats: None,
                quickening_feedback: Vec::new(),
                callsite_feedback: Vec::new(),
                persistent_feedback_epochs: None,
            })));
        }
        Ok(CompilationResult {
            compiled: CompiledPhpScript::new(pipeline),
            timings: timings.finish(),
        })
    }

    /// Executes a previously compiled script with per-request runtime context.
    #[must_use]
    pub fn execute_compiled(
        &self,
        compiled: &CompiledPhpScript,
        input: PhpRequestExecutionInput,
    ) -> PhpExecutionOutput {
        let include_loader = match include_loader_for_request(&input) {
            Ok(loader) => loader,
            Err(error) => {
                return PhpExecutionOutput::engine_error(error);
            }
        };
        let mut runtime_context = input.runtime_context;
        let mut capabilities = FilesystemCapabilities::none().with_stdio(true);
        if let Some(loader) = &include_loader {
            capabilities = capabilities.with_allowed_roots(loader.allowed_roots().to_vec());
        }
        runtime_context = runtime_context.with_filesystem_capabilities(capabilities);
        let vm = Vm::with_options_and_worker_state(
            VmOptions {
                include_loader,
                include_compiler: Some(std::sync::Arc::new(ExecutorIncludeCompiler::new(
                    self.options.include_optimization_level,
                ))),
                runtime_context,
                collect_counters: input.collect_counters,
                collect_profile_spans: input.collect_profile_spans,
                collect_layout_source_attribution: input.collect_layout_source_attribution,
                ..self.options.vm_options.clone()
            },
            self.worker_state.clone(),
        );
        let result = vm.execute(compiled.executable_unit());
        let (quickening_feedback, callsite_feedback, persistent_feedback_epochs) =
            if self.options.collect_quickening_feedback {
                (
                    vm.export_persistent_quickening(),
                    vm.export_persistent_function_callsites(),
                    vm.export_persistent_feedback_epochs(),
                )
            } else {
                (Vec::new(), Vec::new(), None)
            };
        let mut output = execution_output_from_vm(&compiled.path, &compiled.source, result);
        output.quickening_feedback = quickening_feedback;
        output.callsite_feedback = callsite_feedback;
        output.persistent_feedback_epochs = persistent_feedback_epochs;
        output
    }

    /// Performs bounded native prewarming without executing application code.
    /// Returns the number of newly adopted/compiled Cranelift entries.
    #[must_use]
    pub fn prewarm_compiled(&self, compiled: &CompiledPhpScript) -> u64 {
        let vm = Vm::with_options_and_worker_state(
            self.options.vm_options.clone(),
            self.worker_state.clone(),
        );
        vm.prewarm_cranelift(&compiled.executable_unit())
    }

    /// Compiles and executes source in one step.
    #[must_use]
    pub fn execute_source(&self, input: PhpExecutionInput) -> PhpExecutionOutput {
        let compiled = match self.compile_source(PhpCompileInput {
            source: input.source,
            source_path: input.source_path,
            optimization_level: input.optimization_level,
        }) {
            Ok(compiled) => compiled,
            Err(PhpExecutionError::Compile(output)) => return *output,
            Err(PhpExecutionError::Engine(error)) => {
                return PhpExecutionOutput::engine_error(error);
            }
        };
        self.execute_compiled(
            &compiled,
            PhpRequestExecutionInput {
                real_path: input.real_path,
                cwd: input.cwd,
                include_roots: input.include_roots,
                runtime_context: input.runtime_context,
                collect_counters: input.collect_counters,
                collect_profile_spans: input.collect_profile_spans,
                collect_layout_source_attribution: input.collect_layout_source_attribution,
            },
        )
    }
}

struct CompilationResult {
    compiled: CompiledPhpScript,
    timings: Option<CompilePhaseTimings>,
}

/// Compiled, reusable PHP script artifact.
#[derive(Clone, Debug)]
pub struct CompiledPhpScript {
    pub(crate) path: String,
    pub(crate) source: SourceText,
    executable: CompiledUnit,
}

impl CompiledPhpScript {
    fn new(pipeline: Pipeline) -> Self {
        let Pipeline {
            path,
            source,
            lowering,
            ..
        } = pipeline;
        let retained_source = source.shared_text();
        Self {
            path,
            source,
            executable: CompiledUnit::with_ordered_sources(lowering.unit, [retained_source]),
        }
    }

    /// Stable source path used to scope request-persistent engine feedback.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Rehydrates a compiled script from an externally cached IR unit.
    ///
    /// The unit must come from a prior compile of exactly this source;
    /// cache fingerprint validation is the caller's responsibility.
    #[must_use]
    pub fn from_cached_ir_unit(
        source_path: impl Into<String>,
        source: impl Into<String>,
        unit: php_ir::module::IrUnit,
    ) -> Self {
        let path = source_path.into();
        let source = SourceText::new(source);
        let retained_source = source.shared_text();
        Self {
            path,
            source,
            executable: CompiledUnit::with_ordered_sources(unit, [retained_source]),
        }
    }

    /// Returns the lowered IR unit for CLI/reporting adapters that need metadata.
    #[must_use]
    pub fn ir_unit(&self) -> &php_ir::module::IrUnit {
        self.executable.unit()
    }

    /// Returns the reusable VM-facing executable unit.
    #[must_use]
    pub fn executable_unit(&self) -> CompiledUnit {
        self.executable.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PhpExecutionStatus, PhpExecutorOptions};
    use php_optimizer::OptimizationLevel;
    use php_runtime::api::RuntimeContext;
    use php_vm::api::{
        ExecutionFormat, InlineCacheMode, NativeOptimizationPolicy, QuickeningMode,
        SuperinstructionMode,
    };

    #[test]
    fn new_uses_managed_fast_runtime() {
        let executor = PhpExecutor::new();
        let expected = PhpExecutorOptions::managed_fast_runtime();

        assert_eq!(executor.options.optimization_level, OptimizationLevel::O2);
        assert_eq!(
            executor.options.optimization_level,
            expected.optimization_level
        );
        assert_eq!(
            executor.options.include_optimization_level,
            expected.include_optimization_level
        );
        assert_eq!(
            executor.options.vm_options.execution_format,
            ExecutionFormat::Auto
        );
        assert_eq!(
            executor.options.vm_options.superinstructions,
            SuperinstructionMode::On
        );
        assert_eq!(executor.options.vm_options.quickening, QuickeningMode::On);
        assert_eq!(
            executor.options.vm_options.inline_caches,
            InlineCacheMode::On
        );
        assert_eq!(
            executor.options.vm_options.native_optimization,
            NativeOptimizationPolicy::Baseline
        );
    }

    #[test]
    fn compilation_is_equivalent_with_timings_enabled_or_disabled() {
        let executor = PhpExecutor::new();
        let input = PhpCompileInput {
            source: "<?php function add($a, $b) { return $a + $b; } echo add(2, 3);".to_owned(),
            source_path: "timing-equivalence.php".to_owned(),
            optimization_level: Some(OptimizationLevel::O2),
        };

        let untimed = executor
            .compile_source(input.clone())
            .expect("untimed compilation");
        let (timed, timings) = executor
            .compile_source_with_timings(input)
            .expect("timed compilation");

        assert_eq!(untimed.ir_unit(), timed.ir_unit());
        assert_eq!(
            timings
                .phases()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [
                "frontend_analyze_ms",
                "ir_lower_ms",
                "ir_verify_ms",
                "optimizer_ms",
            ]
        );
    }

    #[test]
    fn compilation_diagnostics_are_equal_with_timings_enabled_or_disabled() {
        let executor = PhpExecutor::new();
        let input = PhpCompileInput {
            source: "<?php function broken( {".to_owned(),
            source_path: "timing-diagnostics.php".to_owned(),
            optimization_level: Some(OptimizationLevel::O2),
        };

        let untimed = executor
            .compile_source(input.clone())
            .expect_err("invalid source must fail");
        let timed = executor
            .compile_source_with_timings(input)
            .expect_err("invalid source must fail");

        assert_eq!(untimed, timed);
    }

    #[test]
    fn execute_source_default_uses_managed_fast_path_counters() {
        let executor = PhpExecutor::new();
        let output = executor.execute_source(PhpExecutionInput {
            source: managed_fast_counter_source().to_string(),
            source_path: "managed-fast-counter.php".to_string(),
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli("managed-fast-counter.php", Vec::new()),
            optimization_level: None,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
        });

        assert_eq!(output.status, PhpExecutionStatus::Success);
        assert_eq!(output.stdout, b"123512351235");
        let counters = output.counters.expect("counters should be collected");
        assert_eq!(counters.jit_mode, "off");
        assert_eq!(counters.jit_executed, 0);
        assert!(counters.bytecode_lower_attempts > 0, "{counters:?}");
        assert!(counters.quickening_attempts > 0, "{counters:?}");
        assert!(counters.inline_cache_observations > 0, "{counters:?}");
        assert!(
            counters.function_profiles_by_name.is_empty(),
            "{counters:?}"
        );
        assert!(counters.method_profiles_by_name.is_empty(), "{counters:?}");
        assert!(counters.builtin_profiles_by_name.is_empty(), "{counters:?}");
        assert!(
            counters.array_operation_profiles_by_family.is_empty(),
            "{counters:?}"
        );
    }

    #[test]
    fn execute_source_default_executes_superinstructions() {
        let executor = PhpExecutor::new();
        let output = executor.execute_source(PhpExecutionInput {
            source: default_superinstruction_source().to_string(),
            source_path: "default-superinstructions.php".to_string(),
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli(
                "default-superinstructions.php",
                Vec::new(),
            ),
            optimization_level: None,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
        });

        assert_eq!(output.status, PhpExecutionStatus::Success);
        assert_eq!(output.stdout, b"hello worldab");
        let counters = output.counters.expect("counters should be collected");
        assert!(counters.bytecode_lower_attempts > 0, "{counters:?}");
        assert!(counters.bytecode_lower_successes > 0, "{counters:?}");
        assert!(counters.superinstruction_candidates > 0, "{counters:?}");
        assert!(counters.superinstructions_emitted > 0, "{counters:?}");
        assert!(
            counters.superinstructions_executed.values().sum::<u64>() > 0,
            "{counters:?}"
        );
        assert!(
            counters
                .superinstructions_executed
                .contains_key("load_const_echo"),
            "{counters:?}"
        );
    }

    #[test]
    fn execute_compiled_reuses_vm_executable_handle() {
        let executor = PhpExecutor::new();
        let compiled = executor
            .compile_source(PhpCompileInput {
                source: "<?php echo 1 + 2;".to_string(),
                source_path: "compiled-handle.php".to_string(),
                optimization_level: Some(OptimizationLevel::O0),
            })
            .expect("compile reusable script");
        let before = compiled.executable_unit();

        let first = executor.execute_compiled(
            &compiled,
            PhpRequestExecutionInput {
                real_path: None,
                cwd: std::env::current_dir().expect("current directory"),
                include_roots: Vec::new(),
                runtime_context: RuntimeContext::controlled_cli("compiled-handle.php", Vec::new()),
                collect_counters: false,
                collect_profile_spans: false,
                collect_layout_source_attribution: false,
            },
        );
        let second = executor.execute_compiled(
            &compiled,
            PhpRequestExecutionInput {
                real_path: None,
                cwd: std::env::current_dir().expect("current directory"),
                include_roots: Vec::new(),
                runtime_context: RuntimeContext::controlled_cli("compiled-handle.php", Vec::new()),
                collect_counters: false,
                collect_profile_spans: false,
                collect_layout_source_attribution: false,
            },
        );
        let after = compiled.executable_unit();

        assert_eq!(first.status, PhpExecutionStatus::Success);
        assert_eq!(second.status, PhpExecutionStatus::Success);
        assert_eq!(first.stdout, b"3");
        assert_eq!(second.stdout, b"3");
        assert!(before.ptr_eq(&after));
    }

    #[test]
    fn execute_compiled_reuses_worker_builtin_dispatch_cache() {
        let executor = PhpExecutor::new();
        let compiled = executor
            .compile_source(PhpCompileInput {
                source: "<?php echo array_sum([1, 2, 3]);".to_string(),
                source_path: "worker-cache.php".to_string(),
                optimization_level: Some(OptimizationLevel::O0),
            })
            .expect("compile reusable script");
        let input = || PhpRequestExecutionInput {
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli("worker-cache.php", Vec::new()),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
        };

        let first = executor.execute_compiled(&compiled, input());
        let second = executor.execute_compiled(&compiled, input());
        assert_eq!(first.stdout, b"6");
        assert_eq!(second.stdout, b"6");
        let first = first.counters.expect("first counters");
        let second = second.counters.expect("second counters");
        assert!(first.internal_function_dispatch_cache_misses > 0);
        assert!(second.internal_function_dispatch_cache_hits > 0);
    }

    #[test]
    fn execute_compiled_reuses_guarded_worker_ic_and_class_metadata() {
        let mut options = PhpExecutorOptions::managed_fast_runtime();
        options.vm_options.execution_format = ExecutionFormat::Ir;
        let executor = PhpExecutor::with_options(options);
        let compiled = executor
            .compile_source(PhpCompileInput {
                source: "<?php
                    function worker_f(int $x): int { return $x + 1; }
                    class WorkerCacheSubject {
                        public int $value = 2;
                        public function __construct() {}
                        public function read(): int { return $this->value; }
                    }
                    for ($i = 0; $i < 4; $i++) {
                        $object = new WorkerCacheSubject();
                        echo worker_f($object->read());
                    }"
                .to_owned(),
                source_path: "worker-metadata-cache.php".to_owned(),
                optimization_level: Some(OptimizationLevel::O0),
            })
            .expect("compile worker metadata cache script");
        let input = || PhpRequestExecutionInput {
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli(
                "worker-metadata-cache.php",
                Vec::new(),
            ),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
        };

        let first = executor.execute_compiled(&compiled, input());
        let second = executor.execute_compiled(&compiled, input());
        assert_eq!(first.status, PhpExecutionStatus::Success);
        assert_eq!(second.status, PhpExecutionStatus::Success);
        assert_eq!(first.stdout, b"3333");
        assert_eq!(second.stdout, first.stdout);
        let counters = second.counters.expect("second counters");
        assert!(
            counters
                .persistent_worker_ic_hits_by_family
                .values()
                .sum::<u64>()
                > 0,
            "{counters:?}"
        );
        assert!(
            counters.persistent_worker_class_cache_hits > 0,
            "{counters:?}"
        );
        assert!(
            counters.persistent_worker_default_slot_template_hits > 0,
            "{counters:?}"
        );
        assert!(
            counters.persistent_worker_constructor_hits > 0,
            "{counters:?}"
        );
        assert!(
            counters
                .persistent_worker_request_visible_rejections_by_family
                .is_empty(),
            "{counters:?}"
        );
    }

    #[test]
    fn execute_compiled_reuses_worker_quickening_without_snapshots() {
        let mut options = PhpExecutorOptions::managed_fast_runtime();
        options.vm_options.execution_format = ExecutionFormat::Bytecode;
        options.vm_options.persistent_adaptive_state = true;
        let executor = PhpExecutor::with_options(options);
        let compiled = executor
            .compile_source(PhpCompileInput {
                source: "<?php $sum = 0; for ($i = 0; $i < 20; $i++) { $sum += $i; } echo $sum;"
                    .to_owned(),
                source_path: "worker-quickening.php".to_owned(),
                optimization_level: Some(OptimizationLevel::O0),
            })
            .expect("compile worker quickening script");
        let input = || PhpRequestExecutionInput {
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli("worker-quickening.php", Vec::new()),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
        };

        let first = executor.execute_compiled(&compiled, input());
        let second = executor.execute_compiled(&compiled, input());
        assert_eq!(first.stdout, b"190");
        assert_eq!(second.stdout, first.stdout);
        assert_eq!(
            first
                .counters
                .expect("first counters")
                .persistent_worker_quickening_reused_sites,
            0
        );
        assert!(
            second
                .counters
                .expect("second counters")
                .persistent_worker_quickening_reused_sites
                > 0
        );
        assert!(second.quickening_feedback.is_empty());
        assert!(second.callsite_feedback.is_empty());
    }

    #[test]
    fn worker_quickening_kill_switch_and_generation_key_isolate_state() {
        let mut options = PhpExecutorOptions::managed_fast_runtime();
        options.vm_options.execution_format = ExecutionFormat::Bytecode;
        options.vm_options.persistent_adaptive_state = false;
        let executor = PhpExecutor::with_options(options);
        let compile = |suffix: &str| {
            executor
                .compile_source(PhpCompileInput {
                    source: format!(
                        "<?php $sum = 0; for ($i = 0; $i < 20; $i++) {{ $sum += $i; }} echo $sum, '{suffix}';"
                    ),
                    source_path: "worker-quickening-generation.php".to_owned(),
                    optimization_level: Some(OptimizationLevel::O0),
                })
                .expect("compile worker quickening generation")
        };
        let input = || PhpRequestExecutionInput {
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli(
                "worker-quickening-generation.php",
                Vec::new(),
            ),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
        };

        let first = compile("a");
        let _ = executor.execute_compiled(&first, input());
        let repeated = executor.execute_compiled(&first, input());
        assert_eq!(repeated.stdout, b"190a");
        assert_eq!(
            repeated
                .counters
                .expect("kill switch counters")
                .persistent_worker_quickening_reused_sites,
            0
        );

        let mut enabled = PhpExecutorOptions::managed_fast_runtime();
        enabled.vm_options.execution_format = ExecutionFormat::Bytecode;
        enabled.vm_options.persistent_adaptive_state = true;
        let enabled_executor = PhpExecutor::with_options(enabled);
        let replacement = enabled_executor
            .compile_source(PhpCompileInput {
                source:
                    "<?php $sum = 0; for ($i = 0; $i < 20; $i++) { $sum += $i; } echo $sum, 'b';"
                        .to_owned(),
                source_path: "worker-quickening-generation.php".to_owned(),
                optimization_level: Some(OptimizationLevel::O0),
            })
            .expect("compile replacement generation");
        let fresh = enabled_executor.execute_compiled(&replacement, input());
        assert_eq!(fresh.stdout, b"190b");
        assert_eq!(
            fresh
                .counters
                .expect("replacement counters")
                .persistent_worker_quickening_reused_sites,
            0
        );
    }

    #[test]
    fn worker_quickening_returns_to_worker_after_failed_request() {
        let mut options = PhpExecutorOptions::managed_fast_runtime();
        options.vm_options.execution_format = ExecutionFormat::Bytecode;
        options.vm_options.persistent_adaptive_state = true;
        let executor = PhpExecutor::with_options(options);
        let compiled = executor
            .compile_source(PhpCompileInput {
                source: "<?php $sum = 0; for ($i = 0; $i < 20; $i++) { $sum += $i; } missing_worker_function();"
                    .to_owned(),
                source_path: "worker-quickening-failure.php".to_owned(),
                optimization_level: Some(OptimizationLevel::O0),
            })
            .expect("compile failing worker quickening script");
        let input = || PhpRequestExecutionInput {
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli(
                "worker-quickening-failure.php",
                Vec::new(),
            ),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
        };

        let first = executor.execute_compiled(&compiled, input());
        let second = executor.execute_compiled(&compiled, input());
        assert_eq!(first.status, PhpExecutionStatus::RuntimeError);
        assert_eq!(second.status, first.status);
        assert!(
            second
                .counters
                .expect("failed request counters")
                .persistent_worker_quickening_reused_sites
                > 0
        );
    }

    #[test]
    fn worker_class_cache_invalidates_recompiled_units_and_rejects_heap_defaults() {
        let mut options = PhpExecutorOptions::managed_fast_runtime();
        options.vm_options.execution_format = ExecutionFormat::Ir;
        let executor = PhpExecutor::with_options(options);
        let compile = |default: &str| {
            executor
                .compile_source(PhpCompileInput {
                    source: format!(
                        "<?php class RecompiledSubject {{ public $value = {default}; }} \
                         echo (new RecompiledSubject())->value;"
                    ),
                    source_path: "recompiled-worker-cache.php".to_owned(),
                    optimization_level: Some(OptimizationLevel::O0),
                })
                .expect("compile cache generation")
        };
        let input = || PhpRequestExecutionInput {
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli(
                "recompiled-worker-cache.php",
                Vec::new(),
            ),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
        };

        let first = compile("1");
        assert_eq!(executor.execute_compiled(&first, input()).stdout, b"1");
        let replacement = compile("2");
        let replaced = executor.execute_compiled(&replacement, input());
        assert_eq!(replaced.stdout, b"2");
        assert!(
            replaced
                .counters
                .expect("replacement counters")
                .persistent_worker_invalidations_by_reason
                .contains_key("compiled_unit_identity")
        );

        let heap_default = compile("'request-visible'");
        let rejected = executor.execute_compiled(&heap_default, input());
        assert_eq!(rejected.stdout, b"request-visible");
        let counters = rejected.counters.expect("heap-default counters");
        assert!(
            counters
                .persistent_worker_request_visible_rejections_by_family
                .contains_key("runtime_class_entry"),
            "{counters:?}"
        );
        assert!(
            counters
                .persistent_worker_request_visible_rejections_by_family
                .contains_key("default_slot_template"),
            "{counters:?}"
        );
    }

    #[test]
    fn execute_compiled_reuses_worker_cranelift_compile_cache() {
        let mut options = PhpExecutorOptions::managed_fast_runtime();
        // Exercise the production Auto path and reuse the worker-owned native
        // handle on the second request.
        options.vm_options.execution_format = ExecutionFormat::Auto;
        options.vm_options.native_optimization = NativeOptimizationPolicy::Optimizing;
        options.vm_options.tiering.jit_eager = true;
        let executor = PhpExecutor::with_options(options);
        let compiled = executor
            .compile_source(PhpCompileInput {
                source:
                    "<?php function add(int $a, int $b): int { return $a + $b; } echo add(1, 2);"
                        .to_owned(),
                source_path: "worker-jit-cache.php".to_owned(),
                optimization_level: Some(OptimizationLevel::O0),
            })
            .expect("compile reusable JIT script");
        let input = || PhpRequestExecutionInput {
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli("worker-jit-cache.php", Vec::new()),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
        };

        let first = executor.execute_compiled(&compiled, input());
        let second = executor.execute_compiled(&compiled, input());
        assert_eq!(first.stdout, b"3");
        assert_eq!(second.stdout, b"3");
        let first = first.counters.expect("first counters");
        let second = second.counters.expect("second counters");
        assert!(first.jit_compile_cache_misses > 0, "{first:?}");
        assert!(second.jit_compile_cache_hits > 0, "{second:?}");
    }

    #[test]
    fn bounded_cranelift_prewarm_populates_cache_without_executing_script() {
        let mut options = PhpExecutorOptions::managed_fast_runtime();
        options.vm_options.execution_format = ExecutionFormat::Auto;
        options.vm_options.native_optimization = NativeOptimizationPolicy::Optimizing;
        options.vm_options.tiering.jit_eager = true;
        let executor = PhpExecutor::with_options(options);
        let compiled = executor
            .compile_source(PhpCompileInput {
                source:
                    "<?php function prewarmed(int $a): int { return $a + 1; } echo prewarmed(4);"
                        .to_owned(),
                source_path: "cranelift-prewarm.php".to_owned(),
                optimization_level: Some(OptimizationLevel::O0),
            })
            .expect("compile prewarm fixture");

        assert!(executor.prewarm_compiled(&compiled) > 0);
        let output = executor.execute_compiled(
            &compiled,
            PhpRequestExecutionInput {
                real_path: None,
                cwd: std::env::current_dir().expect("current directory"),
                include_roots: Vec::new(),
                runtime_context: RuntimeContext::controlled_cli(
                    "cranelift-prewarm.php",
                    Vec::new(),
                ),
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: false,
            },
        );

        assert_eq!(output.stdout, b"5");
        let counters = output.counters.expect("prewarmed request counters");
        assert!(counters.jit_compile_cache_hits > 0, "{counters:?}");
        assert_eq!(counters.jit_compile_attempts, 0, "{counters:?}");
    }

    fn managed_fast_counter_source() -> &'static str {
        "<?php\n\
         function ic_f() { return 1; }\n\
         class ICSlotSmoke {\n\
             public $x = 3;\n\
             public function m() { return 2; }\n\
         }\n\
         $object = new ICSlotSmoke();\n\
         $items = [4, 5];\n\
         for ($i = 0; $i < 3; $i = $i + 1) {\n\
             echo ic_f(), $object->m(), $object->x, $items[1];\n\
         }\n"
    }

    fn default_superinstruction_source() -> &'static str {
        "<?php\n\
         $name = \"world\";\n\
         echo \"hello \";\n\
         echo $name;\n\
         echo \"a\" . \"b\";\n"
    }
}
