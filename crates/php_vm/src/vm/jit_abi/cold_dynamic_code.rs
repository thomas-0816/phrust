//! Baseline-native include, eval, and runtime declaration continuation.

use super::*;
use php_runtime::api::Value;

thread_local! {
    pub(super) static BASELINE_INCLUDE_GLOBALS: RefCell<Option<std::collections::BTreeMap<String, Value>>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_CONSTANTS: RefCell<Option<std::collections::BTreeMap<String, Value>>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_INI: RefCell<Option<php_runtime::api::IniRegistry>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_DEFAULT_TIMEZONE: RefCell<Option<String>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_HTTP_RESPONSE: RefCell<Option<php_runtime::api::RuntimeHttpResponseState>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_FILES: RefCell<Option<std::collections::BTreeSet<std::path::PathBuf>>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_MYSQL: RefCell<Option<std::rc::Rc<RefCell<php_runtime::api::MysqlState>>>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_FILTER_INPUT_ARRAYS: RefCell<Option<Rc<std::collections::BTreeMap<i64, php_runtime::api::PhpArray>>>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_FUNCTION_NAMES: RefCell<Option<Rc<NativeFunctionNameScope>>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_SYMBOLS: RefCell<Option<NativeIncludeSymbols>> =
        const { RefCell::new(None) };
    pub(super) static BASELINE_INCLUDE_EXPORTS: RefCell<Option<NativeIncludeExports>> =
        const { RefCell::new(None) };
}

enum NativeIncludeFailure {
    Resolution(String),
    Execution(NativeCallControl),
}

struct NativeIncludeLocalBinding {
    name: String,
    reference: i64,
    caller: Option<(usize, bool)>,
}

fn native_include_exports(compiled: &crate::compiled_unit::CompiledUnit) -> NativeIncludeExports {
    let unit = compiled.unit();
    let entry_file = unit
        .functions
        .get(unit.entry.index())
        .map(|function| function.span.file);
    let functions = unit
        .function_table
        .iter()
        .map(|entry| (entry.name.clone(), entry.function))
        .collect();
    let classes = unit
        .classes
        .iter()
        .filter(|class| {
            !class.flags.is_conditional
                && (class.span.start != 0 || class.span.end != 0)
                && entry_file.is_none_or(|file| class.span.file == file)
        })
        .map(|class| class.name.clone())
        .collect();
    let constants = unit
        .constant_table
        .iter()
        .filter(|entry| entry_file.is_none_or(|file| entry.span.file == file))
        .filter_map(|entry| {
            unit.constants
                .get(entry.value.index())
                .and_then(|value| ir_constant_value(value).ok())
                .map(|value| (entry.name.clone(), value))
        })
        .collect();
    NativeIncludeExports {
        functions,
        native_entries: std::sync::Arc::new(std::collections::BTreeMap::new()),
        native_entry_signature_hashes: std::collections::BTreeMap::new(),
        classes,
        constants,
        autoload_callbacks: Vec::new(),
        shutdown_callbacks: Vec::new(),
    }
}

fn native_include_local_is_superglobal(name: &str) -> bool {
    matches!(
        name,
        "_GET" | "_POST" | "_COOKIE" | "_REQUEST" | "_SERVER" | "_ENV" | "_FILES" | "_SESSION"
    )
}

