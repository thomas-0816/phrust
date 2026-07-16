use super::*;

pub(super) fn register_native_dynamic_unit(
    context: &mut NativeExecutionContext<'_>,
    compiled: crate::compiled_unit::CompiledUnit,
    exports: NativeIncludeExports,
) -> Result<(), String> {
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
            || native_external_class(context, class).is_some()
        {
            return Err(format!(
                "Cannot declare class {class}, because the name is already in use"
            ));
        }
    }
    let exported_classes = classes
        .iter()
        .map(|class| normalize_class_name(class))
        .collect();
    let unit = context.dynamic_units.len();
    context.dynamic_units.push(NativeDynamicUnit {
        native_entry_signature_hashes,
        compiled,
        native_entries,
        exported_classes,
    });
    for (name, function) in functions {
        context.external_functions.insert(
            name.to_ascii_lowercase(),
            NativeDynamicFunction { unit, function },
        );
    }
    context.publish_function_names(published_function_names);
    for class in classes {
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
            context.dynamic_constants.insert(name, value);
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
    Ok(())
}

pub(super) fn native_entries_from_records(
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
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
) -> Result<php_jit::JitFunctionHandle, String> {
    if let Some(handle) = context.native_entries.get(&function) {
        return Ok(handle.clone());
    }
    let external_signatures =
        visible_external_function_signatures(context, context.compiled, function);
    let (records, _) = context.worker_state.compile_native(
        context.compiled,
        function,
        context.options,
        &external_signatures,
    )?;
    context
        .native_entries
        .extend(native_entries_from_records(&records)?);
    context
        .native_entries
        .get(&function)
        .cloned()
        .ok_or_else(|| format!("native function entry {} was not published", function.raw()))
}

pub(super) fn ensure_dynamic_native_entry(
    context: &mut NativeExecutionContext<'_>,
    unit: usize,
    function: php_ir::FunctionId,
) -> Result<php_jit::JitFunctionHandle, String> {
    let package = context
        .dynamic_units
        .get(unit)
        .ok_or_else(|| "dynamic native unit is missing".to_owned())?;
    let compiled = package.compiled.clone();
    let external_signatures = visible_external_function_signatures(context, &compiled, function);
    let signature_hash = super::super::external_function_signatures_hash(&external_signatures);
    if package.native_entry_signature_hashes.get(&function) == Some(&signature_hash)
        && let Some(handle) = package.native_entries.get(&function)
    {
        return Ok(handle.clone());
    }
    let (records, _) = context.worker_state.compile_native(
        &compiled,
        function,
        context.options,
        &external_signatures,
    )?;
    let entries = native_entries_from_records(&records)?;
    let package = context
        .dynamic_units
        .get_mut(unit)
        .ok_or_else(|| "dynamic native unit disappeared during compilation".to_owned())?;
    for compiled_function in entries.keys() {
        package
            .native_entry_signature_hashes
            .insert(*compiled_function, signature_hash);
    }
    package.native_entries.extend(entries);
    package
        .native_entries
        .get(&function)
        .cloned()
        .ok_or_else(|| {
            format!(
                "dynamic native function entry {} was not published",
                function.raw()
            )
        })
}

pub(super) fn visible_external_function_signatures(
    context: &NativeExecutionContext<'_>,
    compiled: &crate::compiled_unit::CompiledUnit,
    root: php_ir::FunctionId,
) -> Vec<php_jit::JitExternalFunctionSignature> {
    collect_visible_external_function_signatures(context, compiled, [root])
}

pub(super) fn visible_external_function_signatures_for_unit(
    context: &NativeExecutionContext<'_>,
    compiled: &crate::compiled_unit::CompiledUnit,
) -> Vec<php_jit::JitExternalFunctionSignature> {
    collect_visible_external_function_signatures(
        context,
        compiled,
        (0..compiled.unit().functions.len())
            .filter_map(|index| u32::try_from(index).ok())
            .map(php_ir::FunctionId::new),
    )
}

