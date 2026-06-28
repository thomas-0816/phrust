use php_ir::{
    LoweringOptions, lower_frontend_result,
    module::{IrUnit, normalize_class_name},
    verify_unit,
};
use php_runtime::{ErrorReporting, ExitStatus, FilesystemCapabilities, RuntimeContext};
use php_semantics::{FrontendResult, Severity, analyze_source, diagnostics::DiagnosticId};
use php_source::{SourceText, TextRange};
use php_vm::{IncludeLoader, Vm, VmOptions};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const EXIT_SUCCESS: i32 = 0;
const EXIT_PHP_ERROR: i32 = 255;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CliIniOptions {
    pub include_path: Option<Vec<PathBuf>>,
    pub display_errors: Option<bool>,
    pub error_reporting: Option<i64>,
    /// Raw `-d name=value` ini overrides forwarded to the runtime registry.
    pub overrides: Vec<(String, String)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineInput {
    pub source: String,
    pub source_path: String,
    pub real_path: Option<PathBuf>,
    pub script_name: String,
    pub script_args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub ini: CliIniOptions,
    pub stdin: Vec<u8>,
}

pub fn execute_php<W, E>(input: EngineInput, stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let pipeline = compile_source(&input.source, &input.source_path)?;
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_PHP_ERROR);
    }
    let include_loader = include_loader_for(&input)?;
    let runtime_context = runtime_context_for(&input, include_loader.as_ref());
    let vm = Vm::with_options(VmOptions {
        include_loader,
        runtime_context,
        ..VmOptions::default()
    });
    let result = vm.execute(pipeline.lowering.unit.clone());
    stdout
        .write_all(result.output.as_bytes())
        .map_err(|error| error.to_string())?;
    match result.status.exit_status() {
        ExitStatus::Success => Ok(EXIT_SUCCESS),
        ExitStatus::CompileError => {
            if write_vm_compile_fatal_line(stderr, &pipeline, &result.status)? {
                return Ok(EXIT_PHP_ERROR);
            }
            write_runtime_diagnostics(stderr, &input.source_path, &result.diagnostics)?;
            writeln!(stderr, "{}: {}", input.source_path, result.status)
                .map_err(|error| error.to_string())?;
            Ok(EXIT_PHP_ERROR)
        }
        ExitStatus::RuntimeError | ExitStatus::Fatal | ExitStatus::Unsupported => {
            // An uncaught exception has already been rendered to stdout as a PHP
            // `Fatal error:`; emitting the internal diagnostic dump as well would
            // duplicate it and pollute PHPT output comparison.
            let rendered_uncaught = result
                .diagnostics
                .first()
                .is_some_and(|diagnostic| diagnostic.id() == "E_PHP_VM_UNCAUGHT_EXCEPTION");
            if !rendered_uncaught {
                write_runtime_diagnostics(stderr, &input.source_path, &result.diagnostics)?;
                writeln!(stderr, "{}: {}", input.source_path, result.status)
                    .map_err(|error| error.to_string())?;
            }
            Ok(EXIT_PHP_ERROR)
        }
    }
}

struct Pipeline {
    path: String,
    source: SourceText,
    frontend: FrontendResult,
    lowering: php_ir::LoweringResult,
}

impl Pipeline {
    fn ok(&self) -> bool {
        !self.frontend.has_errors()
            && self.lowering.diagnostics.is_empty()
            && self.lowering.verification.is_ok()
    }
}

fn compile_source(source: &str, source_path: &str) -> Result<Pipeline, String> {
    let frontend = analyze_source(source);
    let mut lowering = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_path: source_path.to_string(),
            source_text: Some(source.to_string()),
            ..LoweringOptions::default()
        },
    );
    if !frontend.has_errors() && lowering.verification.is_ok() {
        verify_unit(&lowering.unit).map_err(|errors| {
            format!(
                "{source_path}: IR verification failed: {} error(s)",
                errors.len()
            )
        })?;
        lowering.verification = verify_unit(&lowering.unit);
    }
    Ok(Pipeline {
        path: source_path.to_string(),
        source: SourceText::new(source),
        frontend,
        lowering,
    })
}

