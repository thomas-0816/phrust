use crate::composer_metadata::AutoloadCompilationResolver;
use crate::engine_compat::EngineInput;
use crate::input::PhpRequestExecutionInput;
use php_runtime::api::{ErrorReporting, FilesystemCapabilities, RuntimeContext};
use php_vm::api::IncludeLoader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn include_loader_for(input: &EngineInput) -> Result<Option<IncludeLoader>, String> {
    let mut roots = Vec::new();
    push_existing_root(&mut roots, &input.cwd);
    push_script_roots(&mut roots, input.real_path.as_deref());
    if let Some(include_path) = &input.ini.include_path {
        for entry in include_path {
            if entry.is_absolute() {
                push_existing_root(&mut roots, entry);
            } else {
                push_existing_root(&mut roots, &input.cwd.join(entry));
                if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
                    push_existing_root(&mut roots, &real_path.join(entry));
                }
            }
        }
    }
    if roots.is_empty() {
        return Ok(None);
    }
    IncludeLoader::new(roots)
        .map(|loader| {
            loader.with_compilation_dependency_resolver(Arc::new(AutoloadCompilationResolver))
        })
        .map(Some)
        .map_err(|error| error.render_message())
}

pub(crate) fn include_loader_for_request(
    input: &PhpRequestExecutionInput,
) -> Result<Option<IncludeLoader>, String> {
    let mut roots = Vec::new();
    push_existing_root(&mut roots, &input.cwd);
    push_script_roots(&mut roots, input.real_path.as_deref());
    for entry in &input.include_roots {
        if entry.is_absolute() {
            push_existing_root(&mut roots, entry);
        } else {
            push_existing_root(&mut roots, &input.cwd.join(entry));
            if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
                push_existing_root(&mut roots, &real_path.join(entry));
            }
        }
    }
    if roots.is_empty() {
        return Ok(None);
    }
    IncludeLoader::new(roots)
        .map(|loader| {
            loader.with_compilation_dependency_resolver(Arc::new(AutoloadCompilationResolver))
        })
        .map(Some)
        .map_err(|error| error.render_message())
}

fn push_existing_root(roots: &mut Vec<PathBuf>, path: &Path) {
    if path.exists() {
        roots.push(path.to_path_buf());
    }
}

fn push_script_roots(roots: &mut Vec<PathBuf>, real_path: Option<&Path>) {
    if let Some(script_dir) = real_path.and_then(Path::parent) {
        push_existing_root(roots, script_dir);
        if let Some(parent) = script_dir.parent() {
            push_existing_root(roots, parent);
        }
    }
}

pub(crate) fn runtime_context_for(
    input: &EngineInput,
    include_loader: Option<&IncludeLoader>,
) -> RuntimeContext {
    let include_path = input
        .ini
        .include_path
        .clone()
        .unwrap_or_else(|| vec![PathBuf::from(".")]);
    let mut context =
        RuntimeContext::controlled_cli(input.script_name.clone(), input.script_args.clone())
            .with_cwd(input.cwd.clone())
            .with_include_path(include_path)
            .with_env(input.env.clone())
            .with_ini_overrides(input.ini.overrides.clone())
            .with_stdin(input.stdin.clone())
            .with_php_binary(input.php_binary.clone());
    if let Some(mask) = input.ini.error_reporting {
        context.ini.error_reporting = ErrorReporting { mask };
    }
    if let Some(display_errors) = input.ini.display_errors {
        context.ini.display_errors = display_errors;
    }
    if let Some(default_input_filter) = input.ini.default_input_filter {
        context.ini.default_input_filter = default_input_filter;
    }
    if let Some(default_input_filter_flags) = input.ini.default_input_filter_flags {
        context.ini.default_input_filter_flags = default_input_filter_flags;
    }
    let mut allowed_roots = include_loader
        .map(|loader| loader.allowed_roots().to_vec())
        .unwrap_or_default();
    // This helper exclusively builds the CLI-compatible request context.
    // PHP CLI scripts may create file-backed temporary resources without
    // granting the same host path to server requests.
    allowed_roots.push(std::env::temp_dir());
    let capabilities = FilesystemCapabilities::none()
        .with_stdio(true)
        .with_standard_devices(cfg!(target_os = "linux"))
        .with_allowed_roots(allowed_roots);
    context.with_filesystem_capabilities(capabilities)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_runtime_context_allows_the_system_temporary_directory() {
        let cwd = std::env::current_dir().expect("current directory");
        let input = EngineInput {
            source: "<?php".to_owned(),
            source_path: "temp-capability.php".to_owned(),
            real_path: None,
            script_name: "temp-capability.php".to_owned(),
            script_args: Vec::new(),
            cwd,
            env: Vec::new(),
            ini: Default::default(),
            stdin: Vec::new(),
            php_binary: "phrust-php".to_owned(),
            debug: false,
            debug_log: None,
            debug_format: php_diagnostics::DiagnosticOutputFormat::Text,
        };

        let context = runtime_context_for(&input, None);

        assert!(context.filesystem.allows_path(&std::env::temp_dir()));
        assert_eq!(
            context.filesystem.allows_standard_devices(),
            cfg!(target_os = "linux")
        );
    }
}
