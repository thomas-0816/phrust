use super::*;

pub(super) fn execute_rich_foreach_instruction(
    vm: &Vm,
    compiled: &CompiledUnit,
    unit: &IrUnit,
    frame_index: usize,
    kind: &InstructionKind,
    span: IrSpan,
    output: &mut OutputBuffer,
    stack: &mut CallStack,
    state: &mut ExecutionState,
    foreach_iterators: &mut HashMap<RegId, ForeachIterator>,
    exception_handlers: &mut Vec<ExceptionHandler>,
    pending_control: &mut Option<PendingControl>,
) -> RichDispatchOutcome {
    match kind {
        InstructionKind::ForeachInit { iterator, source } => {
            let _profile = vm.request_profile_operation_start(
                RequestProfileOperationCategory::Array,
                "foreach_init",
            );
            let source = match read_operand_at_frame(unit, stack, frame_index, *source) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let foreach_iterator = match vm.foreach_iterator_from_value(
                compiled,
                source,
                output,
                stack,
                state,
                ForeachInvalidSourceBehavior::WarnAndEmpty { span: Some(span) },
            ) {
                Ok(iterator) => iterator,
                Err(result) => {
                    match vm.route_throwable_result(
                        compiled,
                        output,
                        stack,
                        state,
                        exception_handlers,
                        pending_control,
                        result,
                    ) {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => {
                            return RichDispatchOutcome::Return(Box::new(*result));
                        }
                    }
                }
            };
            vm.record_runtime_trace_event(|| {
                format!(
                    "foreach init iterator=r{} kind={}",
                    iterator.raw(),
                    format_foreach_iterator_kind(&foreach_iterator)
                )
            });
            foreach_iterators.insert(*iterator, foreach_iterator);
        }
        InstructionKind::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            let _profile = vm.request_profile_operation_start(
                RequestProfileOperationCategory::Array,
                "foreach_next",
            );
            // Step the iterator in place; see `next_foreach_value`
            // for the rationale (avoids per-step deep clones of
            // the snapshot entries).
            let next_value = match foreach_iterators.get_mut(iterator) {
                Some(ForeachIterator::Snapshot { entries, position }) => {
                    let _source = layout_source::enter(layout_source::FOREACH_VALUE);
                    let next = entries
                        .get(*position)
                        .cloned()
                        .map(|(key, value)| (Some(array_key_to_value(key)), value));
                    if next.is_some() {
                        *position += 1;
                        vm.record_counter_value_clone_reason(layout_source::FOREACH_VALUE.name());
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
                        vm.record_counter_value_clone_reason(layout_source::FOREACH_VALUE.name());
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
                    if needs_next
                        && let Err(result) = vm.call_object_method_value(
                            compiled,
                            object.clone(),
                            "next",
                            output,
                            stack,
                            state,
                        )
                    {
                        match vm.route_throwable_result(
                            compiled,
                            output,
                            stack,
                            state,
                            exception_handlers,
                            pending_control,
                            result,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(Box::new(*result));
                            }
                        }
                    }
                    if matches!(
                        spl_runtime_marker(&object).as_deref(),
                        Some("recursiveiteratoriterator" | "recursivetreeiterator")
                    ) && spl_rii_should_call_valid_child_hook(&object)
                        && let Err(result) = vm.call_spl_rii_child_hook(
                            compiled,
                            &object,
                            "callHasChildren",
                            "RecursiveIteratorIterator->valid",
                            None,
                            output,
                            stack,
                            state,
                        )
                    {
                        match vm.route_throwable_result(
                            compiled,
                            output,
                            stack,
                            state,
                            exception_handlers,
                            pending_control,
                            result,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(Box::new(*result));
                            }
                        }
                    }
                    let valid = match vm.call_object_method_value(
                        compiled,
                        object.clone(),
                        "valid",
                        output,
                        stack,
                        state,
                    ) {
                        Ok(value) => match to_bool(&value) {
                            Ok(value) => value,
                            Err(message) => {
                                return RichDispatchOutcome::Return(Box::new(
                                    vm.runtime_error(output, compiled, stack, message),
                                ));
                            }
                        },
                        Err(result) => {
                            match vm.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
                                exception_handlers,
                                pending_control,
                                result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(Box::new(*result));
                                }
                            }
                        }
                    };
                    if !valid {
                        None
                    } else {
                        let entry_value = match vm.call_object_method_value(
                            compiled,
                            object.clone(),
                            "current",
                            output,
                            stack,
                            state,
                        ) {
                            Ok(value) => value,
                            Err(result) => {
                                match vm.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    exception_handlers,
                                    pending_control,
                                    result,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        return RichDispatchOutcome::Jump(target);
                                    }
                                    RaiseOutcome::Done(result) => {
                                        return RichDispatchOutcome::Return(Box::new(*result));
                                    }
                                }
                            }
                        };
                        let entry_key = if key.is_some() || always_call_key {
                            let key_value = match vm.call_object_method_value(
                                compiled,
                                object.clone(),
                                "key",
                                output,
                                stack,
                                state,
                            ) {
                                Ok(value) => value,
                                Err(result) => {
                                    match vm.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        exception_handlers,
                                        pending_control,
                                        result,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            return RichDispatchOutcome::Jump(target);
                                        }
                                        RaiseOutcome::Done(result) => {
                                            return RichDispatchOutcome::Return(Box::new(*result));
                                        }
                                    }
                                }
                            };
                            key.is_some().then_some(key_value)
                        } else {
                            None
                        };
                        if let Some(ForeachIterator::IteratorObject { needs_next, .. }) =
                            foreach_iterators.get_mut(iterator)
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
                        match vm.resume_generator_to_next_yield(
                            compiled,
                            generator,
                            GeneratorResumeInput::Value(Value::Null),
                            output,
                            stack,
                            state,
                        ) {
                            Ok(next) => next,
                            Err(result) => return RichDispatchOutcome::Return(Box::new(result)),
                        }
                    } else {
                        match vm.advance_generator_to_first_yield(
                            compiled, generator, output, stack, state,
                        ) {
                            Ok(next) => next,
                            Err(result) => return RichDispatchOutcome::Return(Box::new(result)),
                        }
                    }
                }
                Some(ForeachIterator::ByReference { .. }) | None => {
                    return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
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
            let Some((entry_key, entry_value)) = next_value else {
                vm.record_runtime_trace_event(|| {
                    format!("foreach next iterator=r{} status=done", iterator.raw())
                });
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*has_value, Value::Bool(false))
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            };
            vm.record_runtime_trace_event(|| {
                format!(
                    "foreach next iterator=r{} status=value key={} value={}",
                    iterator.raw(),
                    entry_key
                        .as_ref()
                        .map(trace_value)
                        .unwrap_or_else(|| "None".to_owned()),
                    trace_value(&entry_value)
                )
            });
            let frame = stack.frame_mut(frame_index).expect("frame is active");
            if let Err(message) = frame.registers.set(*has_value, Value::Bool(true)) {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            if let Some(key) = key
                && let Err(message) = frame
                    .registers
                    .set(*key, entry_key.unwrap_or(Value::Int(0)))
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            if let Err(message) = frame.registers.set(*value, entry_value) {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::ForeachCleanup { iterator } => {
            if let Some(value) = foreach_iterators
                .remove(iterator)
                .and_then(foreach_iterator_candidate_value)
                && let Some(outcome) = vm.run_destructors_for_unreferenced_value(
                    compiled,
                    output,
                    stack,
                    state,
                    exception_handlers,
                    pending_control,
                    &value,
                )
            {
                match outcome {
                    RaiseOutcome::Caught(target) => {
                        return RichDispatchOutcome::Jump(target);
                    }
                    RaiseOutcome::Done(result) => {
                        return RichDispatchOutcome::Return(Box::new(*result));
                    }
                }
            }
        }
        InstructionKind::ForeachInitRef { iterator, local } => {
            vm.record_counter_alias_state_transition(
                AliasState::NoReferencesObserved,
                AliasState::PropertyOrArrayDimReference,
            );
            vm.record_counter_fast_path_disabled_by_reference(
                AliasState::PropertyOrArrayDimReference,
            );
            vm.record_counter_local_slot_fast_path(local_slot_is_in_bounds(stack, *local));
            let source =
                read_local_value_at_frame(stack, frame_index, *local).unwrap_or(Value::Null);
            let effective_source = effective_value(&source);
            if matches!(
                effective_source,
                Value::Object(_) | Value::Generator(_) | Value::Resource(_)
            ) {
                match vm.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
                                exception_handlers,
                                pending_control,
                                span,
                                "E_PHP_VM_FOREACH_BY_REF_ITERATOR: An iterator cannot be used with foreach by reference"
                                    .to_owned(),
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(Box::new(*result)),
                            }
            }
            let Value::Array(_) = effective_source else {
                let diagnostic = unsupported_feature(
                    "E_PHP_VM_UNSUPPORTED_FOREACH_SOURCE",
                    format!(
                        "foreach by reference over {} is not implemented; runtime-semantics supports local arrays only",
                        value_type_name(&source)
                    ),
                    RuntimeSourceSpan::default(),
                    stack_trace(compiled, stack),
                );
                return RichDispatchOutcome::Return(Box::new(VmResult {
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
                }));
            };
            foreach_iterators.insert(
                *iterator,
                ForeachIterator::ByReference {
                    local: *local,
                    visited_keys: Vec::new(),
                },
            );
            vm.record_runtime_trace_event(|| {
                format!(
                    "foreach init-ref iterator=r{} local={}",
                    iterator.raw(),
                    local.raw()
                )
            });
        }
        InstructionKind::ForeachNextRef {
            has_value,
            iterator,
            key,
            value_local,
        } => {
            let Some(ForeachIterator::ByReference {
                local,
                visited_keys,
            }) = foreach_iterators.get(iterator).cloned()
            else {
                return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_FOREACH_ITERATOR_MISSING: iterator r{} is not initialized",
                        iterator.raw()
                    ),
                )));
            };
            let keys = match foreach_array_keys_from_local_at_frame(stack, frame_index, local) {
                Ok(keys) => keys,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let Some(entry_key) = keys
                .into_iter()
                .find(|candidate| !visited_keys.contains(candidate))
            else {
                vm.record_runtime_trace_event(|| {
                    format!("foreach next-ref iterator=r{} status=done", iterator.raw())
                });
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame was pushed")
                    .registers
                    .set(*has_value, Value::Bool(false))
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            };
            let Some(ForeachIterator::ByReference { visited_keys, .. }) =
                foreach_iterators.get_mut(iterator)
            else {
                return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_FOREACH_ITERATOR_MISSING: iterator r{} is not initialized",
                        iterator.raw()
                    ),
                )));
            };
            visited_keys.push(entry_key.clone());
            vm.record_runtime_trace_event(|| {
                format!(
                    "foreach next-ref iterator=r{} status=value key={}",
                    iterator.raw(),
                    format_array_key_for_trace(&entry_key)
                )
            });
            let cell =
                match ensure_dim_reference_cell(stack, local, std::slice::from_ref(&entry_key)) {
                    Ok(cell) => cell,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
            let frame = stack.frame_mut(frame_index).expect("frame was pushed");
            if let Err(message) = frame.registers.set(*has_value, Value::Bool(true)) {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            if let Some(key) = key
                && let Err(message) = frame.registers.set(*key, array_key_to_value(entry_key))
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            if let Err(message) = frame.locals.bind_reference_cell(*value_local, cell) {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            vm.record_counter_alias_state(AliasState::PropertyOrArrayDimReference);
        }
        _ => unreachable!("non-foreach instruction reached foreach dispatch"),
    }
    RichDispatchOutcome::Continue
}
