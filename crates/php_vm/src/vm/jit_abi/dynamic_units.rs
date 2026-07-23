use super::*;

fn publish_dynamic_unit_entry(
    compiled: &crate::compiled_unit::CompiledUnit,
    function: php_ir::FunctionId,
    handle: &php_jit::JitFunctionHandle,
) {
    let Some(address) = handle.native_entry_address() else {
        return;
    };
    let deployment = compiled.prepared_deployment_image();
    let tier = handle
        .region_state_metadata()
        .map(|metadata| metadata.compiler_tier);
    match tier {
        Some(php_jit::region_ir::NativeCompilerTier::Optimizing) => {
            if let Some(cell) = deployment.optimizing_function_entries.get(function.index()) {
                cell.store(address, std::sync::atomic::Ordering::Release);
            }
        }
        _ => {
            if let Some(cell) = deployment.native_function_entries.get(function.index()) {
                cell.store(address, std::sync::atomic::Ordering::Release);
            }
        }
    }
}

/// Compile-on-demand boundary for a statically known PHP callee.
///
/// The helper resolves code only; generated code performs the native call
/// itself through the uniform packed-argument ABI. This keeps the cold
/// single-flight compile path in Rust while removing the full call dispatcher
/// from every warm invocation.
// SAFETY: audited native ABI pointer boundary; `out` is a synchronous
// caller-owned machine-word slot checked before it is written.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_function_resolve_abi(
    runtime: *mut NativeRequestFastState,
    _vm_context: u64,
    function: u64,
    out: *mut usize,
) -> i32 {
    let Some(out) = std::ptr::NonNull::new(out) else {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    };
    let Ok(function) = u32::try_from(function) else {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    };
    let resolved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_native_context_for(runtime, "function_resolve", |context| {
            let function = php_ir::FunctionId::new(function);
            // This helper is imported exclusively by streaming-baseline
            // artifacts. Keep the tier boundary physical: a baseline call
            // miss may compile and publish only the baseline callee. The
            // optimizing tier has no resolver-helper relocation and reaches
            // publication-validated native cells directly.
            let handle = ensure_native_baseline_entry(context, function)?;
            let address = handle.native_entry_address().ok_or_else(|| {
                format!(
                    "native function entry {} has no executable address",
                    function.raw()
                )
            })?;
            context.publish_native_entry_address(function, address);
            if context.options.native_optimization
                == super::super::NativeOptimizationPolicy::Optimizing
            {
                let compiled = context.compiled.clone();
                let external_signatures =
                    visible_external_function_signatures(context, &compiled, function);
                context.worker_state.schedule_on_demand_optimization(
                    compiled,
                    function,
                    external_signatures,
                );
            }
            Ok(address)
        })
    }));
    match resolved {
        Ok(Some(Ok(address))) if address != 0 => {
            // SAFETY: `out` was validated above and generated code retains the
            // stack slot for the complete synchronous helper call.
            unsafe { out.as_ptr().write(address) };
            0
        }
        Ok(Some(Err(message))) => {
            let _ = with_native_context_for(runtime, "function_resolve", |context| {
                publish_native_call_diagnostic(context, message)
            });
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
        Ok(None) => php_jit::JitCallStatus::COMPILE_REQUIRED.0 as i32,
        Ok(Some(Ok(_))) | Err(_) => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
    }
}