// SAFETY: caller_frame is a synchronous generated-code frame described by the
// caller function's verified local table.
#[allow(unsafe_code)] // Safety: the active cold request owns the raw VM state for this synchronous continuation.
fn prepare_native_include_local_bindings(
    context: &mut NativeRequestColdState<'_>,
    compiled: &crate::compiled_unit::CompiledUnit,
    request: &php_jit::JitNativeDynamicCodeRequest,
) -> Result<Vec<NativeIncludeLocalBinding>, String> {
    let caller = context
        .unit
        .functions
        .get(request.caller_function_id as usize)
        .ok_or_else(|| "native include caller function is missing".to_owned())?;
    if caller.flags.is_top_level {
        return Ok(Vec::new());
    }
    if request.caller_frame == 0 && !caller.locals.is_empty() {
        return Err("function-scoped native include has no caller-local frame".to_owned());
    }
    let caller_locals = caller.locals.clone();
    let include_locals = compiled
        .unit()
        .functions
        .get(compiled.unit().entry.index())
        .map(|entry| entry.locals.clone())
        .ok_or_else(|| "native include entry function is missing".to_owned())?;
    let caller_frame = request.caller_frame as *const i64;
    let mut bindings = Vec::new();
    for name in include_locals {
        if name == "GLOBALS"
            || php_ir::is_compiler_generated_local_name(&name)
            || native_include_local_is_superglobal(&name)
        {
            continue;
        }
        let caller_index = caller_locals
            .iter()
            .position(|candidate| candidate == &name);
        let source = caller_index.map_or_else(
            || php_jit::jit_encode_constant(php_jit::JIT_VALUE_UNINITIALIZED),
            |index| {
                // SAFETY: the generated frame contains one i64 slot for each
                // verified caller local for this synchronous ABI call.
                unsafe { caller_frame.add(index).read() }
            },
        );
        let preserve_reference = context.php_handle_is_reference(source) == Some(true);
        let reference = if preserve_reference {
            context.retain(source)?;
            source
        } else {
            let payload = context
                .duplicate_authoritative_native_value(source)?
                .ok_or_else(|| {
                    format!(
                        "function-scoped include local ${name} has no authoritative native value"
                    )
                })?;
            match context.encode_direct_reference_payload_owned(payload) {
                Ok(reference) => reference,
                Err(error) => {
                    context.release_if_live(payload)?;
                    return Err(error);
                }
            }
        };
        bindings.push(NativeIncludeLocalBinding {
            name,
            reference,
            caller: caller_index.map(|index| (index, preserve_reference)),
        });
    }
    Ok(bindings)
}

