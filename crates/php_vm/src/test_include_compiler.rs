use crate::compiled_unit::CompiledUnit;
use crate::error::VmError;
use crate::include::{
    CompilationDependencyRequest, CompiledInclude, IncludeCompiler, IncludeCompilerFingerprint,
    IncludeLoader, ValidatedIncludeSource,
};
use php_optimizer::{OptimizationLevel, PassContext, PassPipeline};
use std::collections::HashMap;

/// Test-only compiler that keeps frontend ownership outside the include module.
#[derive(Clone, Debug)]
pub(crate) struct TestIncludeCompiler {
    optimization_level: OptimizationLevel,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TestOptimizationLevel {
    O0,
    O2,
}

impl TestIncludeCompiler {
    pub(crate) const fn baseline() -> Self {
        Self {
            optimization_level: OptimizationLevel::O0,
        }
    }

    pub(crate) const fn optimized() -> Self {
        Self {
            optimization_level: OptimizationLevel::O2,
        }
    }

    pub(crate) const fn new(optimization_level: TestOptimizationLevel) -> Self {
        match optimization_level {
            TestOptimizationLevel::O0 => Self::baseline(),
            TestOptimizationLevel::O2 => Self::optimized(),
        }
    }
}

impl IncludeCompiler for TestIncludeCompiler {
    fn fingerprint(&self, loader: &IncludeLoader) -> IncludeCompilerFingerprint {
        IncludeCompilerFingerprint::new(format!(
            "php_vm_test:{}:debug={}:optimization={}:dependencies={:016x}",
            env!("CARGO_PKG_VERSION"),
            cfg!(debug_assertions),
            self.optimization_level.as_str(),
            loader.compilation_dependency_fingerprint(),
        ))
    }

    fn compile_include(
        &self,
        source: ValidatedIncludeSource,
        loader: &IncludeLoader,
    ) -> Result<CompiledInclude, VmError> {
        compile_include(source, loader, self.optimization_level)
    }