pub(super) fn register_native_dynamic_unit(
    context: &mut NativeRequestColdState<'_>,
    compiled: crate::compiled_unit::CompiledUnit,
    exports: NativeIncludeExports,
) -> Result<usize, String> {
    let entry = compiled.unit().entry;
    compiled
        .unit()
        .functions
        .get(entry.index())
        .ok_or_else(|| "dynamic unit entry function is missing".to_owned())?;
    let NativeIncludeExports {
        functions,
        native_entries,
        native_entry_signature_hashes,
        classes,
        constants,
        autoload_callbacks,
        shutdown_callbacks,
    } = exports;
    let published_function_names = functions
        .iter()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    for (name, _) in &functions {
        if context.function_id(name).is_some() || context.external_function(name).is_some() {
            return Err(format!("Cannot redeclare function {name}()"));
        }
    }
    for class in &classes {
        if context
            .unit
            .classes
            .iter()
            .any(|entry| entry.name == normalize_class_name(class))
            || native_external_class_exists(context, class)
        {
            return Err(format!(
                "Cannot declare class {class}, because the name is already in use"
            ));
        }
    }
    let adds_by_reference_signature = functions.iter().any(|(_, function)| {
        compiled
            .unit()
            .functions
            .get(function.index())
            .is_some_and(|function| function.params.iter().any(|parameter| parameter.by_ref))
    });
    if adds_by_reference_signature {
        context.external_signature_epoch = context.external_signature_epoch.saturating_add(1);
    }
    let native_entry_signature_epochs = native_entries
        .keys()
        .copied()
        .map(|function| (function, context.external_signature_epoch))
        .collect();
    for (function, handle) in native_entries.iter() {
        publish_dynamic_unit_entry(&compiled, *function, handle);
    }
    let unit = context.dynamic_units.len();
    context.dynamic_units.push(NativeDynamicUnit {
        compiled,
        native_entries,
        native_entry_signature_hashes,
        native_entry_signature_epochs,
    });
    for (name, function) in functions {
        context.external_functions.insert(
            name.to_ascii_lowercase(),
            NativeDynamicFunction { unit, function },
        );
    }
    context.publish_function_names(published_function_names);
    for class in classes {
        context.external_class_units.insert(class.clone(), unit);
        context.dynamic_classes.insert(class);
    }
    for (name, value) in constants {
        if context.lookup_constant(&name).is_ok() {
            let declaration = context.dynamic_units.get(unit).and_then(|package| {
                package
                    .compiled
                    .unit()
                    .constant_table
                    .iter()
                    .find(|entry| entry.name == name)
                    .map(|entry| (package, entry))
            });
            let (path, line) = declaration.map_or_else(
                || ("<unknown>".to_owned(), 1),
                |(package, entry)| {
                    let path = package
                        .compiled
                        .unit()
                        .files
                        .get(entry.span.file.index())
                        .map_or("<unknown>", |file| file.path.as_str())
                        .to_owned();
                    let line = package
                        .compiled
                        .source_display_line(entry.span, false)
                        .unwrap_or(1);
                    (path, line)
                },
            );
            context.output.write_bytes(format!(
                "\nWarning: Constant {name} already defined, this will be an error in PHP 9 in {path} on line {line}\n"
            ));
        } else {
            context.insert_dynamic_constant(name, value);
        }
    }
    context
        .autoload_callbacks
        .extend(
            autoload_callbacks
                .into_iter()
                .map(|callback| match callback {
                    Value::Callable(callable) => match callable.as_ref() {
                        php_runtime::api::CallableValue::Closure(closure) => {
                            Value::Callable(Box::new(php_runtime::api::CallableValue::Closure(
                                closure.clone().with_owner_unit(Some(unit)),
                            )))
                        }
                        _ => Value::Callable(callable),
                    },
                    value => value,
                }),
        );
    context
        .shutdown_callbacks
        .extend(shutdown_callbacks.into_iter().map(|mut callback| {
            if let Value::Callable(callable) = callback.callable {
                callback.callable = match callable.as_ref() {
                    php_runtime::api::CallableValue::Closure(closure) => {
                        Value::Callable(Box::new(php_runtime::api::CallableValue::Closure(
                            closure.clone().with_owner_unit(Some(unit)),
                        )))
                    }
                    _ => Value::Callable(callable),
                };
            }
            callback
        }));
    Ok(unit)
}

pub(in crate::vm) fn native_entries_from_records(
    records: &[php_jit::JitUnitCompileRecord],
) -> Result<std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>, String> {
    if let Some(rejected) = records
        .iter()
        .find(|record| !matches!(record.result.status, php_jit::JitCompileStatus::Compiled))
    {
        let detail = rejected
            .result
            .diagnostics
            .first()
            .map_or("native compiler returned no diagnostic", String::as_str);
        return Err(format!(
            "dynamic native compilation rejected function {}: {detail}",
            rejected.function.raw()
        ));
    }
    Ok(records
        .iter()
        .filter_map(|record| {
            record
                .result
                .handle
                .as_ref()
                .cloned()
                .map(|handle| (record.function, handle))
        })
        .collect())
}

pub(super) fn ensure_native_entry(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
) -> Result<php_jit::JitFunctionHandle, String> {
    let external_signatures =
        visible_external_function_signatures(context, &context.compiled, function);

    if context.options.native_optimization == super::super::NativeOptimizationPolicy::Optimizing
        && context.options.tiering.enabled
        && !context.options.tiering.native_eager
    {
        if let Some(handle) = context.worker_state.resolved_native_function(
            &context.compiled,
            function,
            context.options,
            &external_signatures,
        ) {
            std::sync::Arc::make_mut(&mut context.native_entries).insert(function, handle.clone());
            return Ok(handle);
        }

        let handle = if let Some(handle) = context.native_entries.get(&function) {
            handle.clone()
        } else {
            ensure_native_baseline_entry(context, function)?
        };
        context.worker_state.schedule_on_demand_optimization(
            context.compiled.clone(),
            function,
            external_signatures,
        );
        std::sync::Arc::make_mut(&mut context.native_entries).insert(function, handle.clone());
        return Ok(handle);
    }

    if let Some(handle) = context.native_entries.get(&function) {
        return Ok(handle.clone());
    }
    let handle = context.worker_state.resolve_native_function(
        &context.compiled,
        function,
        context.options,
        &external_signatures,
    )?;
    std::sync::Arc::make_mut(&mut context.native_entries).insert(function, handle);
    context
        .native_entries
        .get(&function)
        .cloned()
        .ok_or_else(|| format!("native function entry {} was not published", function.raw()))
}