fn include_loader_for(input: &EngineInput) -> Result<Option<IncludeLoader>, String> {
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
    IncludeLoader::new(roots).map(Some)
}

fn push_existing_root(roots: &mut Vec<PathBuf>, path: &Path) {
    if path.exists() {
        roots.push(path.to_path_buf());
    }
}

fn runtime_context_for(
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
            .with_stdin(input.stdin.clone());
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

fn write_frontend_diagnostics<W: Write>(stderr: &mut W, pipeline: &Pipeline) -> Result<(), String> {
    for diagnostic in pipeline.frontend.parser_diagnostics() {
        write_parser_diagnostic(
            stderr,
            &pipeline.path,
            &pipeline.source,
            diagnostic.span,
            diagnostic.id.as_str(),
            &diagnostic.message,
        )?;
    }
    for diagnostic in pipeline.frontend.semantic_diagnostics() {
        if diagnostic.severity() == Severity::Error {
            if let Some(span) = diagnostic.span() {
                if let Some(message) = semantic_diagnostic_php_fatal_message(
                    diagnostic.id(),
                    diagnostic.message(),
                    span,
                    &pipeline.lowering.unit,
                ) {
                    write_php_fatal_line(stderr, &pipeline.path, &pipeline.source, span, &message)?;
                    continue;
                }
                if diagnostic.id() == DiagnosticId::InvalidTypeCallableContext {
                    write_php_fatal_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
                    continue;
                }
                if semantic_diagnostic_uses_php_parse_error_line(diagnostic.id()) {
                    write_php_parse_error_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
                    return Ok(());
                }
                if semantic_diagnostic_uses_php_fatal_line(diagnostic.id()) {
                    write_php_fatal_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
                    if semantic_diagnostic_is_immediate_php_fatal(diagnostic.id()) {
                        return Ok(());
                    }
                    continue;
                }
                write_span_line(
                    stderr,
                    &pipeline.path,
                    span,
                    diagnostic.id().as_str(),
                    diagnostic.message(),
                )?;
            } else {
                writeln!(
                    stderr,
                    "{}: {}: {}",
                    pipeline.path,
                    diagnostic.id().as_str(),
                    diagnostic.message()
                )
                .map_err(|error| error.to_string())?;
            }
        }
    }
    for diagnostic in &pipeline.lowering.diagnostics {
        writeln!(
            stderr,
            "{}:{}..{}: {}: {}",
            pipeline.path,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.id,
            diagnostic.message
        )
        .map_err(|error| error.to_string())?;
    }
    if let Err(errors) = &pipeline.lowering.verification {
        writeln!(
            stderr,
            "{}: IR verification failed: {} error(s)",
            pipeline.path,
            errors.len()
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_php_fatal_line<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &SourceText,
    span: TextRange,
    message: &str,
) -> Result<(), String> {
    let line = line_number_for_span(source, span);
    writeln!(stderr, "Fatal error: {message} in {path} on line {line}")
        .map_err(|error| error.to_string())
}

fn write_vm_compile_fatal_line<W: Write>(
    stderr: &mut W,
    pipeline: &Pipeline,
    status: &php_runtime::ExecutionStatus,
) -> Result<bool, String> {
    let Some(message) = status.message() else {
        return Ok(false);
    };
    let Some(display_message) = vm_compile_error_php_fatal_message(message) else {
        return Ok(false);
    };
    let span = if let Some((class_name, method_name)) = vm_compile_error_interface_method(message) {
        class_method_span(&pipeline.lowering.unit, &class_name, &method_name)
    } else if let Some((class_name, _, _)) = vm_compile_error_interface_method_missing(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some((class_name, constant_name)) = vm_compile_error_interface_constant(message) {
        class_constant_span(&pipeline.lowering.unit, &class_name, &constant_name)
    } else if vm_compile_error_interface_property(message) {
        pipeline
            .lowering
            .unit
            .classes
            .iter()
            .find(|class| class.flags.is_interface)
            .map(|class| class.span)
    } else if let Some((class_name, method_name)) = vm_compile_error_child_method(message) {
        class_method_span(&pipeline.lowering.unit, &class_name, &method_name)
    } else if let Some((class_name, _property_name)) = vm_compile_error_child_property(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some((class_name, _constant_name)) = vm_compile_error_child_constant(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some((parent_class, method_name)) = vm_compile_error_final_method(message) {
        overriding_method_span(&pipeline.lowering.unit, &parent_class, &method_name)
    } else if let Some(class_name) = vm_compile_error_traversable_direct(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some(class_name) = vm_compile_error_child_class(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else {
        None
    };
    let Some(span) = span else {
        return Ok(false);
    };
    write_php_fatal_line(
        stderr,
        &pipeline.path,
        &pipeline.source,
        TextRange::new(span.start as usize, span.end as usize),
        &display_message,
    )?;
    Ok(true)
}

fn semantic_diagnostic_php_fatal_message(
    id: DiagnosticId,
    message: &str,
    span: TextRange,
    unit: &IrUnit,
) -> Option<String> {
    match id {
        DiagnosticId::InvalidConstExpr => {
            Some("Constant expression contains invalid operations".to_owned())
        }
        DiagnosticId::DuplicateClassMember => {
            let constant_name = message
                .strip_prefix("duplicate class constant `")?
                .strip_suffix('`')?;
            let class_name = class_display_name_containing_span(unit, span)?;
            Some(format!(
                "Cannot redefine class constant {class_name}::{constant_name}"
            ))
        }
        DiagnosticId::IncompatibleModifiers => match message {
            "`static` modifier is not allowed on class constant" => {
                Some("Cannot use the static modifier on a class constant".to_owned())
            }
            "`abstract` modifier is not allowed on class constant" => {
                Some("Cannot use the abstract modifier on a class constant".to_owned())
            }
            "method cannot be both abstract and final" => {
                Some("Cannot use the final modifier on an abstract method".to_owned())
            }
            _ => None,
        },
        _ => None,
    }
}

fn class_display_name_containing_span(unit: &IrUnit, span: TextRange) -> Option<&str> {
    let start = span.start().to_usize();
    let end = span.end().to_usize();
    unit.classes
        .iter()
        .filter(|class| class.span.start as usize <= start && end <= class.span.end as usize)
        .min_by_key(|class| class.span.end.saturating_sub(class.span.start))
        .map(|class| class.display_name.as_str())
}

fn vm_compile_error_php_fatal_message(message: &str) -> Option<String> {
    if let Some((class_name, interface_name, method_name)) =
        vm_compile_error_interface_method_missing(message)
    {
        return Some(format!(
            "Class {class_name} contains 1 abstract method and must therefore be declared abstract or implement the remaining method ({interface_name}::{method_name})"
        ));
    }

    message
        .strip_prefix("E_PHP_VM_METHOD_VISIBILITY_OVERRIDE: ")
        .or_else(|| message.strip_prefix("E_PHP_VM_STATIC_METHOD_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_VISIBILITY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_BODY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_SIGNATURE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_PROPERTY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_CONSTANT_VISIBILITY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_FINAL_CLASS_EXTEND: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_FINAL_METHOD_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_PROPERTY_STATIC_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_PROPERTY_VISIBILITY_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_CLASS_EXTENDS_INTERFACE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_IMPLEMENTS_NON_INTERFACE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_TRAVERSABLE_DIRECT_IMPLEMENTATION: "))
        .map(str::to_owned)
}

fn vm_compile_error_interface_method(message: &str) -> Option<(String, String)> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_VISIBILITY: ") {
        let target = rest
            .strip_prefix("Access type for interface method ")?
            .split_once("()")?
            .0;
        return split_class_method(target);
    }
    if let Some(rest) = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_BODY: ") {
        let target = rest
            .strip_prefix("Interface function ")?
            .split_once("()")?
            .0;
        return split_class_method(target);
    }
    None
}

fn vm_compile_error_interface_property(message: &str) -> bool {
    message.starts_with("E_PHP_VM_INTERFACE_PROPERTY: ")
}

fn vm_compile_error_interface_method_missing(message: &str) -> Option<(String, String, String)> {
    let rest = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_MISSING: ")?;
    let rest = rest.strip_prefix("class ")?;
    let (class_name, target) = rest.split_once(" must implement ")?;
    let (interface_name, method_name) = target.split_once("::")?;
    Some((
        class_name.to_owned(),
        interface_name.to_owned(),
        method_name.to_owned(),
    ))
}

fn vm_compile_error_interface_constant(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("E_PHP_VM_INTERFACE_CONSTANT_VISIBILITY: ")?;
    let target = rest
        .strip_prefix("Access type for interface constant ")?
        .split_once(" must be public")?
        .0;
    split_class_method(target)
}

fn vm_compile_error_child_method(message: &str) -> Option<(String, String)> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_METHOD_VISIBILITY_OVERRIDE: ") {
        let target = rest.strip_prefix("Access level to ")?.split_once("()")?.0;
        return split_class_method(target);
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_STATIC_METHOD_OVERRIDE: ") {
        let (parent_method, class_name) = rest
            .strip_prefix("Cannot make static method ")
            .and_then(|rest| rest.split_once("() non static in class "))
            .or_else(|| {
                rest.strip_prefix("Cannot make non static method ")
                    .and_then(|rest| rest.split_once("() static in class "))
            })?;
        let (_, method_name) = split_class_method(parent_method)?;
        return Some((class_name.to_owned(), method_name));
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: ") {
        let target = rest
            .strip_prefix("Declaration of ")?
            .split_once(" must be compatible with ")?
            .0;
        return split_class_method(target);
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_SIGNATURE: ") {
        let target = rest
            .strip_prefix("Declaration of ")?
            .split_once(" must be compatible with ")?
            .0;
        return split_class_method(target);
    }

    None
}

fn vm_compile_error_child_class(message: &str) -> Option<String> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_FINAL_CLASS_EXTEND: ") {
        return Some(
            rest.strip_prefix("Class ")?
                .split_once(" cannot extend final class ")?
                .0
                .to_owned(),
        );
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_CLASS_EXTENDS_INTERFACE: ") {
        return Some(
            rest.strip_prefix("Class ")?
                .split_once(" cannot extend interface ")?
                .0
                .to_owned(),
        );
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_IMPLEMENTS_NON_INTERFACE: ") {
        return Some(rest.split_once(" cannot implement ")?.0.to_owned());
    }

    None
}

fn vm_compile_error_final_method(message: &str) -> Option<(String, String)> {
    let target = message
        .strip_prefix("E_PHP_VM_FINAL_METHOD_OVERRIDE: ")?
        .strip_prefix("Cannot override final method ")?
        .split_once("()")?
        .0;
    split_class_method(target)
}

fn vm_compile_error_traversable_direct(message: &str) -> Option<String> {
    let rest = message.strip_prefix("E_PHP_VM_TRAVERSABLE_DIRECT_IMPLEMENTATION: ")?;
    Some(
        rest.strip_prefix("Class ")?
            .split_once(" must implement interface Traversable ")?
            .0
            .to_owned(),
    )
}

fn vm_compile_error_child_property(message: &str) -> Option<(String, String)> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_PROPERTY_VISIBILITY_OVERRIDE: ") {
        let target = rest
            .strip_prefix("Access level to ")?
            .split_once(" must be ")?
            .0;
        return split_class_property(target);
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_PROPERTY_STATIC_OVERRIDE: ") {
        let target = rest
            .split_once(" as static ")
            .or_else(|| rest.split_once(" as non static "))?
            .1;
        return split_class_property(target);
    }

    None
}

