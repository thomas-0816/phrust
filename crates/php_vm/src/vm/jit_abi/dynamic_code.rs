use super::*;

enum NativeDynamicCodeOutcome {
    Returned(i64),
    Exit(i64),
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
#[allow(unsafe_code)]
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

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
fn execute_native_include(
    context: &mut NativeRequestColdState<'_>,
    request: &php_jit::JitNativeDynamicCodeRequest,
) -> Result<NativeDynamicCodeOutcome, NativeIncludeFailure> {
    let path = String::from_utf8_lossy(
        &native_string(
            context
                .decode(request.source.payload as i64)
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
    let include_path = context.include_path.clone();
    let cwd = context.cwd.clone();
    let resolved = if let Some(cache) = &cache {
        cache
            .resolve_with_include_path(
                &loader,
                including_file.as_deref(),
                &path,
                &include_path,
                Some(&cwd),
            )
            .map_err(|error| NativeIncludeFailure::Resolution(error.to_string()))?
    } else {
        loader
            .resolve_with_include_path(including_file.as_deref(), &path, &include_path, Some(&cwd))
            .map_err(|error| NativeIncludeFailure::Resolution(error.to_string()))?
    };
    let once = request.kind == php_jit::JitNativeDynamicCodeKind::INCLUDE_ONCE
        || request.kind == php_jit::JitNativeDynamicCodeKind::REQUIRE_ONCE;
    if once && context.included_files.contains(&resolved.canonical_path) {
        return Ok(NativeDynamicCodeOutcome::Returned(1));
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
    if compiled
        .unit()
        .linked_entry_autoload_declarations
        .iter()
        .any(Option::is_some)
    {
        let source = context
            .instruction_for_continuation(request.caller_function_id, request.continuation_id)
            .ok_or_else(|| {
                NativeIncludeFailure::Execution("native include call metadata is missing".into())
            })?;
        for declaration in compiled
            .unit()
            .linked_entry_autoload_declarations
            .iter()
            .flatten()
        {
            if native_external_class_exists(context, declaration) {
                continue;
            }
            let normalized = normalize_class_name(declaration);
            if !context.autoload_in_progress.insert(normalized.clone()) {
                continue;
            }
            let callbacks = context.autoload_callbacks.clone();
            for callback in callbacks {
                if let Err(error) = invoke_native_callable_value(
                    context,
                    callback,
                    &[Value::String(PhpString::from_bytes(
                        declaration.as_bytes().to_vec(),
                    ))],
                    &source,
                    None,
                ) {
                    context.autoload_in_progress.remove(&normalized);
                    return Err(NativeIncludeFailure::Execution(error.into()));
                }
                if native_external_class_exists(context, declaration) {
                    break;
                }
            }
            context.autoload_in_progress.remove(&normalized);
        }
    }
    // PHP records every successfully resolved include target. A later
    // include_once/require_once must therefore skip a file that was first
    // loaded through plain include/require as well.
    context
        .included_files
        .insert(resolved.canonical_path.clone());
    let bindings = prepare_native_include_local_bindings(context, &compiled, request)
        .map_err(|error| NativeIncludeFailure::Execution(error.into()))?;
    let mut stabilized_bindings = bindings
        .iter()
        .map(|binding| binding.reference)
        .collect::<Vec<_>>();
    if let Err(error) =
        context.stabilize_owned_native_values_for_cross_unit(&mut stabilized_bindings)
    {
        let _ = release_native_include_local_bindings(context, &bindings);
        return Err(NativeIncludeFailure::Execution(error.into()));
    }
    debug_assert!(
        bindings
            .iter()
            .zip(&stabilized_bindings)
            .all(|(binding, stabilized)| binding.reference == *stabilized),
        "include lvalue identities must remain stable across unit activation"
    );
    let exports = native_include_exports(&compiled);
    let owner_unit = match register_native_dynamic_unit(context, (*compiled).clone(), exports) {
        Ok(unit) => unit,
        Err(error) => {
            let _ = release_native_include_local_bindings(context, &bindings);
            return Err(NativeIncludeFailure::Execution(error.into()));
        }
    };
    if let Err(error) = prepare_dynamic_native_entry(context, owner_unit, compiled.unit().entry) {
        let _ = release_native_include_local_bindings(context, &bindings);
        return Err(NativeIncludeFailure::Execution(error.into()));
    }
    let implicit_return = native_include_uses_implicit_return(compiled.unit());
    let binding_slots = bindings
        .iter()
        .map(|binding| (binding.name.clone(), binding.reference))
        .collect::<Vec<_>>();
    let active = context.with_active_dynamic_unit(
        owner_unit,
        Some(&binding_slots),
        |context| -> Result<(NativeCallResult, Vec<(usize, i64)>), String> {
            let result = match invoke_native_function(context, context.unit.entry, &[]) {
                Ok(value)
                    if implicit_return
                        && context.native_encoded_value_kind(value)
                            == Some(NativeEncodedValueKind::Null) =>
                {
                    context.release_if_live(value)?;
                    Ok(1)
                }
                Ok(value) => Ok(context.transfer_external_return(value, owner_unit)?),
                Err(NativeCallControl::Exit(value)) => Err(NativeCallControl::Exit(
                    context.transfer_external_return(value, owner_unit)?,
                )),
                Err(NativeCallControl::Propagate { status, value }) => {
                    Err(NativeCallControl::Propagate {
                        status,
                        value: context.transfer_external_return(value, owner_unit)?,
                    })
                }
                Err(control) => Err(control),
            };
            let mut active_bindings = bindings
                .iter()
                .map(|binding| binding.reference)
                .collect::<Vec<_>>();
            context.stabilize_owned_native_values_for_cross_unit(&mut active_bindings)?;
            let mut caller_values = Vec::new();
            for binding in &bindings {
                let Some((caller, preserve_reference)) = binding.caller else {
                    continue;
                };
                let value = context
                    .duplicate_active_entry_request_local(&binding.name, preserve_reference)?
                    .ok_or_else(|| {
                        format!(
                            "native include local ${} has no authoritative value",
                            binding.name
                        )
                    })?;
                caller_values.push((caller, value));
            }
            Ok((result, caller_values))
        },
    );
    let release_result = release_native_include_local_bindings(context, &bindings);
    let (result, caller_values) = match active {
        Ok(Ok(active)) => active,
        Ok(Err(error)) | Err(error) => {
            return Err(NativeIncludeFailure::Execution(error.into()));
        }
    };
    if let Err(error) = release_result {
        for (_, value) in caller_values {
            let _ = context.release_if_live(value);
        }
        return Err(NativeIncludeFailure::Execution(error.into()));
    }
    if request.caller_frame != 0 {
        let caller_frame = request.caller_frame as *mut i64;
        for (index, value) in caller_values {
            // SAFETY: this is the same live caller frame used to create the
            // direct lvalue bindings above.
            let previous = unsafe { caller_frame.add(index).read() };
            unsafe { caller_frame.add(index).write(value) };
            if let Err(error) = context.release_if_live(previous) {
                return Err(NativeIncludeFailure::Execution(error.into()));
            }
        }
    }
    match result {
        Ok(value) => Ok(NativeDynamicCodeOutcome::Returned(value)),
        Err(NativeCallControl::Exit(value)) => Ok(NativeDynamicCodeOutcome::Exit(value)),
        Err(control) => Err(NativeIncludeFailure::Execution(control)),
    }
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
fn execute_native_eval(
    context: &mut NativeRequestColdState<'_>,
    request: &php_jit::JitNativeDynamicCodeRequest,
) -> Result<NativeDynamicCodeOutcome, String> {
    let source = String::from_utf8_lossy(&native_string(
        context.decode(request.source.payload as i64)?,
    )?)
    .into_owned();
    let compiler = context
        .options
        .include_compiler
        .clone()
        .ok_or_else(|| "E_PHP_VM_INCLUDE_COMPILER: eval compiler is unavailable".to_owned())?;
    let (caller_locals, caller_file) = context
        .unit
        .functions
        .get(request.caller_function_id as usize)
        .map(|caller| (caller.locals.clone(), caller.span.file))
        .ok_or_else(|| "native eval caller function is missing".to_owned())?;
    let caller_instruction =
        context.instruction_for_continuation(request.caller_function_id, request.continuation_id);
    let caller_line = caller_instruction
        .as_ref()
        .map_or(1, |instruction| native_source_line(context, instruction));
    let caller_file = caller_instruction
        .as_ref()
        .map(|instruction| instruction.span.file)
        .unwrap_or(caller_file);
    let source_path = context.unit.files.get(caller_file.index()).map_or_else(
        || "<eval>".to_owned(),
        |file| format!("{}({caller_line}) : eval()'d code", file.path),
    );
    let compiled = compiler
        .compile_eval(&source_path, &source)
        .map_err(|error| error.to_string())?;
    context.materialize_native_request_globals()?;
    context.materialize_native_dynamic_constants()?;
    let mut inherited_globals = std::mem::take(&mut context.inherited_globals);
    if request.caller_frame != 0 {
        let caller_frame = request.caller_frame as *const i64;
        for (index, name) in caller_locals.iter().enumerate() {
            // SAFETY: Generated code owns this synchronous caller-local frame.
            let encoded = unsafe { caller_frame.add(index).read() };
            let value = context.decode(encoded)?;
            if matches!(value, Value::Uninitialized) {
                continue;
            }
            match inherited_globals.get(name).cloned() {
                Some(Value::Reference(reference)) => match value {
                    Value::Reference(replacement) if reference.ptr_eq(&replacement) => {}
                    Value::Reference(replacement) => {
                        inherited_globals.insert(name.clone(), Value::Reference(replacement));
                    }
                    replacement => reference.set(replacement),
                },
                _ => {
                    inherited_globals.insert(name.clone(), value);
                }
            }
        }
    }
    NATIVE_INCLUDE_GLOBALS.with(|globals| {
        globals.replace(Some(inherited_globals));
    });
    NATIVE_INCLUDE_CONSTANTS.with(|constants| {
        constants.replace(Some(std::mem::take(&mut context.dynamic_constants)));
    });
    NATIVE_INCLUDE_INI.with(|ini| {
        ini.replace(Some(std::mem::take(&mut context.ini_registry)));
    });
    NATIVE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| {
        timezone.replace(Some(std::mem::take(&mut context.default_timezone)));
    });
    NATIVE_INCLUDE_HTTP_RESPONSE.with(|response| {
        response.replace(Some(std::mem::take(&mut context.http_response)));
    });
    NATIVE_INCLUDE_FILES.with(|files| {
        files.replace(Some(std::mem::take(&mut context.included_files)));
    });
    NATIVE_INCLUDE_MYSQL.with(|mysql| {
        mysql.replace(Some(context.mysql_state.clone()));
    });
    NATIVE_INCLUDE_FILTER_INPUT_ARRAYS.with(|arrays| {
        arrays.replace(Some(Rc::clone(&context.filter_input_arrays)));
    });
    NATIVE_INCLUDE_FUNCTION_NAMES.with(|names| {
        names.replace(Some(context.visible_include_function_names()));
    });
    let external_signatures = visible_external_function_signatures_for_unit(context, &compiled);
    NATIVE_INCLUDE_SYMBOLS.with(|symbols| {
        symbols.replace(Some(context.take_include_symbols()?));
        Ok::<(), String>(())
    })?;
    NATIVE_INCLUDE_EXPORTS.with(|exports| {
        exports.take();
    });
    let dynamic_unit = compiled.clone();
    let nested_started_at = context
        .options
        .collect_counters
        .then(std::time::Instant::now);
    let result = super::super::Vm::with_options_and_worker_state(
        context.options.clone(),
        context.worker_state.clone(),
    )
    .execute_with_external_function_signatures(compiled, &external_signatures);
    if let (Some(started_at), Some(counters)) = (nested_started_at, result.counters.as_deref()) {
        context.merge_nested_runtime_counters(counters, started_at.elapsed());
    }
    let returned_globals =
        NATIVE_INCLUDE_GLOBALS.with(|globals| globals.borrow_mut().take().unwrap_or_default());
    context.dynamic_constants = NATIVE_INCLUDE_CONSTANTS
        .with(|constants| constants.borrow_mut().take().unwrap_or_default());
    context.prepare_trusted_constant_fetches();
    if let Some(returned_ini) = NATIVE_INCLUDE_INI.with(|ini| ini.borrow_mut().take()) {
        context.ini_registry = returned_ini;
    }
    if let Some(returned_timezone) =
        NATIVE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| timezone.borrow_mut().take())
    {
        context.default_timezone = returned_timezone;
    }
    if let Some(returned_response) =
        NATIVE_INCLUDE_HTTP_RESPONSE.with(|response| response.borrow_mut().take())
    {
        context.http_response = returned_response;
    }
    if let Some(returned_files) = NATIVE_INCLUDE_FILES.with(|files| files.borrow_mut().take()) {
        context.included_files = returned_files;
    }
    if let Some(returned_mysql) = NATIVE_INCLUDE_MYSQL.with(|mysql| mysql.borrow_mut().take()) {
        context.mysql_state = returned_mysql;
    }
    let returned_symbols =
        NATIVE_INCLUDE_SYMBOLS.with(|symbols| symbols.borrow_mut().take().unwrap_or_default());
    context.restore_include_symbols(returned_symbols);
    let exports = NATIVE_INCLUDE_EXPORTS.with(|exports| exports.borrow_mut().take());
    context.inherited_globals = returned_globals;
    context.reconcile_trusted_global_references()?;
    if request.caller_frame != 0 {
        let caller_frame = request.caller_frame as *mut i64;
        for (index, name) in caller_locals.iter().enumerate() {
            let Some(value) = context.inherited_globals.get(name).cloned() else {
                continue;
            };
            let encoded = context.encode(value)?;
            // SAFETY: This is the same synchronous caller-local frame read above.
            unsafe { caller_frame.add(index).write(encoded) };
        }
    }
    context.output.write_bytes(result.output.as_bytes());
    if let Some(exit_code) = result.process_exit_code {
        let value = context.encode(Value::Int(i64::from(exit_code)))?;
        return Ok(NativeDynamicCodeOutcome::Exit(value));
    }
    if !result.status.is_success() {
        context.diagnostic = result.diagnostics.into_iter().next();
        return Err(format!("evaluated native entry failed: {}", result.status));
    }
    let owner_unit = exports
        .map(|exports| register_native_dynamic_unit(context, dynamic_unit, exports))
        .transpose()?;
    context
        .encode(native_value_with_owner_unit(
            result.return_value.unwrap_or(Value::Null),
            owner_unit,
        ))
        .map(NativeDynamicCodeOutcome::Returned)
}

fn finish_native_dynamic_call_control(
    context: &mut NativeRequestColdState<'_>,
    control: NativeCallControl,
) -> (php_jit::JitCallStatus, Option<i64>) {
    match control {
        NativeCallControl::Rethrow => {
            let value = context
                .take_pending_throwable()
                .and_then(|throwable| context.encode(throwable).ok());
            (php_jit::JitCallStatus::THROW, value)
        }
        NativeCallControl::Throw { class, message } => (
            php_jit::JitCallStatus::THROW,
            encode_native_throwable(context, &class, &message).ok(),
        ),
        NativeCallControl::Propagate { status, value } => (status, Some(value)),
        NativeCallControl::SuspendFiber { .. } => (
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
        context.decode(request.source.payload as i64)?,
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
        return context.encode(Value::Bool(false));
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

fn invoke_native_include(
    runtime: *mut NativeRequestFastState,
    request: &php_jit::JitNativeDynamicCodeRequest,
) -> (php_jit::JitCallStatus, Option<i64>) {
    with_native_context_for(runtime, "include", |context| {
        match execute_native_include(context, request) {
            Ok(NativeDynamicCodeOutcome::Returned(value)) => {
                (php_jit::JitCallStatus::RETURN, Some(value))
            }
            Ok(NativeDynamicCodeOutcome::Exit(value)) => {
                (php_jit::JitCallStatus::EXIT, Some(value))
            }
            Err(NativeIncludeFailure::Execution(control)) => {
                finish_native_dynamic_call_control(context, control)
            }
            Err(NativeIncludeFailure::Resolution(message)) => {
                match render_native_include_failure(context, request, &message) {
                    Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                    Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                }
            }
        }
    })
    .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
}

/// Exact cold include boundary for optimizing code. The include unit is
/// compiled, published, and invoked through its native entry, then control
/// returns to the same optimizing caller frame.
pub(in crate::vm) extern "C" fn jit_native_include_abi(
    runtime: *mut NativeRequestFastState,
    kind: u32,
    caller_function_id: u32,
    continuation_id: u32,
    source: i64,
    caller_frame: u64,
) -> php_jit::JitNativeControlResult {
    debug_assert!(!runtime.is_null());
    let _ = with_native_context_for(runtime, "include", |context| {
        context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
    });
    let request = php_jit::JitNativeDynamicCodeRequest {
        kind: php_jit::JitNativeDynamicCodeKind(kind),
        caller_function_id,
        continuation_id,
        source: php_jit::JitAbiSlot {
            tag: 3,
            flags: 0,
            payload: source as u64,
        },
        caller_frame,
        ..php_jit::JitNativeDynamicCodeRequest::default()
    };
    let (status, value) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        invoke_native_include(runtime, &request)
    }))
    .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None));
    php_jit::JitNativeControlResult::control(status, status.0, value.unwrap_or(0))
}

/// Native dynamic-code compiler boundary. Includes are resolved, compiled to
/// Cranelift entries, published, and invoked without entering an interpreter.
// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_dynamic_code_abi(
    runtime: *mut NativeRequestFastState,
    _vm_context: u64,
    request: *mut php_jit::JitNativeDynamicCodeRequest,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    if request.is_null() || out.is_null() {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let _ = with_native_context_for(runtime, "dynamic_code", |context| {
        context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: Generated code owns this request for the synchronous call.
        let request = unsafe { &*request };
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
            invoke_native_include(runtime, request)
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::EVAL {
            with_native_context_for(runtime, "dynamic_code", |context| match execute_native_eval(context, request) {
                Ok(NativeDynamicCodeOutcome::Returned(value)) => {
                    (php_jit::JitCallStatus::RETURN, Some(value))
                }
                Ok(NativeDynamicCodeOutcome::Exit(value)) => {
                    (php_jit::JitCallStatus::EXIT, Some(value))
                }
                Err(message) => {
                    publish_native_call_diagnostic(context, message);
                    (php_jit::JitCallStatus::RUNTIME_ERROR, None)
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::DECLARE_FUNCTION {
            with_native_context_for(runtime, "dynamic_code", |context| {
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
                match context.encode(Value::Null) {
                    Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                    Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::DECLARE_CLASS {
            with_native_context_for(runtime, "dynamic_code", |context| {
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
                match context.encode(Value::Null) {
                    Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                    Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::REGISTER_CONSTANT {
            with_native_context_for(runtime, "dynamic_code", |context| {
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
                let value = match context.decode(request.source.payload as i64) {
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
                    context.insert_dynamic_constant(name.clone(), value);
                }
                match context.encode(Value::Null) {
                    Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                    Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                }
            })
            .unwrap_or((php_jit::JitCallStatus::RUNTIME_ERROR, None))
        } else if request.kind == php_jit::JitNativeDynamicCodeKind::EMIT_DIAGNOSTIC {
            with_native_context_for(runtime, "dynamic_code", |context| {
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
                    Ok(()) => match context.encode(Value::Null) {
                        Ok(value) => (php_jit::JitCallStatus::RETURN, Some(value)),
                        Err(_) => (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                    },
                    Err(error) if error == "E_PHP_RETHROW" => {
                        let value = context
                            .take_pending_throwable()
                            .and_then(|value| context.encode(value).ok());
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
            with_native_context_for(runtime, "dynamic_code", |context| {
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
                let mut captured_values = Vec::with_capacity(captures.len());
                for capture in captures {
                    let php_ir::Operand::Local(local) = capture.src else {
                        return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                    };
                    if request.caller_frame == 0 {
                        return (php_jit::JitCallStatus::RUNTIME_ERROR, None);
                    }
                    // SAFETY: generated code passes its live caller-local frame for the
                    // duration of this synchronous closure construction request.
                    let encoded =
                        unsafe { *((request.caller_frame as *const i64).add(local.index())) };
                    let value = match context.decode(encoded) {
                        Ok(value) => value,
                        Err(_) => return (php_jit::JitCallStatus::RUNTIME_ERROR, None),
                    };
                    let captured = if capture.by_ref {
                        let reference = match value {
                            Value::Reference(reference) => reference,
                            value => php_runtime::api::ReferenceCell::new(value),
                        };
                        php_runtime::api::ClosureCaptureValue::by_reference(capture.name, reference)
                    } else {
                        let value = match value {
                            Value::Reference(reference) => reference.get(),
                            value => value,
                        };
                        php_runtime::api::ClosureCaptureValue::by_value(capture.name, value)
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
                let mut closure = php_runtime::api::ClosurePayload::new(
                    request.declared_function_id,
                    captured_values,
                )
                .with_debug(debug)
                .with_context(closure_context);
                let binds_this = context
                    .unit
                    .functions
                    .get(request.declared_function_id as usize)
                    .is_some_and(|function| !function.flags.is_static)
                    && context
                        .unit
                        .functions
                        .get(request.caller_function_id as usize)
                        .is_some_and(|function| function.flags.is_method);
                if binds_this && request.caller_frame != 0 {
                    // SAFETY: generated code passes its live caller-local frame for this request.
                    let encoded = unsafe { *(request.caller_frame as *const i64) };
                    let value = context.decode(encoded).map(|value| match value {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    });
                    if let Ok(Value::Object(object)) = value {
                        closure = closure.with_bound_this(Some(object));
                    }
                }
                match context.encode(Value::closure(closure)) {
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
    // SAFETY: `out` is a checked caller-owned result record.
    unsafe {
        out.write(php_jit::JitCallResult {
            status,
            detail: status.0,
            value: value.map_or_else(php_jit::JitAbiSlot::default, |value| php_jit::JitAbiSlot {
                tag: 3,
                flags: 0,
                payload: value as u64,
            }),
        });
    }
    status.0 as i32
}
