//! Foreach iterator state, construction, stepping, and property visibility.

use super::prelude::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ObjectPropertyIterationEntry {
    pub(super) key: String,
    pub(super) storage_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ForeachIterator {
    Snapshot {
        entries: Vec<(ArrayKey, Value)>,
        position: usize,
    },
    /// By-value array iteration over a shared handle: copy-on-write gives
    /// snapshot semantics without cloning the elements up front. Arrays
    /// with top-level reference elements keep the eager snapshot so the
    /// init-time dereference behavior is unchanged.
    ArrayHandle { array: PhpArray, position: usize },
    ObjectProperties {
        object: ObjectRef,
        entries: Vec<ObjectPropertyIterationEntry>,
        position: usize,
    },
    IteratorObject {
        object: ObjectRef,
        needs_next: bool,
        always_call_key: bool,
    },
    ByReference {
        local: LocalId,
        visited_keys: Vec<ArrayKey>,
    },
    Generator {
        generator: GeneratorRef,
        consumed: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ForeachInvalidSourceBehavior {
    Unsupported,
    WarnAndEmpty { span: Option<php_ir::IrSpan> },
}

pub(super) fn foreach_iterator_candidate_value(iterator: ForeachIterator) -> Option<Value> {
    match iterator {
        ForeachIterator::IteratorObject { object, .. }
        | ForeachIterator::ObjectProperties { object, .. } => Some(Value::Object(object)),
        ForeachIterator::Generator { generator, .. } => Some(Value::Generator(generator)),
        ForeachIterator::ArrayHandle { array, .. } => Some(Value::Array(array)),
        ForeachIterator::Snapshot { .. } | ForeachIterator::ByReference { .. } => None,
    }
}

impl Vm {
    pub(super) fn next_foreach_value(
        &self,
        cursor: ExecutionCursor<'_>,
        foreach_iterators: &mut HashMap<RegId, ForeachIterator>,
        iterator: RegId,
        needs_key: bool,
    ) -> Result<Option<(Option<Value>, Value)>, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        // Step the iterator in place: cloning the whole `ForeachIterator`
        // per step deep-copied the snapshot entries vector on every
        // iteration. Only the produced element is cloned (PHP by-value copy
        // semantics); the object/generator branches clone their cheap
        // handles so the map borrow ends before nested calls re-enter.
        let next_value = match foreach_iterators.get_mut(&iterator) {
            Some(ForeachIterator::Snapshot { entries, position }) => {
                let _source = layout_source::enter(layout_source::FOREACH_VALUE);
                let next = entries
                    .get(*position)
                    .cloned()
                    .map(|(key, value)| (Some(array_key_to_value(key)), value));
                if next.is_some() {
                    *position += 1;
                    self.record_counter_value_clone_reason(layout_source::FOREACH_VALUE.name());
                }
                next
            }
            Some(ForeachIterator::ArrayHandle { array, position }) => {
                let _source = layout_source::enter(layout_source::FOREACH_VALUE);
                let next = array.next_pair_at_cursor(position).map(|(key, value)| {
                    let value = match value {
                        Value::Reference(cell) => cell.get(),
                        other => other,
                    };
                    (Some(array_key_to_value(key)), value)
                });
                if next.is_some() {
                    self.record_counter_value_clone_reason(layout_source::FOREACH_VALUE.name());
                }
                next
            }
            Some(ForeachIterator::ObjectProperties {
                object,
                entries,
                position,
            }) => {
                let _source = layout_source::enter(layout_source::FOREACH_VALUE);
                let next = entries.get(*position).map(|entry| {
                    let value = object
                        .get_property(&entry.storage_name)
                        .map(|value| effective_value(&value))
                        .unwrap_or(Value::Null);
                    (Some(Value::string(entry.key.clone().into_bytes())), value)
                });
                if next.is_some() {
                    *position += 1;
                }
                next
            }
            Some(ForeachIterator::IteratorObject {
                object,
                needs_next,
                always_call_key,
            }) => {
                let object = object.clone();
                let needs_next = *needs_next;
                let always_call_key = *always_call_key;
                if needs_next {
                    self.call_object_method_value(
                        compiled,
                        object.clone(),
                        "next",
                        output,
                        stack,
                        state,
                    )?;
                }
                if matches!(
                    spl_runtime_marker(&object).as_deref(),
                    Some("recursiveiteratoriterator" | "recursivetreeiterator")
                ) && spl_rii_should_call_valid_child_hook(&object)
                {
                    self.call_spl_rii_child_hook(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        "callHasChildren",
                        "RecursiveIteratorIterator->valid",
                        None,
                    )?;
                }
                let valid = self.call_object_method_value(
                    compiled,
                    object.clone(),
                    "valid",
                    output,
                    stack,
                    state,
                )?;
                if !to_bool(&valid)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?
                {
                    None
                } else {
                    let entry_value = self.call_object_method_value(
                        compiled,
                        object.clone(),
                        "current",
                        output,
                        stack,
                        state,
                    )?;
                    let entry_key = if needs_key || always_call_key {
                        let key_value = self.call_object_method_value(
                            compiled,
                            object.clone(),
                            "key",
                            output,
                            stack,
                            state,
                        )?;
                        needs_key.then_some(key_value)
                    } else {
                        None
                    };
                    if let Some(ForeachIterator::IteratorObject { needs_next, .. }) =
                        foreach_iterators.get_mut(&iterator)
                    {
                        *needs_next = true;
                    }
                    Some((entry_key, entry_value))
                }
            }
            Some(ForeachIterator::Generator {
                generator,
                consumed,
            }) => {
                let generator = generator.clone();
                let was_consumed = *consumed;
                *consumed = true;
                if was_consumed {
                    self.resume_generator_to_next_yield(
                        compiled,
                        generator,
                        GeneratorResumeInput::Value(Value::Null),
                        output,
                        stack,
                        state,
                    )?
                } else {
                    self.advance_generator_to_first_yield(
                        compiled, generator, output, stack, state,
                    )?
                }
            }
            Some(ForeachIterator::ByReference { .. }) | None => {
                return Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_FOREACH_ITERATOR_MISSING: iterator r{} is not initialized",
                        iterator.raw()
                    ),
                )));
            }
        };

        if let Some((entry_key, entry_value)) = &next_value {
            self.record_runtime_trace_event(|| {
                format!(
                    "foreach next iterator=r{} status=value key={} value={}",
                    iterator.raw(),
                    entry_key
                        .as_ref()
                        .map(trace_value)
                        .unwrap_or_else(|| "None".to_owned()),
                    trace_value(entry_value)
                )
            });
        } else {
            self.record_runtime_trace_event(|| {
                format!("foreach next iterator=r{} status=done", iterator.raw())
            });
        }
        Ok(next_value)
    }

    pub(super) fn foreach_iterator_from_value(
        &self,
        compiled: &CompiledUnit,
        source: Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        invalid_source_behavior: ForeachInvalidSourceBehavior,
    ) -> Result<ForeachIterator, Box<VmResult>> {
        match source {
            Value::Array(array) => {
                // Handle-based iteration: copy-on-write supplies snapshot
                // semantics for source mutation, and reference elements
                // dereference at visit time, matching the reference engine
                // (the previous eager snapshot dereferenced at init).
                if array.is_packed_fast() {
                    self.record_counter_array_sequential_foreach_fast_path_hit();
                }
                self.record_counter_foreach_no_clone_hit();
                Ok(ForeachIterator::ArrayHandle { array, position: 0 })
            }
            Value::Generator(generator) => Ok(ForeachIterator::Generator {
                generator,
                consumed: false,
            }),
            Value::Object(object) => {
                self.foreach_iterator_from_object(compiled, object, output, stack, state)
            }
            source => {
                if let ForeachInvalidSourceBehavior::WarnAndEmpty { span } = invalid_source_behavior
                {
                    self.emit_foreach_invalid_source_warning(
                        compiled, output, stack, state, &source, span,
                    )?;
                    return Ok(ForeachIterator::Snapshot {
                        entries: Vec::new(),
                        position: 0,
                    });
                }
                let diagnostic = unsupported_feature(
                    "E_PHP_VM_UNSUPPORTED_FOREACH_SOURCE",
                    format!(
                        "foreach over {} is not implemented; runtime-semantics supports arrays, public-property objects, Iterator, IteratorAggregate, and generator MVP objects",
                        value_type_name(&source)
                    ),
                    RuntimeSourceSpan::default(),
                    stack_trace(compiled, stack),
                );
                Err(Box::new(VmResult {
                    status: ExecutionStatus::unsupported(diagnostic.message().to_owned()),
                    output: output.clone(),
                    diagnostics: vec![diagnostic],
                    return_value: None,
                    returned_explicitly: false,
                    process_exit_code: None,
                    process_exit_terminates_process: false,
                    yielded: None,
                    fiber_suspension: None,
                    return_ref: None,
                    trace: Vec::new(),
                    counters: None,
                    tiering_stats: None,
                    http_response: None,
                    upload_registry: None,
                    session: None,
                }))
            }
        }
    }

    pub(super) fn emit_foreach_invalid_source_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        source: &Value,
        span: Option<php_ir::IrSpan>,
    ) -> Result<(), Box<VmResult>> {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_FOREACH_INVALID_SOURCE",
            RuntimeSeverity::Warning,
            format!(
                "foreach() argument must be of type array|object, {} given",
                value_type_name(source)
            ),
            span.map_or_else(RuntimeSourceSpan::default, |span| {
                runtime_source_span(compiled, span)
            }),
            stack_trace(compiled, stack),
            Some(php_runtime::api::PhpReferenceClassification::Warning),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::api::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::api::PhpDiagnosticChannel::Warning,
                php_runtime::api::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn foreach_iterator_from_object(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<ForeachIterator, Box<VmResult>> {
        let class_name = object.class_name();
        if self
            .spl_object_has_userland_method(compiled, state, &object, "getIterator")
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
        {
            let inner = self.call_object_method_value(
                compiled,
                object,
                "getIterator",
                output,
                stack,
                state,
            )?;
            return self.foreach_iterator_from_value(
                compiled,
                inner,
                output,
                stack,
                state,
                ForeachInvalidSourceBehavior::Unsupported,
            );
        }
        if spl_runtime_marker(&object).as_deref() == Some("iteratoriterator")
            && let Some(inner) = spl_inner_iterator_delegation_target(&object)
        {
            return self.foreach_iterator_from_value(
                compiled,
                Value::Object(inner),
                output,
                stack,
                state,
                ForeachInvalidSourceBehavior::Unsupported,
            );
        }
        if spl_runtime_marker(&object).is_some_and(|class| is_spl_iterator_runtime_class(&class)) {
            let always_call_key = matches!(
                spl_runtime_marker(&object).as_deref(),
                Some("infiniteiterator" | "limititerator")
            );
            self.call_object_method_value(
                compiled,
                object.clone(),
                "rewind",
                output,
                stack,
                state,
            )?;
            if spl_runtime_marker(&object).as_deref() == Some("recursiveiteratoriterator") {
                object.set_property("__rii_direct_at_root", Value::Bool(false));
            }
            return Ok(ForeachIterator::IteratorObject {
                object,
                needs_next: false,
                always_call_key,
            });
        }
        if spl_runtime_marker(&object).is_some_and(|class| is_spl_container_runtime_class(&class)) {
            self.call_object_method_value(
                compiled,
                object.clone(),
                "rewind",
                output,
                stack,
                state,
            )?;
            return Ok(ForeachIterator::IteratorObject {
                object,
                needs_next: false,
                always_call_key: false,
            });
        }
        if spl_runtime_marker(&object).is_some_and(|class| is_spl_heap_runtime_class(&class)) {
            self.call_object_method_value(
                compiled,
                object.clone(),
                "rewind",
                output,
                stack,
                state,
            )?;
            return Ok(ForeachIterator::IteratorObject {
                object,
                needs_next: false,
                always_call_key: false,
            });
        }
        if spl_runtime_marker(&object).is_some_and(|class| is_spl_file_runtime_class(&class)) {
            self.call_object_method_value(
                compiled,
                object.clone(),
                "rewind",
                output,
                stack,
                state,
            )?;
            return Ok(ForeachIterator::IteratorObject {
                object,
                needs_next: false,
                always_call_key: false,
            });
        }
        match class_is_a_in_state(compiled, state, &class_name, "Iterator") {
            Ok(true) => {
                self.call_object_method_value(
                    compiled,
                    object.clone(),
                    "rewind",
                    output,
                    stack,
                    state,
                )?;
                return Ok(ForeachIterator::IteratorObject {
                    object,
                    needs_next: false,
                    always_call_key: false,
                });
            }
            Ok(false) => {}
            Err(message) => {
                return Err(Box::new(
                    self.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        match class_is_a_in_state(compiled, state, &class_name, "IteratorAggregate") {
            Ok(true) => {
                let inner = self.call_object_method_value(
                    compiled,
                    object,
                    "getIterator",
                    output,
                    stack,
                    state,
                )?;
                return self.foreach_iterator_from_value(
                    compiled,
                    inner,
                    output,
                    stack,
                    state,
                    ForeachInvalidSourceBehavior::Unsupported,
                );
            }
            Ok(false) => {}
            Err(message) => {
                return Err(Box::new(
                    self.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        let scope = current_scope_class(compiled, stack);
        let entries = object_property_iteration_entries(compiled, state, &object, scope.as_deref())
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        Ok(ForeachIterator::ObjectProperties {
            object,
            entries,
            position: 0,
        })
    }
}

pub(super) fn format_foreach_iterator_kind(iterator: &ForeachIterator) -> &'static str {
    match iterator {
        ForeachIterator::Snapshot { .. } => "snapshot",
        ForeachIterator::ArrayHandle { .. } => "array-handle",
        ForeachIterator::ObjectProperties { .. } => "object-properties",
        ForeachIterator::IteratorObject { .. } => "iterator-object",
        ForeachIterator::Generator { .. } => "generator",
        ForeachIterator::ByReference { .. } => "by-reference",
    }
}

pub(super) fn foreach_array_keys_from_local_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> Result<Vec<ArrayKey>, String> {
    let value = read_local_value_at_frame(stack, frame_index, local).unwrap_or(Value::Null);
    let value = effective_value(&value);
    let Value::Array(array) = value else {
        return Err(format!(
            "E_PHP_VM_UNSUPPORTED_FOREACH_SOURCE: foreach by reference over {} is not implemented; runtime-semantics supports local arrays only",
            value_type_name(&value)
        ));
    };
    Ok(array.iter().map(|(key, _)| key.clone()).collect())
}

pub(super) fn object_property_iteration_entries(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    object: &ObjectRef,
    caller_scope: Option<&str>,
) -> Result<Vec<ObjectPropertyIterationEntry>, String> {
    let class_name = object.class_name();
    if normalize_class_name(&class_name) == "simplexmlelement" {
        if let Some(Value::Array(entries)) = object.get_property("__entries") {
            let names = object.get_property("__entry_names");
            return Ok(entries
                .iter()
                .map(|(key, _)| match key {
                    ArrayKey::Int(index) => {
                        let key = names
                            .as_ref()
                            .and_then(|value| match value {
                                Value::Array(names) => names.get(&ArrayKey::Int(index)),
                                _ => None,
                            })
                            .and_then(|value| match value {
                                Value::String(name) => Some(name.to_string_lossy()),
                                _ => None,
                            })
                            .unwrap_or_else(|| index.to_string());
                        ObjectPropertyIterationEntry {
                            key,
                            storage_name: index.to_string(),
                        }
                    }
                    ArrayKey::String(key) => ObjectPropertyIterationEntry {
                        key: key.to_string_lossy(),
                        storage_name: key.to_string_lossy(),
                    },
                })
                .collect());
        }
        return Ok(object
            .properties_snapshot()
            .into_iter()
            .filter_map(|(name, _)| {
                if name.starts_with("__") || name.contains(':') {
                    None
                } else {
                    Some(ObjectPropertyIterationEntry {
                        key: name.clone(),
                        storage_name: name,
                    })
                }
            })
            .collect());
    }
    if normalize_class_name(&class_name) == "domnodelist" {
        let Some(Value::Array(entries)) = object.get_property("__entries") else {
            return Ok(Vec::new());
        };
        return Ok(entries
            .iter()
            .map(|(key, _)| match key {
                ArrayKey::Int(index) => ObjectPropertyIterationEntry {
                    key: index.to_string(),
                    storage_name: index.to_string(),
                },
                ArrayKey::String(key) => ObjectPropertyIterationEntry {
                    key: key.to_string_lossy(),
                    storage_name: key.to_string_lossy(),
                },
            })
            .collect());
    }
    let Some(class) = lookup_class_in_state(compiled, state, &class_name) else {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
            class_name
        ));
    };
    let mut entries = Vec::new();
    let mut declared_names = Vec::new();
    collect_object_property_iteration_entries(
        compiled,
        state,
        &class,
        object,
        caller_scope,
        &mut declared_names,
        &mut entries,
    )?;
    for (name, _value) in object.properties_snapshot() {
        if name.contains(':') || declared_names.iter().any(|declared| declared == &name) {
            continue;
        }
        if !entries.iter().any(|existing| existing.key == name) {
            entries.push(ObjectPropertyIterationEntry {
                key: name.clone(),
                storage_name: name,
            });
        }
    }
    Ok(entries)
}

fn collect_object_property_iteration_entries(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    object: &ObjectRef,
    caller_scope: Option<&str>,
    declared_names: &mut Vec<String>,
    entries: &mut Vec<ObjectPropertyIterationEntry>,
) -> Result<(), String> {
    if let Some(parent) = class
        .parent
        .as_deref()
        .and_then(|parent| lookup_class_in_state(compiled, state, parent))
    {
        collect_object_property_iteration_entries(
            compiled,
            state,
            &parent,
            object,
            caller_scope,
            declared_names,
            entries,
        )?;
    }
    for property in &class.properties {
        if property.flags.is_static {
            continue;
        }
        if !declared_names.iter().any(|name| name == &property.name) {
            declared_names.push(property.name.clone());
        }
        if !object_property_visible_for_iteration(compiled, state, class, property, caller_scope)? {
            continue;
        }
        let storage_name = property_storage_name(class, property);
        if object.get_property(&storage_name).is_none() {
            continue;
        }
        if !entries.iter().any(|entry| entry.key == property.name) {
            entries.push(ObjectPropertyIterationEntry {
                key: property.name.clone(),
                storage_name,
            });
        }
    }
    Ok(())
}

fn object_property_visible_for_iteration(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
    caller_scope: Option<&str>,
) -> Result<bool, String> {
    if property.flags.is_private {
        return Ok(caller_scope.is_some_and(|scope| {
            normalize_class_name(scope) == normalize_class_name(&class.name)
        }));
    }
    if property.flags.is_protected {
        let Some(scope) = caller_scope else {
            return Ok(false);
        };
        return class_is_a_in_state(compiled, state, scope, &class.name);
    }
    Ok(true)
}