fn vm_compile_error_child_constant(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE: ")?;
    let target = rest
        .strip_prefix("Access level to ")?
        .split_once(" must be ")?
        .0;
    split_class_constant(target)
}

fn split_class_method(target: &str) -> Option<(String, String)> {
    let (class_name, method_name) = target.rsplit_once("::")?;
    let method_name = method_name
        .split_once('(')
        .map_or(method_name, |(name, _)| name);
    Some((class_name.to_owned(), method_name.to_owned()))
}

fn split_class_property(target: &str) -> Option<(String, String)> {
    let (class_name, property_name) = target.rsplit_once("::$")?;
    Some((class_name.to_owned(), property_name.to_owned()))
}

fn split_class_constant(target: &str) -> Option<(String, String)> {
    let (class_name, constant_name) = target.rsplit_once("::")?;
    Some((class_name.to_owned(), constant_name.to_owned()))
}

fn class_span(unit: &IrUnit, class_name: &str) -> Option<php_ir::IrSpan> {
    let normalized_class = normalize_class_name(class_name);
    unit.classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)
        .map(|class| class.span)
}

fn class_method_span(unit: &IrUnit, class_name: &str, method_name: &str) -> Option<php_ir::IrSpan> {
    let normalized_class = normalize_class_name(class_name);
    let normalized_method = method_name.to_ascii_lowercase();
    let class = unit
        .classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)?;
    let method = class
        .methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(&normalized_method))?;
    unit.functions
        .get(method.function.index())
        .map(|function| function.span)
}

