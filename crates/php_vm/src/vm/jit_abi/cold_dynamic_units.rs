//! Cold compilation, publication, and compatibility resolution for dynamic
//! include/eval units.
//!
//! Optimizing artifacts consume only the immutable linked entries and runtime
//! views published here; they cannot import this module's resolver ABI.

use super::*;
use php_runtime::api::Value;

fn dynamic_unit_local_is_superglobal(name: &str) -> bool {
    matches!(
        name,
        "_GET" | "_POST" | "_COOKIE" | "_REQUEST" | "_SERVER" | "_ENV" | "_FILES" | "_SESSION"
    )
}

pub(super) fn dynamic_unit_cross_unit_global_names(
    compiled: &crate::compiled_unit::CompiledUnit,
    published_functions: impl IntoIterator<Item = php_ir::FunctionId>,
) -> std::sync::Arc<[String]> {
    let unit = compiled.unit();
    let mut names = std::collections::BTreeSet::new();
    for function in &unit.functions {
        if function.flags.is_top_level {
            names.extend(
                function
                    .locals
                    .iter()
                    .filter(|name| {
                        name.as_str() != "GLOBALS"
                            && !php_ir::is_compiler_generated_local_name(name)
                    })
                    .cloned(),
            );
        } else {
            names.extend(
                function
                    .locals
                    .iter()
                    .filter(|name| dynamic_unit_local_is_superglobal(name))
                    .cloned(),
            );
        }
        names.extend(
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .filter_map(|instruction| match &instruction.kind {
                    php_ir::InstructionKind::BindGlobal { name, .. } => Some(name.clone()),
                    _ => None,
                }),
        );
    }
    let mut published_functions = published_functions
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    published_functions.extend(
        compiled
            .prepared_deployment_image()
            .preferred_function_entries
            .iter()
            .enumerate()
            .filter_map(|(function, entry)| {
                (entry.load(std::sync::atomic::Ordering::Acquire) != 0)
                    .then(|| u32::try_from(function).ok())
                    .flatten()
                    .map(php_ir::FunctionId::new)
            }),
    );
    for function in published_functions {
        let Some(sites) = compiled.prepared_native_global_sites(function) else {
            continue;
        };
        names.extend(
            sites
                .iter()
                .filter_map(|name| name.as_deref().map(str::to_owned)),
        );
    }
    names.into_iter().collect::<Vec<_>>().into()
}

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
            if let Some(cell) = deployment.preferred_function_entries.get(function.index()) {
                compiled.publish_preferred_function_metadata(function, handle);
                cell.store(address, std::sync::atomic::Ordering::Release);
            }
        }
        _ => {
            if let Some(cell) = deployment.native_function_entries.get(function.index()) {
                cell.store(address, std::sync::atomic::Ordering::Release);
            }
            if let Some(cell) = deployment.preferred_function_entries.get(function.index()) {
                if cell
                    .compare_exchange(
                        0,
                        address,
                        std::sync::atomic::Ordering::AcqRel,
                        std::sync::atomic::Ordering::Acquire,
                    )
                    .is_ok()
                {
                    compiled.publish_preferred_function_metadata(function, handle);
                }
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
#[allow(unsafe_code)] // Safety: the active cold request owns the raw VM state for this synchronous continuation.
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
        with_baseline_native_context_for(runtime, "function_resolve", |context| {
            let function = php_ir::FunctionId::new(function);
            // This helper is imported exclusively by streaming-baseline
            // artifacts. Keep the tier boundary physical: the current call
            // always enters the baseline callee. Background workers select
            // optimizing products later from direct baseline-entry counts;
            // merely reaching a declaration is no longer sufficient.
            let handle = ensure_native_baseline_entry(context, function)?;
            let address = handle.native_entry_address().ok_or_else(|| {
                format!(
                    "native function entry {} has no executable address",
                    function.raw()
                )
            })?;
            context.publish_native_entry_address(function, address);
            let mut preferred = handle.clone();
            if context.options.native_optimization
                == super::super::NativeOptimizationPolicy::Optimizing
                && context.options.tiering.enabled
                && !context
                    .worker_state
                    .defers_optimizing_compilation(context.options)
            {
                let compiled = context.compiled.clone();
                let external_signatures =
                    visible_external_function_signatures(context, &compiled, function);
                preferred = context.worker_state.resolve_native_function(
                    &compiled,
                    function,
                    context.options,
                    &external_signatures,
                )?;
            }
            // The current streaming-baseline call enters `address`, while
            // later optimizing callers use `preferred`. Both artifacts consume
            // the same function-scoped native metadata, so publish the reached
            // function before returning either executable address. Previously
            // this resolver bypassed the active-entry installation boundary;
            // demand-zero request-local slots therefore remained empty and
            // generated code interpreted encoded zero as a direct-slot handle.
            install_active_native_entry(context, function, preferred)?;
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
            let _ = with_baseline_native_context_for(runtime, "function_resolve", |context| {
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
    let published_class_names = classes
        .iter()
        .map(|name| normalized_class_name(name).into_owned())
        .collect::<Vec<_>>();
    for (name, _) in &functions {
        if context.function_id(name).is_some() || context.external_function(name).is_some() {
            return Err(format!("Cannot redeclare function {name}()"));
        }
    }
    for class in &classes {
        let normalized = normalized_class_name(class);
        if context
            .unit
            .classes
            .iter()
            .any(|entry| entry.name == normalized)
            || native_external_class_exists(context, class)
        {
            let display_name = compiled
                .unit()
                .classes
                .iter()
                .find(|entry| entry.name == normalized)
                .map_or(class.as_str(), |entry| entry.display_name.as_str());
            return Err(format!(
                "Cannot declare class {display_name}, because the name is already in use"
            ));
        }
    }
    if !functions.is_empty() || !classes.is_empty() {
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
    let cross_unit_global_names =
        dynamic_unit_cross_unit_global_names(&compiled, native_entries.keys().copied());
    let runtime_state = NativeUnitRuntimeState::for_compiled(&compiled);
    let linked_functions = vec![
        php_jit::JitNativeLinkedFunction::default();
        compiled.prepared_linked_function_count()
    ]
    .into_boxed_slice();
    let unit = context.dynamic_units.len();
    context.dynamic_units.push(NativeDynamicUnit {
        compiled,
        cross_unit_global_names,
        native_entries,
        native_entry_signature_hashes,
        native_entry_signature_epochs,
        runtime_state,
        linked_functions,
        published_runtime_view: Box::default(),
    });
    for (name, function) in &functions {
        context.external_functions.insert(
            name.to_ascii_lowercase(),
            NativeDynamicFunction {
                unit,
                function: *function,
            },
        );
    }
    // Class ownership must be visible before declaration-time link
    // preparation. Otherwise an already compiled caller can only discover a
    // newly included method on its first invocation, which would put dynamic
    // method dispatch back into the optimizing hot path.
    for class in &classes {
        context.external_class_units.insert(class.clone(), unit);
        context.dynamic_classes.insert(class.clone());
    }
    // Publish the inactive unit's stable runtime view once at declaration
    // time. Late-linked optimizing callers can then enter its already
    // published native frame on the first PHP invocation; they never need an
    // operation-local resolver transition merely to initialize table
    // addresses.
    context.with_active_dynamic_unit(unit, None, |_| ())?;
    prepare_linked_function_entries(context)?;
    prepare_resolved_external_callers(context, &published_function_names, &published_class_names)?;
    refresh_linked_function_records(context);
    context.publish_function_names(published_function_names);
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
            context.insert_dynamic_constant(name, value)?;
        }
    }
    let autoload_callbacks = autoload_callbacks
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
        })
        .collect();
    let shutdown_callbacks = shutdown_callbacks
        .into_iter()
        .map(|mut callback| {
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
        })
        .collect();
    context.append_registered_include_exports(autoload_callbacks, shutdown_callbacks)?;
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

fn install_active_native_entry(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    handle: php_jit::JitFunctionHandle,
) -> Result<php_jit::JitFunctionHandle, String> {
    std::sync::Arc::make_mut(&mut context.native_entries).insert(function, handle.clone());
    if let Some(unit) = context.current_dynamic_unit {
        let names = dynamic_unit_cross_unit_global_names(
            &context.compiled,
            context.native_entries.keys().copied(),
        );
        if let Some(package) = context.dynamic_units.get_mut(unit) {
            package.cross_unit_global_names = names;
        }
    }
    context.prepare_published_native_metadata()?;
    let _runtime_view = activate_native_context(context);
    Ok(handle)
}

pub(super) fn ensure_native_entry(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
) -> Result<php_jit::JitFunctionHandle, String> {
    let external_signatures =
        visible_external_function_signatures(context, &context.compiled, function);

    if context.options.native_optimization == super::super::NativeOptimizationPolicy::Optimizing
        && !context.options.tiering.enabled
    {
        let handle = if let Some(handle) = context.native_entries.get(&function) {
            handle.clone()
        } else {
            ensure_native_baseline_entry(context, function)?
        };
        return install_active_native_entry(context, function, handle);
    }

    if context
        .worker_state
        .defers_optimizing_compilation(context.options)
    {
        if let Some(handle) = context.worker_state.resolved_native_function(
            &context.compiled,
            function,
            context.options,
            &external_signatures,
        ) {
            return install_active_native_entry(context, function, handle);
        }

        let handle = if let Some(handle) = context.native_entries.get(&function) {
            handle.clone()
        } else {
            ensure_native_baseline_entry(context, function)?
        };
        return install_active_native_entry(context, function, handle);
    }

    if let Some(handle) = context.native_entries.get(&function)
        && (context.options.native_optimization
            != super::super::NativeOptimizationPolicy::Optimizing
            || !context.options.tiering.enabled
            || handle.region_state_metadata().is_some_and(|metadata| {
                metadata.compiler_tier == php_jit::region_ir::NativeCompilerTier::Optimizing
            }))
    {
        return Ok(handle.clone());
    }
    let handle = context.worker_state.resolve_native_function(
        &context.compiled,
        function,
        context.options,
        &external_signatures,
    )?;
    install_active_native_entry(context, function, handle)
}

pub(super) fn ensure_native_baseline_entry(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
) -> Result<php_jit::JitFunctionHandle, String> {
    let external_signatures =
        visible_external_function_signatures(context, &context.compiled, function);
    let prepare_options = context.options.clone();
    context.worker_state.prepare_native_baseline_entry(
        &context.compiled,
        function,
        &prepare_options,
        &external_signatures,
    )?;
    let mut resolved_options = prepare_options;
    resolved_options.native_optimization = match resolved_options.native_optimization {
        super::super::NativeOptimizationPolicy::Optimizing
            if resolved_options.tiering.enabled && !resolved_options.tiering.native_eager =>
        {
            super::super::NativeOptimizationPolicy::TieredBaseline
        }
        super::super::NativeOptimizationPolicy::Optimizing
        | super::super::NativeOptimizationPolicy::Baseline => {
            super::super::NativeOptimizationPolicy::Baseline
        }
        super::super::NativeOptimizationPolicy::TieredBaseline => {
            super::super::NativeOptimizationPolicy::TieredBaseline
        }
    };
    resolved_options.tiering.enabled = false;
    if let Some(handle) = context.worker_state.resolved_native_function(
        &context.compiled,
        function,
        &resolved_options,
        &external_signatures,
    ) {
        return Ok(handle);
    }
    // A deployment publication cell proves that executable baseline code is
    // live, but not that this worker already owns the handle under the active
    // unit/signature key. Cross-unit entry tables can be attached after the
    // original publication. Resolve that exact generation normally so the
    // continuation coordinator never reconstructs ownership from an address.
    context.worker_state.resolve_native_function(
        &context.compiled,
        function,
        &resolved_options,
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
    let active_unit = context.current_dynamic_unit == Some(unit);
    let package = context
        .dynamic_units
        .get(unit)
        .ok_or_else(|| "dynamic native unit is missing".to_owned())?;
    let wants_optimizing = context.options.native_optimization
        == super::super::NativeOptimizationPolicy::Optimizing
        && context.options.tiering.enabled;
    let has_native_entry = if active_unit {
        context.native_entries.contains_key(&function)
    } else {
        package.native_entries.contains_key(&function)
    };
    if package.native_entry_signature_epochs.get(&function) == Some(&signature_epoch)
        && has_native_entry
    {
        return Ok(());
    }
    let compiled = package.compiled.clone();
    let external_signatures = visible_external_function_signatures(context, &compiled, function);
    let signature_hash = super::super::external_function_signatures_hash(&external_signatures);
    if package.native_entry_signature_hashes.get(&function) == Some(&signature_hash)
        && has_native_entry
    {
        context
            .dynamic_units
            .get_mut(unit)
            .ok_or_else(|| "dynamic native unit disappeared during publication".to_owned())?
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
    // A changed external signature invalidates the preferred target as well
    // as the baseline entry. Publish the freshly validated baseline into both
    // cells before an optional optimizing replacement becomes visible.
    if let Some(address) = handle.native_entry_address()
        && let Some(preferred) = compiled
            .prepared_deployment_image()
            .preferred_function_entries
            .get(function.index())
    {
        compiled.publish_preferred_function_metadata(function, &handle);
        preferred.store(address, std::sync::atomic::Ordering::Release);
    }
    let deferred_optimization = context
        .worker_state
        .defers_optimizing_compilation(context.options);
    let preferred = if deferred_optimization {
        // Dynamic callback resolution is a publication boundary, not an
        // instruction to synchronously compile a second tier. Reuse an
        // already completed or restart-persisted optimizing product when
        // present; otherwise the ensuing baseline invocation records the
        // entry and schedules the normal background upgrade. A persistent
        // miss is load-only here and never compiles in the request.
        context
            .worker_state
            .resolved_native_function(&compiled, function, context.options, &external_signatures)
            .or_else(|| {
                context
                    .worker_state
                    .load_cached_native_function(
                        &compiled,
                        function,
                        context.options,
                        &external_signatures,
                    )
                    .ok()
                    .flatten()
            })
            .inspect(|optimizing| publish_dynamic_unit_entry(&compiled, function, optimizing))
            .unwrap_or_else(|| handle.clone())
    } else if wants_optimizing {
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
    if active_unit {
        std::sync::Arc::make_mut(&mut context.native_entries).insert(function, preferred);
    } else {
        std::sync::Arc::make_mut(&mut package.native_entries).insert(function, preferred);
    }
    let published_functions = if active_unit {
        context.native_entries.keys().copied().collect::<Vec<_>>()
    } else {
        context
            .dynamic_units
            .get(unit)
            .into_iter()
            .flat_map(|package| package.native_entries.keys().copied())
            .collect()
    };
    let cross_unit_global_names =
        dynamic_unit_cross_unit_global_names(&compiled, published_functions);
    if let Some(package) = context.dynamic_units.get_mut(unit) {
        package.cross_unit_global_names = cross_unit_global_names;
    }
    if active_unit {
        context.prepare_published_native_metadata()?;
        let _runtime_view = activate_native_context(context);
    } else {
        context.with_active_dynamic_unit(unit, None, |_| ())?;
    }
    // Only this published caller can expose its outgoing immutable links.
    // Recursing through caller-scoped publication computes the newly
    // reachable graph without restarting a whole-request scan per target.
    prepare_linked_function_entries_for_caller(context, unit, function)?;
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

/// At the request boundary, select only baseline functions whose direct
/// process-owned entry counters reached the worker threshold. Generated warm
/// calls remain a single preferred-cell load; no runtime tiering helper or
/// branch is introduced.
pub(super) fn schedule_hot_native_functions(context: &NativeRequestColdState<'_>) {
    if !context.worker_state.background_tiering
        || !context.worker_state.tiering_options.enabled
        || context.worker_state.tiering_options.native_eager
    {
        return;
    }
    let threshold = context
        .worker_state
        .tiering_options
        .function_entry_threshold
        .max(1);
    let mut candidates = Vec::new();
    for package in &context.dynamic_units {
        let deployment = package.compiled.prepared_deployment_image();
        for (index, ((baseline, preferred), entries)) in deployment
            .native_function_entries
            .iter()
            .zip(deployment.preferred_function_entries.iter())
            .zip(deployment.baseline_function_entry_counts.iter())
            .enumerate()
        {
            let baseline = baseline.load(std::sync::atomic::Ordering::Acquire);
            if baseline == 0 || preferred.load(std::sync::atomic::Ordering::Acquire) != baseline {
                continue;
            }
            let entries = entries.load(std::sync::atomic::Ordering::Relaxed);
            if entries < threshold {
                continue;
            }
            let Ok(function) = u32::try_from(index).map(php_ir::FunctionId::new) else {
                continue;
            };
            candidates.push((entries, package.compiled.clone(), function));
        }
    }
    candidates.sort_unstable_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.2.raw().cmp(&right.2.raw()))
    });
    candidates.truncate(super::super::NATIVE_OPTIMIZATION_BATCH_CAPACITY);
    let candidates = candidates
        .into_iter()
        .map(|(entries, compiled, function)| {
            let external_signatures =
                visible_external_function_signatures(context, &compiled, function);
            (compiled, function, external_signatures, entries)
        })
        .collect();
    context
        .worker_state
        .schedule_hot_on_demand_optimizations(context.options, candidates);
}

#[derive(Clone)]
enum PreparedExternalCallTarget {
    Function(NativeDynamicFunction),
    Method {
        target: NativeDynamicFunction,
        entry: php_ir::module::ClassMethodEntry,
    },
    Class {
        unit: usize,
    },
}

impl PreparedExternalCallTarget {
    fn function(&self) -> Option<NativeDynamicFunction> {
        match self {
            Self::Function(target) | Self::Method { target, .. } => Some(*target),
            Self::Class { .. } => None,
        }
    }

    fn unit(&self) -> usize {
        match self {
            Self::Function(target) | Self::Method { target, .. } => target.unit,
            Self::Class { unit } => *unit,
        }
    }
}

fn prepared_external_class_unit(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
) -> Option<usize> {
    let requested = normalized_class_name(class_name);
    let class_name = context
        .class_aliases
        .get(requested.as_ref())
        .map_or(requested.as_ref(), String::as_str);
    context
        .external_class_units
        .get(class_name)
        .copied()
        .or_else(|| context.deployment_classes.contains(class_name).then_some(0))
}

/// Resolves an immutable external method identity without consulting the
/// currently active unit.
///
/// Declaration-time publication deliberately activates the unit being
/// published. The ordinary runtime lookup excludes that unit to distinguish
/// local from external calls, but a caller link prepared at this boundary
/// must still be able to point at the newly active target.
fn prepared_external_method_target(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    method: &str,
) -> Option<(NativeDynamicFunction, php_ir::module::ClassMethodEntry)> {
    let requested = normalized_class_name(class_name);
    let mut class_name = context
        .class_aliases
        .get(requested.as_ref())
        .cloned()
        .unwrap_or_else(|| requested.into_owned());
    let mut unit = context
        .external_class_units
        .get(&class_name)
        .copied()
        .or_else(|| {
            context
                .deployment_classes
                .contains(class_name.as_str())
                .then_some(0)
        })?;
    let mut visited = std::collections::BTreeSet::new();
    loop {
        if !visited.insert((unit, class_name.clone())) {
            return None;
        }
        let package = context.dynamic_units.get(unit)?;
        let class = package.compiled.lookup_unit_class_handle(&class_name)?;
        if let Some(entry) = class
            .methods
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(method))
            .cloned()
        {
            return Some((
                NativeDynamicFunction {
                    unit,
                    function: entry.function,
                },
                entry,
            ));
        }
        class_name = normalized_class_name(class.parent.as_deref()?).into_owned();
        if package
            .compiled
            .lookup_unit_class_handle(&class_name)
            .is_none()
        {
            unit = context
                .external_class_units
                .get(&class_name)
                .copied()
                .or_else(|| {
                    context
                        .deployment_classes
                        .contains(class_name.as_str())
                        .then_some(0)
                })?;
        }
    }
}

fn prepared_external_class_plan(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
) -> Option<u64> {
    let requested = normalized_class_name(class_name);
    let class_name = context
        .class_aliases
        .get(requested.as_ref())
        .map_or(requested.as_ref(), String::as_str);
    let unit = prepared_external_class_unit(context, class_name)?;
    let package = context.dynamic_units.get(unit)?;
    let class_index = package
        .compiled
        .unit()
        .classes
        .iter()
        .position(|class| normalized_class_name(&class.name).as_ref() == class_name)?;
    let view = package.published_runtime_view.as_ref();
    if view.abi_version != php_jit::JIT_RUNTIME_ABI_VERSION
        || class_index >= view.trusted_class_plan_count as usize
        || view.trusted_class_plans == 0
    {
        return None;
    }
    // SAFETY: the published runtime view owns a stable class-plan array for
    // the complete request activation and the checked dense index belongs to
    // the target unit.
    // Safety: the active cold request owns the raw VM state for this synchronous continuation.
    #[allow(unsafe_code)]
    let plan = unsafe {
        *(view.trusted_class_plans as usize as *const php_jit::JitNativePreparedClassPlan)
            .add(class_index)
    };
    (plan.state == php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE && plan.prepared != 0)
        .then_some(plan.prepared)
}

fn prepared_external_call_target(
    context: &NativeRequestColdState<'_>,
    call: &crate::compiled_unit::PreparedExternalFunctionCall,
) -> Option<PreparedExternalCallTarget> {
    if let Some((class, method)) = call.source_name.rsplit_once("::") {
        if let Some((target, entry)) = prepared_external_method_target(context, class, method) {
            return Some(PreparedExternalCallTarget::Method { target, entry });
        }
        if method.eq_ignore_ascii_case("__construct") {
            return prepared_external_class_unit(context, class)
                .map(|unit| PreparedExternalCallTarget::Class { unit });
        }
        return None;
    }
    context
        .external_functions
        .get(call.normalized_name.as_ref())
        .copied()
        .or_else(|| {
            context
                .deployment_functions
                .get(call.normalized_name.as_ref())
                .copied()
                .map(|function| NativeDynamicFunction { unit: 0, function })
        })
        .map(PreparedExternalCallTarget::Function)
}

pub(super) fn collect_visible_external_function_signatures(
    context: &NativeRequestColdState<'_>,
    calls: &[crate::compiled_unit::PreparedExternalFunctionCall],
) -> Vec<php_jit::JitExternalFunctionSignature> {
    calls
        .iter()
        .map(|call| {
            let target = prepared_external_call_target(context, call);
            let function = target.as_ref().and_then(|target| {
                let target = target.function()?;
                context
                    .dynamic_units
                    .get(target.unit)?
                    .compiled
                    .unit()
                    .functions
                    .get(target.function.index())
            });
            let method_entry = match target.as_ref() {
                Some(PreparedExternalCallTarget::Method { entry, .. }) => Some(entry),
                Some(
                    PreparedExternalCallTarget::Function(_)
                    | PreparedExternalCallTarget::Class { .. },
                )
                | None => None,
            };
            let constructor_plan_ready = call
                .source_name
                .rsplit_once("::")
                .filter(|(_, method)| method.eq_ignore_ascii_case("__construct"))
                .is_none_or(|(class, _)| prepared_external_class_plan(context, class).is_some());
            let class_only = matches!(target, Some(PreparedExternalCallTarget::Class { .. }));
            let published = (function.is_some() || class_only)
                && method_entry
                    .is_none_or(|entry| !entry.flags.is_private && !entry.flags.is_protected)
                && constructor_plan_ready;
            if class_only && published {
                return php_jit::JitExternalFunctionSignature {
                    name: call.source_name.to_string(),
                    link_index: call.link_index,
                    published: true,
                    params: Vec::new(),
                    native_params: Vec::new(),
                    native_default_constant_indices: Vec::new(),
                    // Zero distinguishes an allocation-only external class
                    // from a constructor frame, whose hidden receiver makes
                    // the native arity at least one.
                    native_arity: 0,
                    requires_non_reference_trampoline: false,
                    returns_by_reference: false,
                    exception_routes: None,
                };
            }
            let Some(function) = function.filter(|_| published) else {
                return php_jit::JitExternalFunctionSignature {
                    name: call.source_name.to_string(),
                    link_index: call.link_index,
                    published: false,
                    params: Vec::new(),
                    native_params: Vec::new(),
                    native_default_constant_indices: Vec::new(),
                    native_arity: 0,
                    requires_non_reference_trampoline: false,
                    returns_by_reference: false,
                    exception_routes: None,
                };
            };
            php_jit::JitExternalFunctionSignature {
                // Match the source unit's call target. The lowering lookup is
                // intentionally independent of the publishing unit's spelling.
                name: call.source_name.to_string(),
                link_index: call.link_index,
                published: true,
                params: function
                    .params
                    .iter()
                    .map(|parameter| php_jit::JitExternalParameterSignature {
                        name: parameter.name.clone(),
                        by_ref: parameter.by_ref,
                        variadic: parameter.variadic,
                    })
                    .collect(),
                native_params: function.params.clone(),
                native_default_constant_indices: {
                    let constants = target
                        .as_ref()
                        .and_then(PreparedExternalCallTarget::function)
                        .and_then(|target| context.dynamic_units.get(target.unit))
                        .map(|package| package.compiled.unit().constants.as_slice());

                    function
                        .params
                        .iter()
                        .map(|parameter| {
                            let default = parameter.default.as_ref()?;
                            constants?
                                .iter()
                                .position(|constant| constant == default)
                                .and_then(|index| u32::try_from(index).ok())
                        })
                        .collect::<Vec<_>>()
                },
                native_arity: u32::try_from(
                    function.params.len()
                        + usize::from(method_entry.is_some_and(|entry| !entry.flags.is_static)),
                )
                .unwrap_or(u32::MAX),
                requires_non_reference_trampoline:
                    native_function_requires_non_reference_trampoline(
                        function,
                        method_entry.is_some(),
                    ),
                returns_by_reference: function.returns_by_ref,
                exception_routes: native_function_exception_routes(
                    target
                        .as_ref()
                        .and_then(PreparedExternalCallTarget::function)
                        .expect("published function signature has an exact target")
                        .function,
                    function,
                ),
            }
        })
        .collect()
}

/// Publishes code for every immutable cross-unit link that became resolvable
/// at a declaration boundary.
///
/// Compilation and signature validation belong here, before generated code
/// can observe the link. The compiled call itself then loads an already
/// published preferred entry and never enters the operation-local resolver
/// transition merely because this is the target's first invocation.
fn prepare_linked_function_entries(context: &mut NativeRequestColdState<'_>) -> Result<(), String> {
    let mut callers = Vec::new();
    for (caller_unit, caller) in context.dynamic_units.iter().enumerate() {
        let mut published_callers = if context.current_dynamic_unit == Some(caller_unit) {
            context.native_entries.keys().copied().collect::<Vec<_>>()
        } else {
            caller.native_entries.keys().copied().collect::<Vec<_>>()
        };
        // Background tiering publishes directly into the process-owned entry
        // cells after the originating request has released its request-local
        // handle map. A later request must still prepare every outgoing link
        // of that published caller before the preferred entry can execute.
        // Restricting this scan to `native_entries` omitted exactly those
        // background-published functions, so their first cross-unit call saw
        // an empty target cell and only a later invocation became correct.
        published_callers.extend(
            caller
                .compiled
                .prepared_deployment_image()
                .preferred_function_entries
                .iter()
                .enumerate()
                .filter_map(|(function, entry)| {
                    (entry.load(std::sync::atomic::Ordering::Acquire) != 0)
                        .then(|| u32::try_from(function).ok())
                        .flatten()
                        .map(php_ir::FunctionId::new)
                }),
        );
        published_callers.sort_unstable();
        published_callers.dedup();
        for caller_function in published_callers {
            callers.push((caller_unit, caller_function));
        }
    }
    for (unit, function) in callers {
        prepare_linked_function_entries_for_caller(context, unit, function)?;
    }
    Ok(())
}

/// Publishes the exact outgoing native links of one already published caller.
///
/// Target publication recurses through this same scoped operation, so a
/// reachable call graph is prepared once without repeatedly walking dormant
/// units and unrelated callers.
fn prepare_linked_function_entries_for_caller(
    context: &mut NativeRequestColdState<'_>,
    caller_unit: usize,
    caller_function: php_ir::FunctionId,
) -> Result<(), String> {
    let mut targets = context
        .dynamic_units
        .get(caller_unit)
        .ok_or_else(|| "linked native caller unit is missing".to_owned())?
        .compiled
        .prepared_external_function_calls(caller_function)
        .iter()
        .filter_map(|call| {
            prepared_external_call_target(context, call)
                .and_then(|target| target.function())
                .map(|target| (target.unit, target.function))
        })
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(package) = context.dynamic_units.get(caller_unit) {
        for specialization in package
            .compiled
            .prepared_method_specializations(caller_function)
        {
            let php_jit::JitMethodSpecializationTarget::Linked(signature) = specialization.target
            else {
                continue;
            };
            let call = crate::compiled_unit::PreparedExternalFunctionCall {
                normalized_name: signature.name.to_ascii_lowercase().into_boxed_str(),
                source_name: signature.name.into_boxed_str(),
                link_index: signature.link_index,
            };
            if let Some(target) =
                prepared_external_call_target(context, &call).and_then(|target| target.function())
            {
                targets.insert((target.unit, target.function));
            }
        }
    }
    for (unit, function) in targets {
        let unpublished = context
            .dynamic_units
            .get(unit)
            .and_then(|package| {
                package
                    .compiled
                    .prepared_deployment_image()
                    .native_function_entries
                    .get(function.index())
            })
            .is_none_or(|entry| entry.load(std::sync::atomic::Ordering::Acquire) == 0);
        if unpublished {
            prepare_dynamic_native_entry(context, unit, function)?;
        }
    }
    let compiled = context
        .dynamic_units
        .get(caller_unit)
        .ok_or_else(|| "linked native caller unit disappeared".to_owned())?
        .compiled
        .clone();
    let external_signatures =
        visible_external_function_signatures(context, &compiled, caller_function);
    if !external_signatures.is_empty()
        && context.options.native_optimization == super::super::NativeOptimizationPolicy::Optimizing
        && context.worker_state.has_compiled_optimizing_function(
            &compiled,
            caller_function,
            &external_signatures,
        )
    {
        // Background compilation deliberately leaves cross-unit products
        // unpublished. All exact targets are prepared above, so this
        // request-owned declaration boundary can now atomically adopt the
        // completed product without exposing it to an older runtime view.
        let optimizing = context.worker_state.resolve_native_function(
            &compiled,
            caller_function,
            context.options,
            &external_signatures,
        )?;
        publish_dynamic_unit_entry(&compiled, caller_function, &optimizing);
        if context.current_dynamic_unit == Some(caller_unit) {
            std::sync::Arc::make_mut(&mut context.native_entries)
                .insert(caller_function, optimizing);
        } else if let Some(package) = context.dynamic_units.get_mut(caller_unit) {
            std::sync::Arc::make_mut(&mut package.native_entries)
                .insert(caller_function, optimizing);
        }
    }
    refresh_linked_function_records_for_function(context, caller_unit, caller_function);
    Ok(())
}

/// Recompile already published callers whose immutable external-call index
/// now resolves against declarations added by the current publication.
///
/// This is deliberately a declaration-time operation. Generated optimizing
/// code must not discover a typed/default/variadic/reference signature at its
/// first invocation and enter an operation-local fallback before it can pack
/// the authoritative native frame.
fn prepare_resolved_external_callers(
    context: &mut NativeRequestColdState<'_>,
    published_names: &[String],
    published_classes: &[String],
) -> Result<(), String> {
    if published_names.is_empty() && published_classes.is_empty() {
        return Ok(());
    }
    let published_names = published_names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    let published_classes = published_classes
        .iter()
        .map(|name| normalized_class_name(name).into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let mut callers = Vec::new();
    for (unit, package) in context.dynamic_units.iter().enumerate() {
        for function in 0..package.compiled.unit().functions.len() {
            let function = php_ir::FunctionId::new(
                u32::try_from(function)
                    .map_err(|_| "dynamic function index exceeds the native ABI".to_owned())?,
            );
            let resolves_published_name = package
                .compiled
                .prepared_external_function_calls(function)
                .iter()
                .any(|call| {
                    published_names.contains(call.normalized_name.as_ref())
                        || call
                            .normalized_name
                            .rsplit_once("::")
                            .is_some_and(|(class, _)| published_classes.contains(class))
                })
                || package
                    .compiled
                    .prepared_method_specializations(function)
                    .into_iter()
                    .any(|specialization| {
                        let php_jit::JitMethodSpecializationTarget::Linked(signature) =
                            specialization.target
                        else {
                            return false;
                        };
                        let normalized = signature.name.to_ascii_lowercase();
                        published_names.contains(&normalized)
                            || normalized
                                .rsplit_once("::")
                                .is_some_and(|(class, _)| published_classes.contains(class))
                    });
            let mapped_native_entry = if context.current_dynamic_unit == Some(unit) {
                context.native_entries.contains_key(&function)
            } else {
                package.native_entries.contains_key(&function)
            };
            // On-demand same-unit compilation publishes the canonical
            // baseline entry cell without retaining a duplicate handle in
            // every request-local map. Declaration-time external-link
            // refresh must therefore consult that publication cell as well;
            // otherwise an already compiled caller is silently left with its
            // unresolved specialization after an include declares the target.
            let deployed_native_entry = package
                .compiled
                .prepared_deployment_image()
                .native_function_entries
                .get(function.index())
                .is_some_and(|entry| entry.load(std::sync::atomic::Ordering::Acquire) != 0);
            let has_native_entry = mapped_native_entry || deployed_native_entry;
            if resolves_published_name && has_native_entry {
                callers.push((unit, function));
            }
        }
    }
    for (unit, function) in callers {
        prepare_dynamic_native_entry(context, unit, function)?;
    }
    Ok(())
}

fn linked_function_record(
    context: &NativeRequestColdState<'_>,
    call: &crate::compiled_unit::PreparedExternalFunctionCall,
) -> Option<php_jit::JitNativeLinkedFunction> {
    let target = prepared_external_call_target(context, call)?;
    let target_unit = context.dynamic_units.get(target.unit())?;
    if target_unit.published_runtime_view.abi_version != php_jit::JIT_RUNTIME_ABI_VERSION {
        return None;
    }
    let (preferred_entry, baseline_entry) = target.function().map_or(Some((0, 0)), |function| {
        let deployment = target_unit.compiled.prepared_deployment_image();
        Some((
            std::ptr::from_ref(
                deployment
                    .preferred_function_entries
                    .get(function.function.index())?,
            ) as usize as u64,
            std::ptr::from_ref(
                deployment
                    .native_function_entries
                    .get(function.function.index())?,
            ) as usize as u64,
        ))
    })?;
    let prepared_class = call
        .source_name
        .rsplit_once("::")
        .and_then(|(class, _)| prepared_external_class_plan(context, class))
        .unwrap_or(0);
    if matches!(target, PreparedExternalCallTarget::Class { .. }) && prepared_class == 0 {
        return None;
    }
    Some(php_jit::JitNativeLinkedFunction {
        preferred_entry,
        baseline_entry,
        runtime_view: std::ptr::from_ref(target_unit.published_runtime_view.as_ref()) as usize
            as u64,
        prepared_class,
    })
}

fn linked_function_record_updates(
    context: &NativeRequestColdState<'_>,
    calls: &[crate::compiled_unit::PreparedExternalFunctionCall],
) -> Vec<(usize, php_jit::JitNativeLinkedFunction)> {
    calls
        .iter()
        .map(|call| {
            (
                call.link_index as usize,
                linked_function_record(context, call).unwrap_or_default(),
            )
        })
        .collect()
}

fn linked_method_record(
    context: &NativeRequestColdState<'_>,
    signature: &php_jit::JitExternalFunctionSignature,
) -> Option<php_jit::JitNativeLinkedFunction> {
    let call = crate::compiled_unit::PreparedExternalFunctionCall {
        normalized_name: signature.name.to_ascii_lowercase().into_boxed_str(),
        source_name: signature.name.clone().into_boxed_str(),
        link_index: signature.link_index,
    };
    let current =
        collect_visible_external_function_signatures(context, std::slice::from_ref(&call))
            .into_iter()
            .next()?;
    if &current != signature {
        return None;
    }
    linked_function_record(context, &call)
}

fn linked_method_record_updates(
    context: &NativeRequestColdState<'_>,
    compiled: &crate::compiled_unit::CompiledUnit,
) -> Vec<(usize, php_jit::JitNativeLinkedFunction)> {
    compiled
        .unit()
        .functions
        .iter()
        .enumerate()
        .filter_map(|(function, _)| u32::try_from(function).ok().map(php_ir::FunctionId::new))
        .flat_map(|function| compiled.prepared_method_specializations(function))
        .filter_map(|specialization| {
            let php_jit::JitMethodSpecializationTarget::Linked(signature) = specialization.target
            else {
                return None;
            };
            Some((
                signature.link_index as usize,
                linked_method_record(context, &signature).unwrap_or_default(),
            ))
        })
        .collect()
}

fn apply_linked_function_record_updates(
    context: &mut NativeRequestColdState<'_>,
    caller_unit: usize,
    updates: Vec<(usize, php_jit::JitNativeLinkedFunction)>,
) {
    let Some(package) = context.dynamic_units.get_mut(caller_unit) else {
        return;
    };
    for (link_index, record) in updates {
        if let Some(slot) = package.linked_functions.get_mut(link_index) {
            *slot = record;
        }
    }
}

pub(super) fn refresh_linked_function_records_for_unit(
    context: &mut NativeRequestColdState<'_>,
    caller_unit: usize,
) {
    let mut updates = context
        .dynamic_units
        .get(caller_unit)
        .map_or_else(Vec::new, |package| {
            linked_function_record_updates(
                context,
                package.compiled.prepared_unit_external_function_calls(),
            )
        });
    if let Some(package) = context.dynamic_units.get(caller_unit) {
        updates.extend(linked_method_record_updates(context, &package.compiled));
    }
    apply_linked_function_record_updates(context, caller_unit, updates);
}

fn refresh_linked_function_records_for_function(
    context: &mut NativeRequestColdState<'_>,
    caller_unit: usize,
    function: php_ir::FunctionId,
) {
    let mut updates = context
        .dynamic_units
        .get(caller_unit)
        .map_or_else(Vec::new, |package| {
            linked_function_record_updates(
                context,
                package.compiled.prepared_external_function_calls(function),
            )
        });
    if let Some(package) = context.dynamic_units.get(caller_unit) {
        updates.extend(
            package
                .compiled
                .prepared_method_specializations(function)
                .into_iter()
                .filter_map(|specialization| {
                    let php_jit::JitMethodSpecializationTarget::Linked(signature) =
                        specialization.target
                    else {
                        return None;
                    };
                    Some((
                        signature.link_index as usize,
                        linked_method_record(context, &signature).unwrap_or_default(),
                    ))
                }),
        );
    }
    apply_linked_function_record_updates(context, caller_unit, updates);
}

pub(super) fn refresh_linked_function_records(context: &mut NativeRequestColdState<'_>) {
    for unit in 0..context.dynamic_units.len() {
        refresh_linked_function_records_for_unit(context, unit);
    }
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
            invoke_registered_autoload_callbacks_until(
                context,
                name.as_bytes(),
                source,
                |context| native_external_class_exists(context, name),
            )?;
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