    fn compile_eval(&self, source_path: &str, source: &str) -> Result<CompiledUnit, VmError> {
        let frontend = php_semantics::analyze_source(source);
        if frontend.has_errors() {
            return Err(include_compile_error(
                "E_PHP_VM_EVAL_PARSE_ERROR",
                format!("{source_path} failed frontend analysis"),
            )
            .with_context("path", source_path)
            .with_context("stage", "frontend"));
        }
        let lowering = php_ir::lower_frontend_result(
            &frontend,
            php_ir::LoweringOptions {
                source_path: source_path.to_owned(),
                source_text: Some(source.to_owned()),
                ..php_ir::LoweringOptions::default()
            },
        );
        if !lowering.diagnostics.is_empty() || lowering.verification.is_err() {
            return Err(include_compile_error(
                "E_PHP_VM_EVAL_COMPILE_ERROR",
                format!("{source_path} failed IR lowering"),
            )
            .with_context("path", source_path)
            .with_context("stage", "ir_lowering")
            .with_context("detail", ir_lowering_failure_detail(&lowering)));
        }
        Ok(CompiledUnit::with_ordered_sources(
            lowering.unit,
            [std::sync::Arc::<str>::from(source)],
        ))
    }
}

fn compile_include(
    source: ValidatedIncludeSource,
    loader: &IncludeLoader,
    optimization_level: OptimizationLevel,
) -> Result<CompiledInclude, VmError> {
    let entry_path = source.loaded().canonical_path.clone();
    let mut session = php_ir::CompilationSession::new(
        source
            .loaded()
            .canonical_path
            .to_string_lossy()
            .into_owned(),
        source.loaded().source.clone(),
    );
    let mut dependencies = Vec::new();
    let mut providers = HashMap::<String, php_ir::CompilationFileId>::new();
    for name in session.declared_trait_names(session.entry()) {
        providers.insert(name, session.entry());
    }

    let mut next_file = 0;
    while next_file < session.files().len() {
        let file_id = session.files()[next_file].id();
        if session.files()[next_file].frontend().has_errors() {
            return Err(include_compile_error(
                "E_PHP_VM_INCLUDE_COMPILE_ERROR",
                format!(
                    "{} failed frontend analysis",
                    session.files()[next_file].path()
                ),
            )
            .with_context("path", session.files()[next_file].path())
            .with_context("stage", "frontend"));
        }
        for request in session.unresolved_trait_requests(file_id) {
            if let Some(provider) = providers.get(&request.normalized_name).copied() {
                let provider_source = &session.files()[provider.index()];
                session.add_dependency(
                    file_id,
                    &request.normalized_name,
                    provider_source.path().to_owned(),
                    provider_source.source().to_owned(),
                );
                continue;
            }
            let requesting_path = std::path::Path::new(session.files()[file_id.index()].path());
            let Some(resolved_dependency) =
                loader.load_compilation_dependency(CompilationDependencyRequest {
                    requesting_path,
                    declaration: &request.resolved_name,
                })?
            else {
                continue;
            };
            let (dependency_source, metadata_sources, activate_through_autoload) =
                resolved_dependency.into_parts();
            let probe = php_ir::CompilationSession::new(
                dependency_source
                    .loaded()
                    .canonical_path
                    .to_string_lossy()
                    .into_owned(),
                dependency_source.loaded().source.clone(),
            );
            if !probe
                .declared_trait_names(probe.entry())
                .iter()
                .any(|name| name == &request.normalized_name)
            {
                return Err(include_compile_error(
                    "E_PHP_VM_INCLUDE_DEPENDENCY_MISMATCH",
                    format!(
                        "mapped file {} does not declare trait `{}`",
                        dependency_source.loaded().canonical_path.display(),
                        request.normalized_name
                    ),
                )
                .with_context("declaration", &request.normalized_name)
                .with_context(
                    "canonical_path",
                    dependency_source.loaded().canonical_path.display(),
                ));
            }

            let path = dependency_source.loaded().canonical_path.clone();
            let dependency = if activate_through_autoload {
                session.add_autoload_dependency(
                    file_id,
                    &request.normalized_name,
                    &request.normalized_name,
                    path.to_string_lossy().into_owned(),
                    dependency_source.loaded().source.clone(),
                )
            } else {
                session.add_dependency(
                    file_id,
                    &request.normalized_name,
                    path.to_string_lossy().into_owned(),
                    dependency_source.loaded().source.clone(),
                )
            };
            for declared in session.declared_trait_names(dependency) {
                if let Some(previous) = providers.insert(declared.clone(), dependency)
                    && previous != dependency
                {
                    return Err(include_compile_error(
                        "E_PHP_VM_INCLUDE_DUPLICATE_DECLARATION",
                        format!("duplicate trait declaration `{declared}`"),
                    )
                    .with_context("declaration", declared));
                }
            }
            dependencies.extend(
                metadata_sources
                    .into_iter()
                    .map(ValidatedIncludeSource::into_dependency),
            );
            dependencies.push(dependency_source.into_dependency());
        }
        next_file += 1;
    }

    if let Some(cycle) = session.dependency_cycle() {
        let paths = cycle
            .edges
            .iter()
            .map(|edge| session.files()[edge.requester.index()].path())
            .chain(
                cycle
                    .edges
                    .last()
                    .map(|edge| session.files()[edge.dependency.index()].path()),
            )
            .collect::<Vec<_>>();
        let declarations = cycle
            .edges
            .iter()
            .map(|edge| edge.declaration.as_str())
            .collect::<Vec<_>>();
        return Err(include_compile_error(
            "E_PHP_VM_INCLUDE_DEPENDENCY_CYCLE",
            format!("declaration dependency cycle: {}", paths.join(" -> ")),
        )
        .with_context("paths", paths.join(":"))
        .with_context("declarations", declarations.join(":")));
    }

    let mut lowering =
        php_ir::lower_compilation_session(&session, php_ir::LoweringOptions::default());
    if !lowering.diagnostics.is_empty() || lowering.verification.is_err() {
        let detail = ir_lowering_failure_detail(&lowering);
        return Err(include_compile_error(
            "E_PHP_VM_INCLUDE_COMPILE_ERROR",
            format!(
                "{} failed IR lowering: {detail}",
                session.files()[session.entry().index()].path()
            ),
        )
        .with_context("path", session.files()[session.entry().index()].path())
        .with_context("stage", "ir_lowering")
        .with_context("detail", detail)
        .with_context(
            "local_trait_files",
            session
                .files()
                .iter()
                .filter(|file| file.id() != session.entry())
                .map(|file| file.path().to_owned())
                .collect::<Vec<_>>()
                .join(":"),
        ));
    }
    if optimization_level.runs_pipeline() {
        PassPipeline::performance()
            .run(&mut lowering.unit, &PassContext::new(optimization_level))
            .map_err(|error| {
                include_compile_error(
                    "E_PHP_VM_INCLUDE_COMPILE_ERROR",
                    format!("{} optimizer failed: {error}", entry_path.display()),
                )
                .with_context("path", entry_path.display())
                .with_context("stage", "optimizer")
            })?;
    }
    let retained_sources = session
        .files()
        .iter()
        .map(|file| std::sync::Arc::<str>::from(file.source()));
    Ok(CompiledInclude {
        unit: CompiledUnit::with_ordered_sources(lowering.unit, retained_sources),
        dependencies,
    })
}

fn include_compile_error(code: &'static str, message: impl Into<String>) -> VmError {
    VmError::fatal(code, "include_compile", message)
}

fn ir_lowering_failure_detail(lowering: &php_ir::LoweringResult) -> String {
    if let Some(diagnostic) = lowering.diagnostics.first() {
        return format!("{}: {}", diagnostic.id, diagnostic.message);
    }
    if let Err(error) = &lowering.verification {
        return format!("IR verification failed: {error:?}");
    }
    "unknown IR lowering failure".to_string()
}