fn class_constant_span(
    unit: &IrUnit,
    class_name: &str,
    constant_name: &str,
) -> Option<php_ir::IrSpan> {
    let normalized_class = normalize_class_name(class_name);
    let class = unit
        .classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)?;
    class
        .constants
        .iter()
        .find(|constant| constant.name.eq_ignore_ascii_case(constant_name))
        .map(|constant| constant.span)
}

fn overriding_method_span(
    unit: &IrUnit,
    parent_class: &str,
    method_name: &str,
) -> Option<php_ir::IrSpan> {
    let normalized_method = method_name.to_ascii_lowercase();
    unit.classes.iter().find_map(|class| {
        if !unit_class_extends(unit, class, parent_class) {
            return None;
        }
        let method = class
            .methods
            .iter()
            .find(|method| method.name.eq_ignore_ascii_case(&normalized_method))?;
        unit.functions
            .get(method.function.index())
            .map(|function| function.span)
    })
}

fn unit_class_extends(unit: &IrUnit, class: &php_ir::ClassEntry, parent_class: &str) -> bool {
    let normalized_parent = normalize_class_name(parent_class);
    let mut next = class.parent.as_deref();
    while let Some(name) = next {
        if normalize_class_name(name) == normalized_parent {
            return true;
        }
        next = unit
            .classes
            .iter()
            .find(|candidate| normalize_class_name(&candidate.name) == normalize_class_name(name))
            .and_then(|candidate| candidate.parent.as_deref());
    }
    false
}

