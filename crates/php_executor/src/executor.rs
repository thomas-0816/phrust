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
use php_vm::api::{CompiledUnit, NativeCompileCacheStats, Vm, VmOptions, VmWorkerState};

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
        Self::with_options_and_worker_state(options, worker_state)
    }

    /// Creates an executor backed by process-owned immutable native caches.
    #[must_use]
    pub fn with_options_and_worker_state(
        options: PhpExecutorOptions,
        worker_state: VmWorkerState,
    ) -> Self {
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

    /// Returns process-worker native compile cache counters.
    #[must_use]
    pub fn native_compile_cache_stats(&self) -> NativeCompileCacheStats {
        self.worker_state.native_compile_cache_stats()
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
                native_cache_stats: None,
                native_cache_load_nanos: 0,
                native_compile_nanos: 0,
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
        let allow_standard_devices = runtime_context.filesystem.allows_standard_devices();
        let mut capabilities = FilesystemCapabilities::none()
            .with_stdio(true)
            .with_standard_devices(allow_standard_devices);
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
                ..self.options.vm_options.clone()
            },
            self.worker_state.clone(),
        );
        let result = vm.execute(compiled.executable_unit());
        execution_output_from_vm(&compiled.path, &compiled.source, result)
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
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrConstant, IrReturnType, IrSpan, UnitId};
    use php_ir::{InstructionKind, Operand};

    #[test]
    fn native_builtin_arginfo_errors_are_php_catchable() {
        let output = PhpExecutor::default().execute_source(PhpExecutionInput {
            source: r#"<?php
try {
    abs();
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    json_decode([]);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    clearstatcache(__phrust_probe_unknown: 1);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
echo function_exists('class_alias') ? "class_alias:available\n" : "class_alias:missing\n";
echo function_exists('print') ? "print:available\n" : "print:missing\n";
"#
            .to_owned(),
            source_path: "builtin-arginfo.php".to_owned(),
            real_path: None,
            cwd: std::path::PathBuf::from("."),
            include_roots: Vec::new(),
            runtime_context: php_runtime::api::RuntimeContext::default(),
            optimization_level: None,
            collect_counters: false,
        });
        assert_eq!(
            output.status,
            PhpExecutionStatus::Success,
            "stdout={} diagnostics={}",
            String::from_utf8_lossy(&output.stdout),
            output.diagnostics_text
        );
        assert_eq!(
            output.stdout,
            b"ArgumentCountError:abs() expects exactly 1 argument, 0 given\n\
TypeError:json_decode(): Argument #1 ($json) must be of type string, array given\n\
Error:Unknown named parameter $__phrust_probe_unknown\n\
class_alias:available\n\
print:missing\n"
        );
    }

    #[test]
    fn native_internal_api_introspection_uses_generated_hierarchy() {
        let output = PhpExecutor::default().execute_source(PhpExecutionInput {
            source: r#"<?php
echo method_exists('ArgumentCountError', 'getMessage') ? "error-method\n" : "missing\n";
echo method_exists('AppendIterator', 'getInnerIterator') ? "spl-method\n" : "missing\n";
echo defined('ReflectionClass::IS_IMPLICIT_ABSTRACT') ? "class-constant\n" : "missing\n";
$reflection = new ReflectionClass('ReflectionObject');
echo $reflection->hasProperty('name') ? "inherited-property\n" : "missing\n";
$errorReflection = new ReflectionClass('Error');
echo $errorReflection->hasProperty('message') ? "error-property\n" : "missing\n";
echo (new ReflectionClass('Error'))->hasProperty('message') ? "temporary-property\n" : "missing\n";
$class = 'Error';
$member = 'message';
echo (new ReflectionClass($class))->getName(), "\n";
echo (new ReflectionClass($class))->hasProperty($member) ? "variable-property\n" : "missing\n";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $classAvailable && (new ReflectionClass($class))->hasProperty($member) ? "probe-property\n" : "missing\n";
"#
            .to_owned(),
            source_path: "internal-api-introspection.php".to_owned(),
            real_path: None,
            cwd: std::path::PathBuf::from("."),
            include_roots: Vec::new(),
            runtime_context: php_runtime::api::RuntimeContext::default(),
            optimization_level: None,
            collect_counters: false,
        });
        assert_eq!(
            output.status,
            PhpExecutionStatus::Success,
            "stdout={} diagnostics={}",
            String::from_utf8_lossy(&output.stdout),
            output.diagnostics_text
        );
        assert_eq!(
            output.stdout,
            b"error-method\nspl-method\nclass-constant\ninherited-property\nerror-property\ntemporary-property\nError\nvariable-property\nprobe-property\n"
        );
    }

    #[test]
    fn bounded_cranelift_prewarm_populates_cache_without_executing_script() {
        let mut builder = IrBuilder::new(UnitId::new(91));
        let file = builder.add_file("prewarm.php");
        let span = IrSpan::new(file, 0, 18);
        let constant = builder.intern_constant(IrConstant::Int(42));
        let function = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        let value = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadConst {
                dst: value,
                constant,
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(value)), span);
        builder.set_entry(function);
        let compiled = CompiledPhpScript::from_cached_ir_unit(
            "prewarm.php",
            "<?php return 42;",
            builder.finish(),
        );
        let executor = PhpExecutor::default();
        assert!(executor.prewarm_compiled(&compiled) > 0);

        let worker = VmWorkerState::new(php_vm::api::TieringOptions::default());
        let first = PhpExecutor::with_options_and_worker_state(
            PhpExecutorOptions::default(),
            worker.clone(),
        );
        let second =
            PhpExecutor::with_options_and_worker_state(PhpExecutorOptions::default(), worker);
        assert!(first.prewarm_compiled(&compiled) > 0);
        let before = second.native_compile_cache_stats();
        assert!(second.prewarm_compiled(&compiled) > 0);
        let after = second.native_compile_cache_stats();
        assert_eq!(after.misses, before.misses);
        assert_eq!(after.hits, before.hits + 1);
    }
}
