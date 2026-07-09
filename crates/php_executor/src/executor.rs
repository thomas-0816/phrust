use crate::diagnostics::{
    execution_output_from_vm, frontend_diagnostic_envelopes, render_frontend_diagnostics,
};
use crate::input::{
    PhpCompileInput, PhpExecutionError, PhpExecutionInput, PhpExecutionOutput, PhpExecutionStatus,
    PhpExecutorOptions, PhpRequestExecutionInput,
};
use crate::pipeline::{
    CompilePhaseTimings, Pipeline, apply_optimization, apply_optimization_with_timings,
    compile_source, compile_source_with_timings,
};
use crate::request::include_loader_for_request;
use php_runtime::api::{FilesystemCapabilities, RuntimeHttpResponseState};
use php_source::SourceText;
use php_vm::api::{CompiledUnit, Vm, VmOptions};

/// Transport-independent PHP executor.
#[derive(Clone, Debug, Default)]
pub struct PhpExecutor {
    options: PhpExecutorOptions,
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
        Self { options }
    }

    /// Compiles source into a reusable artifact.
    pub fn compile_source(
        &self,
        input: PhpCompileInput,
    ) -> Result<CompiledPhpScript, PhpExecutionError> {
        let mut pipeline =
            compile_source(&input.source, &input.source_path).map_err(PhpExecutionError::Engine)?;
        apply_optimization(
            &mut pipeline,
            input
                .optimization_level
                .unwrap_or(self.options.optimization_level),
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
        Ok(CompiledPhpScript::new(pipeline))
    }

    /// Compiles source into a reusable artifact and returns internal phase timings.
    pub fn compile_source_with_timings(
        &self,
        input: PhpCompileInput,
    ) -> Result<(CompiledPhpScript, CompilePhaseTimings), PhpExecutionError> {
        let (mut pipeline, mut timings) =
            compile_source_with_timings(&input.source, &input.source_path)
                .map_err(PhpExecutionError::Engine)?;
        let optimization_timings = apply_optimization_with_timings(
            &mut pipeline,
            input
                .optimization_level
                .unwrap_or(self.options.optimization_level),
        )
        .map_err(PhpExecutionError::Engine)?;
        timings.phases.extend(
            optimization_timings
                .phases()
                .iter()
                .map(|(key, value)| (key.clone(), *value)),
        );
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
        Ok((CompiledPhpScript::new(pipeline), timings))
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
        let vm = Vm::with_options(VmOptions {
            include_loader,
            runtime_context,
            collect_counters: input.collect_counters,
            collect_profile_spans: input.collect_profile_spans,
            collect_layout_source_attribution: input.collect_layout_source_attribution,
            ..self.options.vm_options.clone()
        });
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
        Self {
            path,
            source,
            executable: CompiledUnit::new(lowering.unit),
        }
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
        Self {
            path: source_path.into(),
            source: SourceText::new(source),
            executable: CompiledUnit::new(unit),
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
        ExecutionFormat, InlineCacheMode, JitMode, QuickeningMode, SuperinstructionMode,
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
            executor.options.vm_options.include_optimization_level,
            expected.vm_options.include_optimization_level
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
        assert_eq!(executor.options.vm_options.jit, JitMode::Off);
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
        assert_eq!(counters.native_executions, counters.jit_executed);
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