pub(super) fn ensure_native_baseline_entry(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
) -> Result<php_jit::JitFunctionHandle, String> {
    let external_signatures =
        visible_external_function_signatures(context, &context.compiled, function);
    let mut options = context.options.clone();
    options.native_optimization = super::super::NativeOptimizationPolicy::Baseline;
    options.tiering.enabled = false;
    context.worker_state.resolve_native_function(
        &context.compiled,
        function,
        &options,
        &external_signatures,
    )
}

/// Ensure that a dynamic-unit entry is current without cloning its owning
/// code handle. Cross-unit dispatch immediately swaps the unit's publication
/// map into the active context, where the actual invocation acquires its one
/// required handle. Returning a clone here as well made every warm external
/// call perform two generation-owner reference-count operations.
pub(super) fn prepare_dynamic_native_entry(
    context: &mut NativeRequestColdState<'_>,
    unit: usize,
    function: php_ir::FunctionId,
) -> Result<(), String> {
    let signature_epoch = context.external_signature_epoch;
    let package = context
        .dynamic_units
        .get(unit)
        .ok_or_else(|| "dynamic native unit is missing".to_owned())?;
    let wants_optimizing =
        context.options.native_optimization == super::super::NativeOptimizationPolicy::Optimizing;
    let prepared_tier_matches = |handle: &php_jit::JitFunctionHandle| {
        handle.region_state_metadata().is_some_and(|metadata| {
            metadata.compiler_tier
                == if wants_optimizing {
                    php_jit::region_ir::NativeCompilerTier::Optimizing
                } else {
                    php_jit::region_ir::NativeCompilerTier::Baseline
                }
        })
    };
    if package.native_entry_signature_epochs.get(&function) == Some(&signature_epoch)
        && package
            .native_entries
            .get(&function)
            .is_some_and(prepared_tier_matches)
    {
        return Ok(());
    }
    let compiled = package.compiled.clone();
    let external_signatures = visible_external_function_signatures(context, &compiled, function);
    let signature_hash = super::super::external_function_signatures_hash(&external_signatures);
    if package.native_entry_signature_hashes.get(&function) == Some(&signature_hash)
        && package
            .native_entries
            .get(&function)
            .is_some_and(prepared_tier_matches)
    {
        context
            .dynamic_units
            .get_mut(unit)
            .expect("dynamic native unit was already validated")
            .native_entry_signature_epochs
            .insert(function, signature_epoch);
        return Ok(());
    }
    let mut baseline_options = context.options.clone();
    baseline_options.native_optimization = super::super::NativeOptimizationPolicy::Baseline;
    baseline_options.tiering.enabled = false;
    let handle = context.worker_state.resolve_native_function(
        &compiled,
        function,
        &baseline_options,
        &external_signatures,
    )?;
    publish_dynamic_unit_entry(&compiled, function, &handle);
    let preferred = if wants_optimizing {
        let optimizing = context.worker_state.resolve_native_function(
            &compiled,
            function,
            context.options,
            &external_signatures,
        )?;
        publish_dynamic_unit_entry(&compiled, function, &optimizing);
        optimizing
    } else {
        handle
    };
    let package = context
        .dynamic_units
        .get_mut(unit)
        .ok_or_else(|| "dynamic native unit disappeared during compilation".to_owned())?;
    package
        .native_entry_signature_hashes
        .insert(function, signature_hash);
    package
        .native_entry_signature_epochs
        .insert(function, signature_epoch);
    std::sync::Arc::make_mut(&mut package.native_entries).insert(function, preferred);
    Ok(())
}

pub(super) fn visible_external_function_signatures(
    context: &NativeRequestColdState<'_>,
    compiled: &crate::compiled_unit::CompiledUnit,
    root: php_ir::FunctionId,
) -> Vec<php_jit::JitExternalFunctionSignature> {
    collect_visible_external_function_signatures(
        context,
        compiled.prepared_external_function_calls(root),
    )
}

pub(super) fn visible_external_function_signatures_for_unit(
    context: &NativeRequestColdState<'_>,
    compiled: &crate::compiled_unit::CompiledUnit,
) -> Vec<php_jit::JitExternalFunctionSignature> {
    collect_visible_external_function_signatures(
        context,
        compiled.prepared_unit_external_function_calls(),
    )
}