fn release_native_include_local_bindings(
    context: &mut NativeRequestColdState<'_>,
    bindings: &[NativeIncludeLocalBinding],
) -> Result<(), String> {
    let mut first_error = None;
    for binding in bindings {
        if let Err(error) = context.release_if_live(binding.reference) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn publish_native_dynamic_unit(
    context: &mut NativeRequestColdState<'_>,
    compiled: crate::compiled_unit::CompiledUnit,
    request: &php_jit::JitNativeDynamicCodeRequest,
    resolution: &mut php_jit::JitNativeDynamicUnitResolution,
    implicit_include_return: bool,
) -> Result<(), NativeIncludeFailure> {
    for declaration in compiled
        .unit()
        .linked_entry_autoload_declarations
        .iter()
        .flatten()
    {
        if !native_external_class_exists(context, declaration) {
            return Err(NativeIncludeFailure::Resolution(format!(
                "dynamic unit requires generated autoload continuation for {declaration}"
            )));
        }
    }
    let bindings = prepare_native_include_local_bindings(context, &compiled, request)
        .map_err(|error| NativeIncludeFailure::Execution(error.into()))?;
    if bindings.len() > resolution.include_binding_capacity as usize
        || (!bindings.is_empty() && resolution.include_binding_plan == 0)
    {
        let _ = release_native_include_local_bindings(context, &bindings);
        return Err(NativeIncludeFailure::Execution(
            "dynamic include binding plan exceeds caller storage".into(),
        ));
    }
    let mut stabilized = bindings
        .iter()
        .map(|binding| binding.reference)
        .collect::<Vec<_>>();
    if let Err(error) = context.stabilize_owned_native_values_for_cross_unit(&mut stabilized) {
        let _ = release_native_include_local_bindings(context, &bindings);
        return Err(NativeIncludeFailure::Execution(error.into()));
    }
    debug_assert!(
        bindings
            .iter()
            .zip(&stabilized)
            .all(|(binding, value)| binding.reference == *value),
        "dynamic-unit lvalue identities must remain stable"
    );

    let entry = compiled.unit().entry;
    let exports = native_include_exports(&compiled);
    let unit = match register_native_dynamic_unit(context, compiled, exports) {
        Ok(unit) => unit,
        Err(error) => {
            let _ = release_native_include_local_bindings(context, &bindings);
            return Err(NativeIncludeFailure::Execution(error.into()));
        }
    };
    if let Err(error) = prepare_dynamic_native_entry(context, unit, entry) {
        let _ = release_native_include_local_bindings(context, &bindings);
        return Err(NativeIncludeFailure::Execution(error.into()));
    }
    let slots = bindings
        .iter()
        .map(|binding| (binding.name.clone(), binding.reference))
        .collect::<Vec<_>>();
    if let Err(error) = context.with_active_dynamic_unit(unit, Some(&slots), |_| ()) {
        let _ = release_native_include_local_bindings(context, &bindings);
        return Err(NativeIncludeFailure::Execution(error.into()));
    }

    let package = context.dynamic_units.get(unit).ok_or_else(|| {
        NativeIncludeFailure::Execution("published dynamic unit is missing".into())
    })?;
    let deployment = package.compiled.prepared_deployment_image();
    let generic = deployment
        .generic_function_entries
        .get(entry.index())
        .ok_or_else(|| {
            NativeIncludeFailure::Execution("dynamic Generic entry cell is missing".into())
        })?;
    let preferred = deployment
        .preferred_function_entries
        .get(entry.index())
        .ok_or_else(|| {
            NativeIncludeFailure::Execution("dynamic preferred entry cell is missing".into())
        })?;
    if generic.load(std::sync::atomic::Ordering::Acquire) == 0
        || preferred.load(std::sync::atomic::Ordering::Acquire) == 0
        || package.published_runtime_view.abi_version != php_jit::JIT_RUNTIME_ABI_VERSION
    {
        let _ = release_native_include_local_bindings(context, &bindings);
        return Err(NativeIncludeFailure::Execution(
            "dynamic unit publication is incomplete".into(),
        ));
    }

    // SAFETY: generated code owns this synchronous stack plan and advertises
    // one record of capacity for every caller local.
    #[allow(unsafe_code)] // Safety: the generated stack plan owns this synchronous record range.
    unsafe {
        let records =
            resolution.include_binding_plan as usize as *mut php_jit::JitNativeDynamicBinding;
        for (index, binding) in bindings.iter().enumerate() {
            records.add(index).write(php_jit::JitNativeDynamicBinding {
                caller_slot: binding.caller.map_or(u32::MAX, |(slot, _)| {
                    u32::try_from(slot).unwrap_or(u32::MAX)
                }),
                flags: binding.caller.map_or(0, |(_, preserve)| {
                    u32::from(preserve) * php_jit::JitNativeDynamicBinding::PRESERVE_REFERENCE
                }),
                reference: binding.reference,
            });
        }
    }
    resolution.abi_version = php_jit::JIT_RUNTIME_ABI_VERSION;
    resolution.struct_size =
        u32::try_from(std::mem::size_of::<php_jit::JitNativeDynamicUnitResolution>())
            .unwrap_or(u32::MAX);
    resolution.action = php_jit::JitNativeDynamicUnitAction::INVOKE;
    resolution.flags = u32::from(implicit_include_return)
        * php_jit::JitNativeDynamicUnitResolution::IMPLICIT_INCLUDE_RETURN;
    resolution.unit_identity = package.compiled.artifact_identity();
    resolution.generic_entry_cell = std::ptr::from_ref(generic) as usize as u64;
    resolution.preferred_entry_cell = std::ptr::from_ref(preferred) as usize as u64;
    resolution.runtime_view =
        std::ptr::from_ref(package.published_runtime_view.as_ref()) as usize as u64;
    resolution.include_binding_count = u32::try_from(bindings.len()).unwrap_or(u32::MAX);
    resolution.export_generation = context.external_signature_epoch;
    resolution.declaration_generation = context.external_signature_epoch;
    release_native_include_local_bindings(context, &bindings)
        .map_err(|error| NativeIncludeFailure::Execution(error.into()))
}

fn resolve_native_include_unit(
    context: &mut NativeRequestColdState<'_>,
    request: &php_jit::JitNativeDynamicCodeRequest,
    resolution: &mut php_jit::JitNativeDynamicUnitResolution,
) -> Result<(), NativeIncludeFailure> {
    let path = String::from_utf8_lossy(
        &native_string(
            context
                .decode_baseline_value(request.source.payload as i64)
                .map_err(|error| NativeIncludeFailure::Execution(error.into()))?,
        )
        .map_err(|error| NativeIncludeFailure::Execution(error.into()))?,
    )
    .into_owned();
    let loader = context.options.include_loader.clone().ok_or_else(|| {
        NativeIncludeFailure::Resolution(
            "E_PHP_VM_INCLUDE_DISABLED: include loader is unavailable".to_owned(),
        )
    })?;
    let compiler = context.options.include_compiler.clone().ok_or_else(|| {
        NativeIncludeFailure::Resolution(
            "E_PHP_VM_INCLUDE_COMPILER: include compiler is unavailable".to_owned(),
        )
    })?;
    let cache = context.options.include_cache.clone();
    let including_file = context
        .unit
        .functions
        .get(request.caller_function_id as usize)
        .and_then(|function| context.unit.files.get(function.span.file.index()))
        .map(|file| std::path::PathBuf::from(&file.path));
    let resolved = if let Some(cache) = &cache {
        cache.resolve_with_include_path(
            &loader,
            including_file.as_deref(),
            &path,
            &context.include_path,
            Some(&context.cwd),
        )
    } else {
        loader.resolve_with_include_path(
            including_file.as_deref(),
            &path,
            &context.include_path,
            Some(&context.cwd),
        )
    }
    .map_err(|error| NativeIncludeFailure::Resolution(error.to_string()))?;
    let once = matches!(
        request.kind,
        php_jit::JitNativeDynamicCodeKind::INCLUDE_ONCE
            | php_jit::JitNativeDynamicCodeKind::REQUIRE_ONCE
    );
    if once && context.included_files.contains(&resolved.canonical_path) {
        resolution.action = php_jit::JitNativeDynamicUnitAction::COMPLETE;
        resolution.control_status = php_jit::JitCallStatus::RETURN;
        resolution.control_value = 1;
        return Ok(());
    }
    let compiled = if let Some(cache) = &cache {
        cache
            .get_or_compile_include(&loader, &resolved, compiler.as_ref())
            .map_err(|error| NativeIncludeFailure::Resolution(error.to_string()))?
    } else {
        let source = loader
            .load_validated_resolved(&resolved)
            .map_err(|error| NativeIncludeFailure::Resolution(error.to_string()))?;
        std::sync::Arc::new(
            compiler
                .compile_include(source, &loader)
                .map_err(|error| NativeIncludeFailure::Resolution(error.to_string()))?
                .unit,
        )
    };
    context.included_files.insert(resolved.canonical_path);
    let implicit = native_include_uses_implicit_return(compiled.unit());
    publish_native_dynamic_unit(context, (*compiled).clone(), request, resolution, implicit)
}

fn resolve_native_eval_unit(
    context: &mut NativeRequestColdState<'_>,
    request: &php_jit::JitNativeDynamicCodeRequest,
    resolution: &mut php_jit::JitNativeDynamicUnitResolution,
) -> Result<(), NativeIncludeFailure> {
    let source = String::from_utf8_lossy(
        &native_string(
            context
                .decode_baseline_value(request.source.payload as i64)
                .map_err(|error| NativeIncludeFailure::Execution(error.into()))?,
        )
        .map_err(|error| NativeIncludeFailure::Execution(error.into()))?,
    )
    .into_owned();
    let compiler = context.options.include_compiler.clone().ok_or_else(|| {
        NativeIncludeFailure::Resolution(
            "E_PHP_VM_INCLUDE_COMPILER: eval compiler is unavailable".to_owned(),
        )
    })?;
    let caller = context
        .unit
        .functions
        .get(request.caller_function_id as usize)
        .ok_or_else(|| NativeIncludeFailure::Execution("native eval caller is missing".into()))?;
    let instruction =
        context.instruction_for_continuation(request.caller_function_id, request.continuation_id);
    let line = instruction
        .as_ref()
        .map_or(1, |instruction| native_source_line(context, instruction));
    let file = instruction
        .as_ref()
        .map(|instruction| instruction.span.file)
        .unwrap_or(caller.span.file);
    let path = context.unit.files.get(file.index()).map_or_else(
        || "<eval>".to_owned(),
        |file| format!("{}({line}) : eval()'d code", file.path),
    );
    let compiled = compiler
        .compile_eval(&path, &source)
        .map_err(|error| NativeIncludeFailure::Resolution(error.to_string()))?;
    publish_native_dynamic_unit(context, compiled, request, resolution, false)
}

fn finish_native_dynamic_call_control(
    context: &mut NativeRequestColdState<'_>,
    control: NativeCallControl,
) -> (php_jit::JitCallStatus, Option<i64>) {
    match control {
        NativeCallControl::Rethrow => {
            let value = context
                .take_pending_throwable()
                .and_then(|throwable| context.encode_baseline_value(throwable).ok());
            (php_jit::JitCallStatus::THROW, value)
        }
        NativeCallControl::Throw { class, message } => (
            php_jit::JitCallStatus::THROW,
            encode_native_throwable(context, &class, &message).ok(),
        ),
        NativeCallControl::ArgumentCount {
            function,
            passed,
            required,
            target_span,
        } => {
            let message =
                native_argument_count_message(context, &function, passed, required, target_span);
            (
                php_jit::JitCallStatus::THROW,
                encode_native_throwable_at(context, "ArgumentCountError", &message, target_span)
                    .ok(),
            )
        }
        NativeCallControl::SuspendFiber => (
            php_jit::JitCallStatus::SUSPEND_FIBER,
            context.pending_fiber_suspension_value.take(),
        ),
        NativeCallControl::Exit(value) => (php_jit::JitCallStatus::EXIT, Some(value)),
        NativeCallControl::PublishedRuntimeError => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
        NativeCallControl::RuntimeError(message) => {
            publish_native_call_diagnostic(context, message);
            (php_jit::JitCallStatus::RUNTIME_ERROR, None)
        }
    }
}

fn render_native_include_failure(
    context: &mut NativeRequestColdState<'_>,
    request: &php_jit::JitNativeDynamicCodeRequest,
    _message: &str,
) -> Result<i64, String> {
    let path = String::from_utf8_lossy(&native_string(
        context.decode_baseline_value(request.source.payload as i64)?,
    )?)
    .into_owned();
    let source_path = context
        .unit
        .files
        .first()
        .map_or_else(|| "<unknown>".to_owned(), |file| file.path.clone());
    let (line, span_start, span_end) = context
        .instruction_for_continuation(request.caller_function_id, request.continuation_id)
        .map_or((1, 0, 0), |instruction| {
            (
                native_source_line(context, &instruction),
                instruction.span.start,
                instruction.span.end,
            )
        });
    let require = request.kind == php_jit::JitNativeDynamicCodeKind::REQUIRE
        || request.kind == php_jit::JitNativeDynamicCodeKind::REQUIRE_ONCE;
    if !require {
        if context.error_reporting & 2 != 0 {
            context.output.write_bytes(format!(
                "\nWarning: include({path}): Failed to open stream: No such file or directory in {source_path} on line {line}\n\nWarning: include(): Failed opening '{path}' for inclusion (include_path='.:') in {source_path} on line {line}\n"
            ));
        }
        return context.encode_baseline_value(Value::Bool(false));
    }
    let fatal = format!("Failed opening required '{path}' (include_path='.:')");
    if context.error_reporting & 2 != 0 {
        context.output.write_bytes(format!(
            "\nWarning: require({path}): Failed to open stream: No such file or directory in {source_path} on line {line}\n"
        ));
    }
    context.output.write_bytes(format!(
        "\nFatal error: Uncaught Error: {fatal} in {source_path}:{line}\nStack trace:\n#0 {{main}}\n  thrown in {source_path} on line {line}\n"
    ));
    context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
        "E_PHP_VM_REQUIRE_FAILED",
        php_runtime::api::RuntimeSeverity::FatalError,
        fatal.clone(),
        php_runtime::api::RuntimeSourceSpan {
            file: Some(source_path),
            start: span_start,
            end: span_end,
        },
        Vec::new(),
        None,
    ));
    Err(fatal)
}

