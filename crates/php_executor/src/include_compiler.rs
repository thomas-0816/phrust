use php_optimizer::{OptimizationLevel, PassContext, PassPipeline};
use php_vm::api::{
    CompiledInclude, CompiledUnit, IncludeCompiler, IncludeCompilerFingerprint, IncludeLoader,
    ValidatedIncludeSource, VmError,
};
use std::collections::HashMap;

/// Executor-owned compiler used by VM include and eval execution.
#[derive(Clone, Debug)]
pub struct ExecutorIncludeCompiler {
    optimization_level: OptimizationLevel,
}

impl ExecutorIncludeCompiler {
    /// Creates a compiler with an explicit include optimization level.
    #[must_use]
    pub const fn new(optimization_level: OptimizationLevel) -> Self {
        Self { optimization_level }
    }
}

impl IncludeCompiler for ExecutorIncludeCompiler {
    fn fingerprint(&self, loader: &IncludeLoader) -> IncludeCompilerFingerprint {
        IncludeCompilerFingerprint::new(format!(
            "php_executor:{}:debug={}:optimization={}:dependencies={:016x}",
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
        Ok(CompiledUnit::new(lowering.unit))
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
            let mut inferred = false;
            let dependency_source =
                match loader.load_compilation_dependency(&request.normalized_name)? {
                    Some(dependency_source) => {
                        if !source_declares_trait(&dependency_source, &request.normalized_name) {
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
                        dependency_source
                    }
                    // No explicit mapping: infer the trait's file from the
                    // requesting file's own PSR-4 layout, the same file the
                    // reference autoloader would pull in at class-link time.
                    // A miss, a policy-rejected path, or a file that does not
                    // declare the trait falls through to the standard
                    // missing-trait diagnostic.
                    None => {
                        let Some(dependency_source) =
                            load_psr_inferred_trait(&session, file_id, &request, loader)
                        else {
                            continue;
                        };
                        inferred = true;
                        dependency_source
                    }
                };

            let path = dependency_source.loaded().canonical_path.clone();
            let dependency = if inferred {
                session.add_inferred_dependency(
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
    Ok(CompiledInclude {
        unit: CompiledUnit::new(lowering.unit),
        dependencies,
    })
}

fn source_declares_trait(source: &ValidatedIncludeSource, normalized_name: &str) -> bool {
    let probe = php_ir::CompilationSession::new(
        source
            .loaded()
            .canonical_path
            .to_string_lossy()
            .into_owned(),
        source.loaded().source.clone(),
    );
    probe
        .declared_trait_names(probe.entry())
        .iter()
        .any(|name| name == normalized_name)
}

/// Loads the trait file a PSR-4 autoloader would provide for `request`,
/// inferred from the requesting file's namespace-to-path layout. Returns
/// `None` when inference fails, the loader's root policy rejects the file,
/// or the file does not declare the requested trait.
fn load_psr_inferred_trait(
    session: &php_ir::CompilationSession,
    file_id: php_ir::CompilationFileId,
    request: &php_ir::UnresolvedTraitRequest,
    loader: &IncludeLoader,
) -> Option<ValidatedIncludeSource> {
    let requesting = &session.files()[file_id.index()];
    let requesting_path = std::path::Path::new(requesting.path());
    let map = crate::psr_map::LocalPsrSourceMap::infer(requesting.source(), requesting_path)?;
    let path = map.resolve_declaration(&request.normalized_name)?;
    let path = path.to_string_lossy();
    let resolved = loader
        .resolve_with_include_path(
            None,
            &path,
            &[],
            loader.allowed_roots().first().map(std::path::Path::new),
        )
        .ok()?;
    let dependency_source = loader.load_validated_resolved(&resolved).ok()?;
    source_declares_trait(&dependency_source, &request.normalized_name).then_some(dependency_source)
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