fn write_php_parse_error_line<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &SourceText,
    span: TextRange,
    message: &str,
) -> Result<(), String> {
    let line = line_number_for_span(source, span);
    writeln!(stderr, "Parse error: {message} in {path} on line {line}")
        .map_err(|error| error.to_string())
}

fn line_number_for_span(source: &SourceText, span: TextRange) -> usize {
    source.line_col(span.start()).line
}

fn write_runtime_diagnostics<W: Write>(
    stderr: &mut W,
    path: &str,
    diagnostics: &[php_runtime::RuntimeDiagnostic],
) -> Result<(), String> {
    for diagnostic in diagnostics {
        writeln!(
            stderr,
            "{path}: runtime-diagnostic: {}",
            diagnostic.to_json()
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_span_line<W: Write>(
    stderr: &mut W,
    path: &str,
    span: TextRange,
    id: &str,
    message: &str,
) -> Result<(), String> {
    writeln!(
        stderr,
        "{}:{}..{}: {}: {}",
        path,
        span.start().to_usize(),
        span.end().to_usize(),
        id,
        message
    )
    .map_err(|error| error.to_string())
}

fn write_parser_diagnostic<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &SourceText,
    span: TextRange,
    id: &str,
    message: &str,
) -> Result<(), String> {
    if message.starts_with("syntax error,") {
        write_php_parse_error_line(stderr, path, source, span, message)
    } else {
        write_span_line(stderr, path, span, id, message)
    }
}

fn semantic_diagnostic_uses_php_fatal_line(id: DiagnosticId) -> bool {
    matches!(
        id,
        DiagnosticId::ClosureUseDuplicatesParameter
            | DiagnosticId::DuplicateClosureUseVariable
            | DiagnosticId::ClosureUseAutoGlobal
            | DiagnosticId::ThisParameter
            | DiagnosticId::ThisReassignment
    )
}

fn semantic_diagnostic_uses_php_parse_error_line(id: DiagnosticId) -> bool {
    matches!(id, DiagnosticId::InvalidClassConstantWrite)
}

fn semantic_diagnostic_is_immediate_php_fatal(id: DiagnosticId) -> bool {
    matches!(
        id,
        DiagnosticId::ThisParameter | DiagnosticId::ThisReassignment
    )
}

pub fn read_script(path: &Path) -> Result<(String, PathBuf, String), String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let real_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let source_path = real_path.to_string_lossy().into_owned();
    Ok((source, real_path, source_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_number_for_span_uses_one_based_source_lines() {
        let source = SourceText::new("<?php\nfunction f(callable&Traversable $x) {}\n");
        assert_eq!(line_number_for_span(&source, TextRange::new(6, 14)), 2);
    }

    #[test]
    fn php_fatal_line_matches_php_compile_error_shape() {
        let source = SourceText::new("<?php\nfunction f(callable&Traversable $x) {}\n");
        let mut stderr = Vec::new();

        write_php_fatal_line(
            &mut stderr,
            "fixture.php",
            &source,
            TextRange::new(6, 14),
            "Type callable cannot be part of an intersection type",
        )
        .expect("fatal line should render");

        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Type callable cannot be part of an intersection type in fixture.php on line 2\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_class_table_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass Base { public function show() {} }\nclass Child extends Base {\n    protected function show() {}\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Access level to child::show() must be public (as in class base) in fixture.php on line 4\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_property_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass Base { public static $p; }\nclass Child extends Base {\n    public $p;\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Cannot redeclare static Base::$p as non static Child::$p in fixture.php on line 3\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_final_class_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nfinal class Base {}\nclass Child extends Base {}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Class child cannot extend final class base in fixture.php on line 3\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_class_constant_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass Base { public const TOKEN = 1; }\nclass Child extends Base {\n    protected const TOKEN = 2;\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Access level to Child::TOKEN must be public (as in class Base) in fixture.php on line 3\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_interface_signature_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\ninterface Contract { public function __construct(); }\nclass Child implements Contract {\n    public function __construct($value) {}\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Declaration of Child::__construct($value) must be compatible with Contract::__construct() in fixture.php on line 4\n"
        );
    }

    #[test]
    fn execute_php_renders_direct_traversable_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass test implements Traversable {\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Class test must implement interface Traversable as part of either Iterator or IteratorAggregate in fixture.php on line 2\n"
        );
    }

    #[test]
    fn execute_php_renders_invalid_const_expr_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass C { const BAD = \"$name\"; }\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Constant expression contains invalid operations in fixture.php on line 2\n"
        );
    }
}