/// Native dynamic-code publication boundary. Includes and eval units are
/// resolved, compiled, and published here; generated code invokes the entry.
// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_cold_dynamic_unit_resolve_abi(
    runtime: *mut NativeRequestFastState,
    _vm_context: u64,
    request: *mut php_jit::JitNativeDynamicCodeRequest,
    out: *mut php_jit::JitNativeDynamicUnitResolution,
) -> i32 {
    if request.is_null() || out.is_null() {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let _ = with_baseline_native_context_for(runtime, "dynamic_code", |context| {
        context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: Generated code owns these records for the synchronous call.
        let request = unsafe { &*request };
        let resolution = unsafe { &mut *out };
        if request.abi_version != php_jit::JIT_RUNTIME_ABI_VERSION
            || request.struct_size as usize
                != std::mem::size_of::<php_jit::JitNativeDynamicCodeRequest>()
        {
            (php_jit::JitCallStatus::ABI_MISMATCH, None)
        } else if matches!(
            request.kind,
            php_jit::JitNativeDynamicCodeKind::INCLUDE
                | php_jit::JitNativeDynamicCodeKind::INCLUDE_ONCE
                | php_jit::JitNativeDynamicCodeKind::REQUIRE
                | php_jit::JitNativeDynamicCodeKind::REQUIRE_ONCE
        ) {
            with_baseline_native_context_for(runtime, "dynamic_unit_resolve", |context| {
                match resolve_native_include_unit(context, request, resolution) {
                    Ok(()) => (php_jit::JitCallStatus::RETURN, None),
                    Err(NativeIncludeFailure::Execution(control)) => {
                        finish_native_dynamic_call_control(context, control)
                    }
                    Err(NativeIncludeFailure::Resolution(message)) => {
                        match render_native_include_failure(context, request, &message) {
                            Ok(value) => {
                                resolution.action =
                                    php_jit::JitNativeDynamicUnitAction::COMPLETE;
                                (php_jit::JitCallStatus::RETURN, Some(value))
                            }
                            Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                        }
                    }
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::EVAL {
            with_baseline_native_context_for(runtime, "dynamic_unit_resolve", |context| {
                match resolve_native_eval_unit(context, request, resolution) {
                    Ok(()) => (php_jit::JitCallStatus::RETURN, None),
                    Err(NativeIncludeFailure::Execution(control)) => {
                        finish_native_dynamic_call_control(context, control)
                    }
                    Err(NativeIncludeFailure::Resolution(message)) => {
                        publish_native_call_diagnostic(context, message);
                        (php_jit::JitCallStatus::RUNTIME_ERROR, None)
                    }
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::DECLARE_FUNCTION {
            with_baseline_native_context_for(runtime, "dynamic_code", |context| {
                let function = php_ir::FunctionId::new(request.declared_function_id);
                let Some(target) = context.unit.functions.get(function.index()) else {
                    return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                };
                let normalized = target.name.to_ascii_lowercase();
                let by_reference = target.params.iter().any(|parameter| parameter.by_ref);
                if context.deployment_functions.contains_key(normalized.as_str())
                    || context.dynamic_functions.contains_key(&normalized)
                    || context.external_functions.contains_key(&normalized)
                {
                    publish_native_call_diagnostic(
                        context,
                        format!("Cannot redeclare function {}()", target.name),
                    );
                    return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                }
                if let Some(unit) = context.current_dynamic_unit {
                    context
                        .external_functions
                        .insert(normalized.clone(), NativeDynamicFunction { unit, function });
                    if by_reference {
                        context.external_signature_epoch =
                            context.external_signature_epoch.saturating_add(1);
                    }
                } else {
                    context
                        .dynamic_functions
                        .insert(normalized.clone(), function);
                }
                context.publish_function_names([normalized]);
                match context.encode_baseline_value(Value::Null) {
                    Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                    Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::DECLARE_CLASS {
            with_baseline_native_context_for(runtime, "dynamic_code", |context| {
                let class =
                    context.unit.classes.iter().find(|class| {
                        stable_native_symbol_hash(&class.name) == request.symbol_hash
                    });
                let Some(class) = class else {
                    return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                };
                let normalized = normalize_class_name(&class.name);
                if context.deployment_classes.contains(normalized.as_str())
                    || context.dynamic_classes.contains(&normalized)
                    || context.external_class_units.contains_key(&normalized)
                {
                    publish_native_call_diagnostic(
                        context,
                        format!(
                            "Cannot declare class {}, because the name is already in use",
                            class.name
                        ),
                    );
                    return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                }
                if let Some(unit) = context.current_dynamic_unit {
                    context
                        .external_class_units
                        .insert(normalized.clone(), unit);
                }
                context.dynamic_classes.insert(normalized);
                match context.encode_baseline_value(Value::Null) {
                    Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                    Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::REGISTER_CONSTANT {
            with_baseline_native_context_for(runtime, "dynamic_code", |context| {
                let instruction = context
                    .instruction_for_continuation(
                        request.caller_function_id,
                        request.continuation_id,
                    );
                let Some(instruction) = instruction else {
                    return (php_jit::JitCallStatus::ABI_MISMATCH, None);
                };
                let php_ir::InstructionKind::RegisterConstant { name, .. } = &instruction.kind
                else {
                    return (php_jit::JitCallStatus::ABI_MISMATCH, None);
                };
                if stable_native_symbol_hash(name) != request.symbol_hash {
                    return (php_jit::JitCallStatus::ABI_MISMATCH, None);
                }
                let value = match context.decode_baseline_value(request.source.payload as i64) {
                    Ok(value) => dereference_native_assignment_value(value),
                    Err(_) => return (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                };
                if context.lookup_constant(name).is_ok() {
                    let path = context
                        .unit
                        .files
                        .get(instruction.span.file.index())
                        .map_or("<unknown>", |file| file.path.as_str());
                    let line = native_source_line(context, &instruction);
                    context.output.write_bytes(format!(
                        "\nWarning: Constant {name} already defined, this will be an error in PHP 9 in {path} on line {line}\n"
                    ));
                } else {
                    if context
                        .insert_dynamic_constant(name.clone(), value)
                        .is_err()
                    {
                        return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                    }
                }
                match context.encode_baseline_value(Value::Null) {
                    Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                    Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::EMIT_DIAGNOSTIC {
            with_baseline_native_context_for(runtime, "dynamic_code", |context| {
                let instruction = context
                    .instruction_for_continuation(
                        request.caller_function_id,
                        request.continuation_id,
                    );
                let Some(instruction) = instruction else {
                    return (php_jit::JitCallStatus::ABI_MISMATCH, None);
                };
                let php_ir::InstructionKind::EmitDiagnostic {
                    severity,
                    message,
                    leading_newline,
                    ..
                } = &instruction.kind
                else {
                    return (php_jit::JitCallStatus::ABI_MISMATCH, None);
                };
                let errno = match severity {
                    php_ir::instruction::IrDiagnosticSeverity::Warning => 2,
                    php_ir::instruction::IrDiagnosticSeverity::Deprecation => 8192,
                };
                match emit_native_php_diagnostic(
                    context,
                    errno,
                    message,
                    &instruction,
                    *leading_newline,
                ) {
                    Ok(()) => match context.encode_baseline_value(Value::Null) {
                        Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                        Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                    },
                    Err(error) if error == "E_PHP_RETHROW" => {
                        let value = context
                            .take_pending_throwable()
                            .and_then(|value| context.encode_baseline_value(value).ok());
                        (php_jit::JitCallStatus::THROW, value)
                    }
                    Err(error) => {
                        publish_native_call_diagnostic(context, error);
                        (php_jit::JitCallStatus::RUNTIME_ERROR, None)
                    }
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::MAKE_CLOSURE {
            with_baseline_native_context_for(runtime, "dynamic_code", |context| {
                let captures = context
                    .instruction_for_continuation(
                        request.caller_function_id,
                        request.continuation_id,
                    )
                    .and_then(|instruction| match &instruction.kind {
                        php_ir::InstructionKind::MakeClosure { captures, .. } => {
                            Some(captures.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                let caller_frame = request.caller_frame as *mut i64;
                let mut captured_values = Vec::with_capacity(captures.len());
                let capture_descriptors = captures
                    .iter()
                    .map(|capture| (capture.name.clone(), capture.by_ref))
                    .collect::<Vec<_>>();
                for capture in &captures {
                    let php_ir::Operand::Local(local) = capture.src else {
                        for captured in captured_values {
                            let _ = context.release_if_live(captured);
                        }
                        return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                    };
                    if caller_frame.is_null() {
                        for captured in captured_values {
                            let _ = context.release_if_live(captured);
                        }
                        return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                    }
                    // SAFETY: generated code passes its live caller-local frame for the
                    // duration of this synchronous closure construction request.
                    let caller_slot = unsafe { caller_frame.add(local.index()) };
                    let encoded = unsafe { caller_slot.read() };
                    let captured = if capture.by_ref {
                        if context.php_handle_is_reference(encoded) == Some(true) {
                            match context.duplicate_authoritative_native_value(encoded) {
                                Ok(Some(reference)) => reference,
                                _ => {
                                    for captured in captured_values {
                                        let _ = context.release_if_live(captured);
                                    }
                                    return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                                }
                            }
                        } else {
                            // Ownership of the old local value moves into the
                            // direct reference payload. The caller frame keeps
                            // the first reference owner and the closure gets a
                            // retained owner for the same alias identity.
                            let reference =
                                match context.encode_direct_reference_payload_owned(encoded) {
                                    Ok(reference) => reference,
                                    Err(_) => {
                                        for captured in captured_values {
                                            let _ = context.release_if_live(captured);
                                        }
                                        return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                                    }
                                };
                            unsafe { caller_slot.write(reference) };
                            if context.retain(reference).is_err() {
                                for captured in captured_values {
                                    let _ = context.release_if_live(captured);
                                }
                                return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                            }
                            reference
                        }
                    } else {
                        match context
                            .duplicate_authoritative_dereferenced_native_value(encoded)
                        {
                            Ok(Some(value)) => value,
                            Ok(None) => {
                                let value = context
                                    .baseline_decode_dereferenced_native_value(encoded)
                                    .and_then(|value| context.encode_baseline_value(value));
                                match value {
                                    Ok(value) => value,
                                    Err(_) => {
                                        for captured in captured_values {
                                            let _ = context.release_if_live(captured);
                                        }
                                        return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                                    }
                                }
                            }
                            Err(_) => {
                                for captured in captured_values {
                                    let _ = context.release_if_live(captured);
                                }
                                return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                            }
                        }
                    };
                    captured_values.push(captured);
                }
                let debug = context
                    .unit
                    .functions
                    .get(request.declared_function_id as usize)
                    .and_then(|function| {
                        let file = context.unit.files.get(function.span.file.index())?;
                        let line = context
                            .compiled
                            .source_display_line(function.span, false)
                            .unwrap_or(1);
                        Some(php_runtime::api::ClosureDebugInfo {
                            name: format!("{{closure:{}:{line}}}", file.path),
                            file: file.path.clone(),
                            line,
                            parameters: function
                                .params
                                .iter()
                                .map(|parameter| php_runtime::api::ClosureDebugParameter {
                                    name: parameter.name.clone(),
                                    required: parameter.required,
                                })
                                .collect(),
                        })
                    });
                let scope_class =
                    native_effective_calling_class(context, request.caller_function_id)
                        .map(|class| std::sync::Arc::<str>::from(class.display_name.as_str()));
                let called_class = context
                    .called_classes
                    .last()
                    .cloned()
                    .or_else(|| scope_class.clone());
                let closure_context = php_runtime::api::ClosureContext {
                    owner_unit: context.current_dynamic_unit,
                    scope_class: scope_class.clone(),
                    called_class,
                    declaring_class: scope_class,
                };
                let closure =
                    php_runtime::api::ClosurePayload::new(request.declared_function_id, Vec::new())
                        .with_debug(debug)
                        .with_context(closure_context);
                let bound_this_local = php_jit::region_ir::native_closure_bound_this_local(
                    &context.unit,
                    php_ir::FunctionId::new(request.caller_function_id),
                    php_ir::FunctionId::new(request.declared_function_id),
                );
                let implicit_this = if let Some(bound_this_local) = bound_this_local {
                    if caller_frame.is_null() {
                        for captured in captured_values {
                            let _ = context.release_if_live(captured);
                        }
                        return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                    }
                    // SAFETY: publication resolved the exact `$this` local
                    // for this caller/Closure pair, and generated code passes
                    // its full live caller-local frame for this request.
                    let encoded = unsafe { caller_frame.add(bound_this_local.index()).read() };
                    let value = context
                        .duplicate_authoritative_dereferenced_native_value(encoded)
                        .and_then(|value| {
                            value.map_or_else(
                                || {
                                    context
                                        .baseline_decode_dereferenced_native_value(encoded)
                                        .and_then(|value| context.encode_baseline_value(value))
                                },
                                Ok,
                            )
                        });
                    match value {
                        Ok(value) => Some(value),
                        Err(_) => {
                            for captured in captured_values {
                                let _ = context.release_if_live(captured);
                            }
                            return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                        }
                    }
                } else {
                    None
                };
                match context.publish_prepared_closure_owned(NativePreparedClosure::new(
                    closure,
                    std::sync::Arc::from(capture_descriptors),
                    implicit_this,
                    captured_values.into_boxed_slice(),
                    None,
                    false,
                    false,
                    false,
                    false,
                )) {
                    Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                    Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else {
            (php_jit::JitCallStatus::COMPILE_REQUIRED, None)
        }
    }))
    .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None));
    let (status, value) = outcome;
    // Include/eval resolution writes INVOKE itself. Other exact publication
    // operations complete synchronously without executing a PHP body.
    let resolution = unsafe { &mut *out };
    resolution.abi_version = php_jit::JIT_RUNTIME_ABI_VERSION;
    resolution.struct_size =
        u32::try_from(std::mem::size_of::<php_jit::JitNativeDynamicUnitResolution>())
            .unwrap_or(u32::MAX);
    if resolution.action != php_jit::JitNativeDynamicUnitAction::INVOKE {
        resolution.action = if status == php_jit::JitCallStatus::RETURN {
            php_jit::JitNativeDynamicUnitAction::COMPLETE
        } else {
            php_jit::JitNativeDynamicUnitAction::CONTROL
        };
        resolution.control_status = status;
        resolution.control_detail = status.0;
        resolution.control_value = value.unwrap_or_default();
    }
    status.0 as i32
}
