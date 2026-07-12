use php_optimizer::{OptimizationLevel, PassContext, PassPipeline};
use php_vm::api::{
    CompilationDependencyRequest, CompiledInclude, CompiledUnit, IncludeCompiler,
    IncludeCompilerFingerprint, IncludeLoader, ValidatedIncludeSource, VmError,
};
use std::collections::{HashMap, HashSet};

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
    let mut dependency_paths = HashSet::new();
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
            if !source_declares_trait(resolved_dependency.source(), &request.normalized_name) {
                return Err(include_compile_error(
                    "E_PHP_VM_INCLUDE_DEPENDENCY_MISMATCH",
                    format!(
                        "mapped file {} does not declare trait `{}`",
                        resolved_dependency
                            .source()
                            .loaded()
                            .canonical_path
                            .display(),
                        request.normalized_name
                    ),
                )
                .with_context("declaration", &request.normalized_name)
                .with_context(
                    "canonical_path",
                    resolved_dependency
                        .source()
                        .loaded()
                        .canonical_path
                        .display(),
                ));
            }
            let (dependency_source, metadata_sources, activate_through_autoload) =
                resolved_dependency.into_parts();
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
            for metadata_source in metadata_sources {
                if dependency_paths.insert(metadata_source.loaded().canonical_path.clone()) {
                    dependencies.push(metadata_source.into_dependency());
                }
            }
            if dependency_paths.insert(path) {
                dependencies.push(dependency_source.into_dependency());
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer_metadata::AutoloadCompilationResolver;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "phrust-include-compiler-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create fixture");
        root
    }

    fn write(root: &Path, relative: &str, source: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, source).expect("write fixture");
    }

    fn load(root: &Path, relative: &str) -> (IncludeLoader, ValidatedIncludeSource) {
        let loader = IncludeLoader::for_root(root)
            .expect("loader")
            .with_compilation_dependency_resolver(Arc::new(AutoloadCompilationResolver));
        let resolved = loader
            .resolve_with_include_path(None, relative, &[], Some(root))
            .expect("resolve source");
        let source = loader
            .load_validated_resolved(&resolved)
            .expect("load source");
        (loader, source)
    }

    #[test]
    fn composer_metadata_resolves_trait_without_source_inference() {
        let root = fixture("composer-trait");
        write(
            &root,
            "src/Registry.php",
            "<?php namespace Acme; use Acme\\Support\\WithThing; class Registry { use WithThing; }",
        );
        write(
            &root,
            "src/Support/WithThing.php",
            "<?php namespace Acme\\Support; trait WithThing { public function value(): string { return 'ok'; } }",
        );
        write(
            &root,
            "vendor/composer/autoload_psr4.php",
            "<?php return ['Acme\\\\' => [__DIR__ . '/../../src']];",
        );
        let (loader, source) = load(&root, "src/Registry.php");
        let probe = php_ir::CompilationSession::new(
            source
                .loaded()
                .canonical_path
                .to_string_lossy()
                .into_owned(),
            source.loaded().source.clone(),
        );
        let request = probe
            .unresolved_trait_requests(probe.entry())
            .pop()
            .expect("trait request");
        assert_eq!(request.resolved_name, "Acme\\Support\\WithThing");

        let compiled =
            compile_include(source, &loader, OptimizationLevel::O0).expect("compile mapped trait");

        assert_eq!(compiled.unit.unit().files.len(), 2);
        assert_eq!(
            compiled.unit.unit().linked_entry_autoload_declarations,
            vec![Some("acme\\support\\withthing".to_owned()), None]
        );
        assert_eq!(compiled.dependencies.len(), 2);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn static_psr4_autoloader_resolves_trait_without_composer_metadata() {
        let root = fixture("static-psr4-trait");
        write(
            &root,
            "autoload.php",
            r#"<?php
$prefix = 'Acme\\';
$prefix_len = 5;
$base_dir = __DIR__;
spl_autoload_register(static function ($class_name) use ($prefix, $prefix_len, $base_dir): void {
    if (0 === strncmp($class_name, $prefix, $prefix_len)) {
        $relative_class = substr($class_name, $prefix_len);
        $file = $base_dir . '/src/' . str_replace('\\', '/', $relative_class) . '.php';
        if (file_exists($file)) {
            require $file;
        }
    }
});
"#,
        );
        write(
            &root,
            "src/Registry.php",
            "<?php namespace Acme; use Acme\\Support\\WithThing; class Registry { use WithThing; }",
        );
        write(
            &root,
            "src/Support/WithThing.php",
            "<?php namespace Acme\\Support; trait WithThing { public function value(): string { return 'ok'; } }",
        );
        let (loader, source) = load(&root, "src/Registry.php");

        let compiled = compile_include(source, &loader, OptimizationLevel::O0)
            .expect("compile static PSR-4 mapped trait");

        assert_eq!(compiled.unit.unit().files.len(), 2);
        assert_eq!(
            compiled.unit.unit().linked_entry_autoload_declarations,
            vec![Some("acme\\support\\withthing".to_owned()), None]
        );
        assert_eq!(compiled.dependencies.len(), 2);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn unmapped_sibling_trait_is_not_discovered_from_source_layout() {
        let root = fixture("unmapped-trait");
        write(
            &root,
            "src/Registry.php",
            "<?php namespace Acme; use Acme\\Support\\MissingTrait; class Registry { use MissingTrait; }",
        );
        write(
            &root,
            "src/Support/MissingTrait.php",
            "<?php namespace Acme\\Support; trait MissingTrait {}",
        );
        let (loader, source) = load(&root, "src/Registry.php");

        let error = compile_include(source, &loader, OptimizationLevel::O0)
            .expect_err("unmapped source layout must not be inferred");

        assert_eq!(error.code(), "E_PHP_VM_INCLUDE_COMPILE_ERROR");
        assert!(error.render_message().contains("E_PHP_IR_TRAIT_NOT_FOUND"));
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