pub(super) fn collect_visible_external_function_signatures(
    context: &NativeExecutionContext<'_>,
    compiled: &crate::compiled_unit::CompiledUnit,
    roots: impl IntoIterator<Item = php_ir::FunctionId>,
) -> Vec<php_jit::JitExternalFunctionSignature> {
    let local_functions = compiled
        .unit()
        .function_table
        .iter()
        .map(|entry| (entry.name.to_ascii_lowercase(), entry.function))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut local_methods = std::collections::BTreeMap::<String, Vec<php_ir::FunctionId>>::new();
    for method in compiled
        .unit()
        .classes
        .iter()
        .flat_map(|class| &class.methods)
    {
        local_methods
            .entry(method.name.to_ascii_lowercase())
            .or_default()
            .push(method.function);
    }
    let mut reachable = std::collections::BTreeSet::new();
    let mut pending = roots
        .into_iter()
        .filter(|function| reachable.insert(*function))
        .collect::<Vec<_>>();
    let mut called_external_functions = std::collections::BTreeMap::new();
    while let Some(function_id) = pending.pop() {
        let Some(function) = compiled.unit().functions.get(function_id.index()) else {
            continue;
        };
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            match &instruction.kind {
                php_ir::InstructionKind::CallFunction { name, .. }
                | php_ir::InstructionKind::BindReferenceFromCall { name, .. } => {
                    let normalized = name.to_ascii_lowercase();
                    if let Some(callee) = local_functions.get(&normalized) {
                        if reachable.insert(*callee) {
                            pending.push(*callee);
                        }
                    } else {
                        called_external_functions.insert(normalized, name.clone());
                    }
                }
                php_ir::InstructionKind::CallMethod { method, .. }
                | php_ir::InstructionKind::CallStaticMethod { method, .. }
                | php_ir::InstructionKind::BindReferenceFromMethodCall { method, .. } => {
                    // Native lowering can turn a same-unit method invocation
                    // into a direct call. Follow every same-named local method
                    // conservatively because the IR receiver may be dynamic;
                    // this keeps late external by-reference signatures in the
                    // cache identity of the actual entry-point call graph.
                    if let Some(callees) = local_methods.get(&method.to_ascii_lowercase()) {
                        for callee in callees {
                            if reachable.insert(*callee) {
                                pending.push(*callee);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    called_external_functions
        .into_iter()
        .filter_map(|(normalized, call_name)| {
            let target = context.external_functions.get(&normalized)?;
            let function = context
                .dynamic_units
                .get(target.unit)?
                .compiled
                .unit()
                .functions
                .get(target.function.index())?;
            // Ordinary by-value calls need no compile-time cross-unit
            // contract: the runtime binder clears speculative local flags and
            // dimension/property locations remain values. Only a by-reference
            // parameter changes the generated caller's lvalue preparation.
            if !function.params.iter().any(|parameter| parameter.by_ref) {
                return None;
            }
            Some(php_jit::JitExternalFunctionSignature {
                // Match the source unit's call target. The lowering lookup is
                // intentionally independent of the publishing unit's spelling.
                name: call_name,
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

pub(super) fn native_external_class(
    context: &NativeExecutionContext<'_>,
    name: &str,
) -> Option<(usize, php_ir::module::ClassEntry)> {
    let requested = normalize_class_name(name);
    let normalized = context
        .class_aliases
        .get(&requested)
        .map_or(requested.as_str(), String::as_str);
    context
        .dynamic_units
        .iter()
        .enumerate()
        .rev()
        .find_map(|(unit, package)| {
            if context.current_dynamic_unit == Some(unit) {
                return None;
            }
            if !package.exported_classes.contains(normalized) {
                return None;
            }
            package
                .compiled
                .unit()
                .classes
                .iter()
                .find(|class| class.name == normalized)
                .cloned()
                .map(|class| (unit, class))
        })
}

pub(super) fn native_autoload_class(
    context: &mut NativeExecutionContext<'_>,
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
        if native_external_class(context, name).is_none() {
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
                if native_external_class(context, name).is_some() {
                    break;
                }
            }
        }
        if let Some((_, class)) = native_external_class(context, name) {
            let dependencies = class
                .parent_display_name
                .or(class.parent)
                .into_iter()
                .chain(class.interfaces);
            for dependency in dependencies {
                native_autoload_class(context, &dependency, source)?;
            }
        }
        Ok(())
    })();
    context.autoload_in_progress.remove(&normalized);
    result
}
