use crate::engine_compat::EngineInput;
use crate::input::PhpRequestExecutionInput;
use php_runtime::api::{ErrorReporting, FilesystemCapabilities, RuntimeContext};
use php_vm::api::IncludeLoader;
use std::path::{Path, PathBuf};

pub(crate) fn include_loader_for(input: &EngineInput) -> Result<Option<IncludeLoader>, String> {
    let mut roots = Vec::new();
    push_existing_root(&mut roots, &input.cwd);
    if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
        push_existing_root(&mut roots, real_path);
    }
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
        .map(Some)
        .map_err(|error| error.render_message())
}

pub(crate) fn include_loader_for_request(
    input: &PhpRequestExecutionInput,
) -> Result<Option<IncludeLoader>, String> {
    let mut roots = Vec::new();
    push_existing_root(&mut roots, &input.cwd);
    if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
        push_existing_root(&mut roots, real_path);
    }
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
        .map(Some)
        .map_err(|error| error.render_message())
}

fn push_existing_root(roots: &mut Vec<PathBuf>, path: &Path) {
    if path.exists() {
        roots.push(path.to_path_buf());
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
    let mut capabilities = FilesystemCapabilities::none().with_stdio(true);
    if let Some(loader) = include_loader {
        capabilities = capabilities.with_allowed_roots(loader.allowed_roots().to_vec());
    }
    context.with_filesystem_capabilities(capabilities)
}