pub(super) fn collect_visible_external_function_signatures(
    context: &NativeRequestColdState<'_>,
    calls: &[crate::compiled_unit::PreparedExternalFunctionCall],
) -> Vec<php_jit::JitExternalFunctionSignature> {
    calls
        .iter()
        .filter_map(|call| {
            let target = context
                .external_functions
                .get(call.normalized_name.as_ref())?;
            let function = context
                .dynamic_units
                .get(target.unit)?
                .compiled
                .unit()
                .functions
                .get(target.function.index())?;
            Some(php_jit::JitExternalFunctionSignature {
                // Match the source unit's call target. The lowering lookup is
                // intentionally independent of the publishing unit's spelling.
                name: call.source_name.to_string(),
                params: function
                    .params
                    .iter()
                    .map(|parameter| php_jit::JitExternalParameterSignature {
                        name: parameter.name.clone(),
                        by_ref: parameter.by_ref,
                        variadic: parameter.variadic,
                    })
                    .collect(),
            })
        })
        .collect()
}

pub(super) fn native_include_uses_implicit_return(unit: &php_ir::IrUnit) -> bool {
    let Some(function) = unit.functions.get(unit.entry.index()) else {
        return false;
    };
    function.blocks.iter().any(|block| {
        block.terminator.as_ref().is_some_and(|terminator| {
            terminator.span == function.span
                && matches!(
                    terminator.kind,
                    php_ir::instruction::TerminatorKind::Return {
                        value: Some(php_ir::Operand::Constant(constant)),
                        ..
                    } if unit.constants.get(constant.index()).is_some_and(|value| matches!(value, php_ir::IrConstant::Null))
                )
        })
    })
}

pub(super) fn native_external_class_handle(
    context: &NativeRequestColdState<'_>,
    name: &str,
) -> Option<(usize, crate::compiled_unit::CompiledClass)> {
    let (unit, class_entry) = native_external_class_ref(context, name)?;
    let package = &context.dynamic_units[unit];
    let class = package
        .compiled
        .lookup_unit_class_handle(&class_entry.name)?;
    Some((unit, class))
}

pub(super) fn native_external_class_ref<'a>(
    context: &'a NativeRequestColdState<'_>,
    name: &str,
) -> Option<(usize, &'a php_ir::module::ClassEntry)> {
    let requested = normalized_class_name(name);
    let normalized = context
        .class_aliases
        .get(requested.as_ref())
        .map_or(requested.as_ref(), String::as_str);
    let unit = context
        .external_class_units
        .get(normalized)
        .copied()
        .or_else(|| context.deployment_classes.contains(normalized).then_some(0))?;
    if context.current_dynamic_unit == Some(unit) {
        return None;
    }
    let package = context.dynamic_units.get(unit)?;
    package
        .compiled
        .lookup_unit_class(normalized)
        .map(|class| (unit, class))
}

pub(super) fn native_external_class_exists(
    context: &NativeRequestColdState<'_>,
    name: &str,
) -> bool {
    native_external_class_ref(context, name).is_some()
}

pub(super) fn native_autoload_class(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    source: &php_ir::Instruction,
) -> Result<(), String> {
    let normalized = normalize_class_name(name);
    if context
        .unit
        .classes
        .iter()
        .any(|class| class.name == normalized)
        || php_std::ExtensionRegistry::standard_library()
            .enabled_class(&normalized)
            .is_some()
        || matches!(
            normalized.as_str(),
            "exception"
                | "error"
                | "typeerror"
                | "valueerror"
                | "argumentcounterror"
                | "fibererror"
        )
    {
        return Ok(());
    }
    if !context.autoload_in_progress.insert(normalized.clone()) {
        return Ok(());
    }
    let result = (|| {
        if !native_external_class_exists(context, name) {
            let callbacks = context.autoload_callbacks.clone();
            for callback in callbacks {
                invoke_native_callable_value(
                    context,
                    callback,
                    &[Value::String(PhpString::from_bytes(
                        name.as_bytes().to_vec(),
                    ))],
                    source,
                    None,
                )?;
                if native_external_class_exists(context, name) {
                    break;
                }
            }
        }
        if let Some((_, class)) = native_external_class_handle(context, name) {
            let dependencies = class
                .parent_display_name
                .clone()
                .or_else(|| class.parent.clone())
                .into_iter()
                .chain(class.interfaces.iter().cloned());
            for dependency in dependencies {
                native_autoload_class(context, &dependency, source)?;
            }
        }
        Ok(())
    })();
    context.autoload_in_progress.remove(&normalized);
    result
}
