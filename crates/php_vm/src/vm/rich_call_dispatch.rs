use super::*;

pub(super) fn execute_rich_call_instruction(
    vm: &Vm,
    cursor: ExecutionCursor<'_>,
    site: dispatch_contract::RichInstructionSite<'_>,
    running_fiber: &Option<FiberRef>,
    diagnostics: &mut Vec<RuntimeDiagnostic>,
    foreach_iterators: &mut HashMap<RegId, ForeachIterator>,
    control: dispatch_contract::RichControlState<'_>,
) -> RichDispatchOutcome {
    let ExecutionCursor {
        compiled,
        output,
        stack,
        state,
    } = cursor;
    let dispatch_contract::RichInstructionSite {
        unit,
        function: _,
        function_id,
        block_id,
        instruction,
        instruction_index,
        frame_index,
    } = site;
    let dispatch_contract::RichControlState {
        exception_handlers,
        pending_control,
    } = control;

    match &instruction.kind {
        InstructionKind::CallFunction { dst, name, args } => {
            match vm.try_execute_rich_preg_match_start_offset_ascii_fast(
                name,
                args,
                unit,
                stack,
                frame_index,
                state,
            ) {
                Ok(Some(return_value)) => {
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, return_value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Ok(None) => {}
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            }
            let values =
                match read_call_args_for_function_at_frame(unit, stack, frame_index, name, args) {
                    Ok(values) => values,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
            let lowered_name = normalize_function_name(name);
            let interned_name = PhpString::intern(lowered_name.as_bytes());
            let epoch = state.lookup_epoch();
            let call_shape = function_call_shape(&values);
            let target = vm
                .lookup_function_call_inline_cache(
                    IrInlineCacheSite::classic(compiled, function_id, block_id, instruction.id),
                    &interned_name,
                    epoch,
                    &call_shape,
                )
                .or_else(|| {
                    let resolved =
                        vm.resolve_function_call_target(compiled, state, &lowered_name)?;
                    if vm.options.inline_caches.enabled()
                        && function_call_target_is_builtin(&resolved)
                    {
                        vm.record_counter_builtin_call_ic(false);
                    }
                    vm.install_function_call_inline_cache(
                        IrInlineCacheSite::classic(compiled, function_id, block_id, instruction.id),
                        &interned_name,
                        epoch,
                        call_shape.clone(),
                        resolved.clone(),
                    );
                    Some(resolved)
                });
            let Some(target) = target else {
                let diagnostic = undefined_function(
                    name,
                    RuntimeSourceSpan::default(),
                    stack_trace(compiled, stack),
                );
                let result = VmResult::runtime_error_with_diagnostic(
                    output.clone(),
                    diagnostic.message().to_owned(),
                    diagnostic,
                );
                if let Some(throwable) = runtime_error_throwable(&result) {
                    tag_throwable_location(&throwable, compiled, instruction.span);
                    state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                    if let Some(target) = handle_throw(
                        compiled,
                        throwable.clone(),
                        stack,
                        state,
                        exception_handlers,
                        pending_control,
                    ) {
                        return RichDispatchOutcome::Jump(target);
                    }
                    return RichDispatchOutcome::Return(Box::new(
                        vm.propagate_exception(output, stack, state, throwable),
                    ));
                }
                return RichDispatchOutcome::Return(Box::new(result));
            };
            let temporary_iterator_arg =
                iterator_function_temporary_arg_value(&lowered_name, args, &values);
            let result = vm.execute_function_call_target(
                ExecutionCursor::new(compiled, output, stack, state),
                target,
                values,
                Some((
                    compiled_unit_cache_key(compiled),
                    function_id,
                    block_id,
                    instruction.id,
                )),
                Some(instruction.span),
                running_fiber,
            );
            if !result.status.is_success() {
                let original_throwable = state
                    .pending_throw
                    .take()
                    .or_else(|| runtime_error_throwable(&result));
                if let Some((arg_operand, arg_value)) = temporary_iterator_arg.clone() {
                    if let Err(message) =
                        unset_register_operand_at_frame(stack, frame_index, arg_operand)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    let candidates = destructor_candidates_for_value(&arg_value);
                    let rooted_object_ids = php_visible_non_register_root_object_ids(stack, state);
                    let sweep = vm.run_destructors_for_unreferenced_candidates_with_roots(
                        ExecutionCursor::new(compiled, output, stack, state),
                        exception_handlers,
                        pending_control,
                        candidates,
                        &rooted_object_ids,
                        None,
                    );
                    if let Some(outcome) = sweep.outcome {
                        match outcome {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                }
                if let Some(throwable) = original_throwable {
                    state.pending_throw = Some(throwable);
                }
            }
            if (!result.status.is_success() || state.pending_throw.is_some())
                && let Some(throwable) = state
                    .pending_throw
                    .take()
                    .or_else(|| runtime_error_throwable(&result))
            {
                if let Some(target) = handle_throw(
                    compiled,
                    throwable.clone(),
                    stack,
                    state,
                    exception_handlers,
                    pending_control,
                ) {
                    return RichDispatchOutcome::Jump(target);
                }
                return RichDispatchOutcome::Return(Box::new(
                    vm.propagate_exception(output, stack, state, throwable),
                ));
            }
            if !result.status.is_success() || state.pending_throw.is_some() {
                return RichDispatchOutcome::Return(Box::new(result));
            }
            if result.fiber_suspension.is_some() {
                return RichDispatchOutcome::Return(Box::new(vm.propagate_fiber_suspension(
                    result,
                    compiled,
                    FiberContinuationState::new(
                        *dst,
                        block_id,
                        instruction_index + 1,
                        foreach_iterators,
                        exception_handlers,
                        pending_control,
                    ),
                    output,
                    stack,
                )));
            }
            diagnostics.extend(result.diagnostics);
            let return_value = result.return_value.unwrap_or(Value::Null);
            if let Some((arg_operand, arg_value)) = temporary_iterator_arg {
                if let Err(message) =
                    unset_register_operand_at_frame(stack, frame_index, arg_operand)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                let mut rooted_object_ids = php_visible_non_register_root_object_ids(stack, state);
                rooted_object_ids.extend(preserved_destructor_object_ids(std::slice::from_ref(
                    &return_value,
                )));
                let candidates = destructor_candidates_for_value(&arg_value);
                let sweep = vm.run_destructors_for_unreferenced_candidates_with_roots(
                    ExecutionCursor::new(compiled, output, stack, state),
                    exception_handlers,
                    pending_control,
                    candidates,
                    &rooted_object_ids,
                    None,
                );
                if let Some(outcome) = sweep.outcome {
                    match outcome {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                    }
                }
            }
            if let Err(message) =
                unset_consumed_call_arg_registers_at_frame(stack, frame_index, args, Some(*dst))
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame is active")
                .registers
                .set(*dst, return_value)
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::CallMethod {
            dst,
            object,
            method,
            args,
        } => {
            let object_operand = *object;
            let receiver = match read_operand_at_frame(unit, stack, frame_index, *object) {
                Ok(receiver) => receiver,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                Ok(values) => values,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let object = match receiver {
                Value::Fiber(fiber) => {
                    let value = match vm.call_fiber_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        fiber,
                        method,
                        values,
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            if let Some(throwable) = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result))
                            {
                                tag_throwable_location(&throwable, compiled, instruction.span);
                                state.pending_trace =
                                    Some(capture_backtrace_string(compiled, stack));
                                if let Some(target) = handle_throw(
                                    compiled,
                                    throwable.clone(),
                                    stack,
                                    state,
                                    exception_handlers,
                                    pending_control,
                                ) {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                return RichDispatchOutcome::Return(Box::new(
                                    vm.propagate_exception(output, stack, state, throwable),
                                ));
                            }
                            return RichDispatchOutcome::Return(Box::new(*result));
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Value::Generator(generator) => {
                    let value = match vm.call_generator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        generator,
                        method,
                        values,
                    ) {
                        Ok(value) => value,
                        Err(result) => return RichDispatchOutcome::Return(Box::new(*result)),
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Value::Callable(callable) if method.eq_ignore_ascii_case("__invoke") => {
                    let value = match vm
                        .call_callable_inner(
                            ExecutionCursor::new(compiled, output, stack, state),
                            Value::Callable(callable),
                            values,
                            Some(instruction.span),
                            false,
                            None,
                        )
                        .return_value
                    {
                        Some(value) => value,
                        None => Value::Null,
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Value::Callable(callable) if method.eq_ignore_ascii_case("call") => {
                    let result = vm.call_closure_call_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        *callable,
                        values,
                        instruction.span,
                    );
                    if !result.status.is_success()
                        && let Some(throwable) = state
                            .pending_throw
                            .take()
                            .or_else(|| runtime_error_throwable(&result))
                    {
                        if let Some(target) = handle_throw(
                            compiled,
                            throwable.clone(),
                            stack,
                            state,
                            exception_handlers,
                            pending_control,
                        ) {
                            return RichDispatchOutcome::Jump(target);
                        }
                        return RichDispatchOutcome::Return(Box::new(
                            vm.propagate_exception(output, stack, state, throwable),
                        ));
                    }
                    if !result.status.is_success() {
                        return RichDispatchOutcome::Return(Box::new(result));
                    }
                    let return_value = result.return_value.unwrap_or(Value::Null);
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, return_value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Value::Callable(callable) if method.eq_ignore_ascii_case("bindto") => {
                    let result = vm.call_closure_bind_to_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        *callable,
                        values,
                        instruction.span,
                    );
                    if !result.status.is_success()
                        && let Some(throwable) = state
                            .pending_throw
                            .take()
                            .or_else(|| runtime_error_throwable(&result))
                    {
                        if let Some(target) = handle_throw(
                            compiled,
                            throwable.clone(),
                            stack,
                            state,
                            exception_handlers,
                            pending_control,
                        ) {
                            return RichDispatchOutcome::Jump(target);
                        }
                        return RichDispatchOutcome::Return(Box::new(
                            vm.propagate_exception(output, stack, state, throwable),
                        ));
                    }
                    if !result.status.is_success() {
                        return RichDispatchOutcome::Return(Box::new(result));
                    }
                    let return_value = result.return_value.unwrap_or(Value::Null);
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, return_value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Value::Object(object) => object,
                other => {
                    let message = format!(
                        "E_PHP_VM_METHOD_CALL_NON_OBJECT: Call to a member function {method}() on {}",
                        value_type_name(&other)
                    );
                    match vm.raise_runtime_error(
                        ExecutionCursor::new(compiled, output, stack, state),
                        exception_handlers,
                        pending_control,
                        instruction.span,
                        message,
                    ) {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                    }
                }
            };
            if internal_throwable_instanceof(&object.class_name_handle(), "throwable").is_some() {
                let value = match internal_throwable_method_value(
                    &object,
                    method,
                    values.into_iter().map(|arg| arg.value).collect(),
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_reflection_runtime_class(&object.class_name()) {
                let values = values.into_iter().map(|arg| arg.value).collect::<Vec<_>>();
                if normalize_class_name(&object.class_name()) == "reflectionclass"
                    && matches!(
                        normalize_method_name(method).as_str(),
                        "newinstance" | "newinstanceargs"
                    )
                {
                    let result = vm.reflection_class_new_instance(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        values,
                        instruction.span,
                    );
                    if !result.status.is_success() {
                        match vm.route_throwable_result(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            result,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                    if result.fiber_suspension.is_some() {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.propagate_fiber_suspension(
                                result,
                                compiled,
                                FiberContinuationState::new(
                                    *dst,
                                    block_id,
                                    instruction_index + 1,
                                    foreach_iterators,
                                    exception_handlers,
                                    pending_control,
                                ),
                                output,
                                stack,
                            ),
                        ));
                    }
                    diagnostics.extend(result.diagnostics);
                    let value = result.return_value.unwrap_or(Value::Null);
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if normalize_class_name(&object.class_name()) == "reflectionattribute"
                    && normalize_method_name(method) == "newinstance"
                {
                    let result = vm.reflection_attribute_new_instance(
                        compiled,
                        &object,
                        output,
                        stack,
                        state,
                        instruction.span,
                    );
                    if !result.status.is_success() {
                        match vm.route_throwable_result(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            result,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                    if result.fiber_suspension.is_some() {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.propagate_fiber_suspension(
                                result,
                                compiled,
                                FiberContinuationState::new(
                                    *dst,
                                    block_id,
                                    instruction_index + 1,
                                    foreach_iterators,
                                    exception_handlers,
                                    pending_control,
                                ),
                                output,
                                stack,
                            ),
                        ));
                    }
                    diagnostics.extend(result.diagnostics);
                    let value = result.return_value.unwrap_or(Value::Null);
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if let Err(result) = vm.preflight_reflection_class_method(
                    ExecutionCursor::new(compiled, output, stack, state),
                    &object,
                    method,
                    &values,
                ) {
                    match vm.route_throwable_result(
                        ExecutionCursor::new(compiled, output, stack, state),
                        exception_handlers,
                        pending_control,
                        *result,
                    ) {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                    }
                }
                let value = match reflection_method_value(
                    ExecutionCursor::new(compiled, output, stack, state),
                    &object,
                    method,
                    values,
                    runtime_source_span(compiled, instruction.span),
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        let result = vm.runtime_error(output, compiled, stack, message);
                        match vm.route_throwable_result(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            result,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_php_token_runtime_class(&object.class_name()) {
                let values = values.into_iter().map(|arg| arg.value).collect::<Vec<_>>();
                let value = match php_token_method_value(&object, method, values) {
                    Ok(value) => value,
                    Err(message) => {
                        let result = vm.runtime_error(output, compiled, stack, message);
                        match vm.route_throwable_result(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            result,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if spl_runtime_marker(&object).is_some_and(|class| {
                is_spl_file_runtime_class(&class) && spl_file_method_is_supported(method)
            }) && normalize_method_name(method) == "fpassthru"
                && !spl_file_is_initialized(&object)
            {
                let value = match call_spl_file_method_in_state(
                    compiled,
                    state,
                    &object,
                    method,
                    values,
                    &vm.options.runtime_context,
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if let Some(spl_class) = spl_runtime_marker(&object) {
                let object_display_class = object.display_name();
                let object_class = normalize_class_name(&object_display_class);
                if object_class != spl_class {
                    let scope = current_scope_class(compiled, stack);
                    match lookup_resolved_method_in_state(
                        compiled,
                        state,
                        &object_display_class,
                        method,
                        scope.as_deref(),
                    ) {
                        Ok(Some(resolved))
                            if internal_runtime_class_entry(&normalize_class_name(
                                &resolved.class.name,
                            ))
                            .is_none() =>
                        {
                            if let Err(message) = validate_method_callable_in_state_scope(
                                compiled,
                                state,
                                scope.as_deref(),
                                &resolved.class,
                                &resolved.method,
                            ) {
                                return RichDispatchOutcome::Return(Box::new(
                                    vm.runtime_error(output, compiled, stack, message),
                                ));
                            }
                            let class_owner =
                                class_owner_in_state(compiled, state, &resolved.class.name);
                            let result = vm.execute_function(
                                &class_owner,
                                resolved.method.function,
                                FunctionCall::new(values, Vec::new())
                                    .with_call_site_strict_types(compiled.unit().strict_types)
                                    .with_call_span(instruction.span)
                                    .with_this(object.clone())
                                    .with_class_context_handles(
                                        vm.class_name_handles(&resolved.class.name).normalized,
                                        object_called_class_handle(&object),
                                        vm.class_name_handles(&resolved.class.name).normalized,
                                    ),
                                output,
                                stack,
                                state,
                            );
                            if !result.status.is_success() {
                                match vm.route_throwable_result(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    exception_handlers,
                                    pending_control,
                                    result,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        return RichDispatchOutcome::Jump(target);
                                    }
                                    RaiseOutcome::Done(result) => {
                                        return RichDispatchOutcome::Return(result);
                                    }
                                }
                            }
                            diagnostics.extend(result.diagnostics);
                            let return_value = result.return_value.unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, return_value)
                            {
                                return RichDispatchOutcome::Return(Box::new(
                                    vm.runtime_error(output, compiled, stack, message),
                                ));
                            }
                            return RichDispatchOutcome::Continue;
                        }
                        Ok(_) => {}
                        Err(message) => {
                            return RichDispatchOutcome::Return(Box::new(
                                vm.runtime_error(output, compiled, stack, message),
                            ));
                        }
                    }
                }
            }
            if spl_runtime_marker(&object).is_some_and(|class| {
                is_spl_iterator_runtime_class(&class) && spl_iterator_method_is_supported(method)
            }) {
                if spl_runtime_marker(&object).as_deref() == Some("appenditerator")
                    && matches!(
                        normalize_method_name(method).as_str(),
                        "append" | "rewind" | "next"
                    )
                {
                    let value = match vm.call_spl_append_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        values,
                        Some(instruction.span),
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if spl_runtime_marker(&object).as_deref() == Some("limititerator")
                    && spl_limit_iterator_uses_live_inner(&object)
                    && matches!(
                        normalize_method_name(method).as_str(),
                        "rewind" | "valid" | "current" | "key" | "next" | "seek" | "getposition"
                    )
                {
                    let value = match vm.call_spl_limit_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        values,
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if spl_runtime_marker(&object)
                    .is_some_and(|class| is_spl_caching_iterator_class(&class))
                    && spl_caching_iterator_uses_live_inner(&object)
                    && matches!(
                        normalize_method_name(method).as_str(),
                        "rewind" | "valid" | "current" | "key" | "next"
                    )
                {
                    let value = match vm.call_spl_caching_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        values,
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if spl_runtime_marker(&object).as_deref() == Some("norewinditerator")
                    && matches!(
                        normalize_method_name(method).as_str(),
                        "rewind" | "valid" | "current" | "key" | "next"
                    )
                {
                    let value = match vm.call_spl_no_rewind_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        values,
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if spl_runtime_marker(&object)
                    .is_some_and(|class| is_spl_caching_iterator_class(&class))
                    && normalize_method_name(method) == "__tostring"
                {
                    if let Err(message) =
                        validate_spl_iterator_arg_count(&object.class_name(), &values, 0, 0)
                    {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                    let value = match vm.spl_caching_iterator_to_string(
                        compiled,
                        &object,
                        runtime_source_span(compiled, instruction.span),
                        output,
                        stack,
                        state,
                    ) {
                        Ok(value) => Value::String(value),
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if spl_runtime_marker(&object)
                    .is_some_and(|class| is_spl_caching_iterator_class(&class))
                    && matches!(
                        normalize_method_name(method).as_str(),
                        "offsetget" | "offsetexists"
                    )
                {
                    let result = vm.call_spl_caching_iterator_offset_access_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        values,
                        Some(instruction.span),
                    );
                    if !result.status.is_success() || state.pending_throw.is_some() {
                        let cleanup_values = match take_method_call_temporary_registers_at_frame(
                            stack,
                            frame_index,
                            object_operand,
                            args,
                        ) {
                            Ok(values) => values,
                            Err(message) => {
                                return RichDispatchOutcome::Return(Box::new(
                                    vm.runtime_error(output, compiled, stack, message),
                                ));
                            }
                        };
                        for value in &cleanup_values {
                            release_unrooted_object_handles(value);
                        }
                        match vm.route_throwable_result(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            result,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                    diagnostics.extend(result.diagnostics);
                    let return_value = result.return_value.unwrap_or(Value::Null);
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, return_value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    let cleanup_values = match take_method_call_temporary_registers_at_frame(
                        stack,
                        frame_index,
                        object_operand,
                        args,
                    ) {
                        Ok(values) => values,
                        Err(message) => {
                            return RichDispatchOutcome::Return(Box::new(
                                vm.runtime_error(output, compiled, stack, message),
                            ));
                        }
                    };
                    for value in &cleanup_values {
                        release_unrooted_object_handles(value);
                    }
                    return RichDispatchOutcome::Continue;
                }
                let value = if spl_runtime_marker(&object).as_deref() == Some("multipleiterator")
                    && matches!(
                        normalize_method_name(method).as_str(),
                        "rewind" | "valid" | "current" | "key" | "next"
                    ) {
                    match vm.call_spl_multiple_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        values,
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    }
                } else if matches!(
                    spl_runtime_marker(&object).as_deref(),
                    Some("recursiveiteratoriterator" | "recursivetreeiterator")
                ) && matches!(
                    normalize_method_name(method).as_str(),
                    "rewind" | "valid" | "current" | "next"
                ) {
                    match vm.call_spl_recursive_iterator_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        object,
                        method,
                        values,
                        Some(instruction.span),
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    }
                } else {
                    match call_spl_iterator_method(
                        object,
                        method,
                        values,
                        &vm.options.runtime_context,
                    ) {
                        Ok(value) => value,
                        Err(message) => {
                            match vm.raise_runtime_error(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if spl_runtime_marker(&object).is_some_and(|class| {
                is_spl_container_runtime_class(&class) && spl_container_method_is_supported(method)
            }) {
                let value = match vm.call_spl_container_method_with_magic(
                    ExecutionCursor::new(compiled, output, stack, state),
                    object,
                    method,
                    values,
                    Some(instruction.span),
                ) {
                    Ok(value) => value,
                    Err(result) => return RichDispatchOutcome::Return(Box::new(*result)),
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if spl_runtime_marker(&object).is_some_and(|class| {
                is_spl_heap_runtime_class(&class) && spl_heap_method_is_supported(method)
            }) {
                let value = match vm.call_spl_heap_method(
                    ExecutionCursor::new(compiled, output, stack, state),
                    object,
                    method,
                    values,
                ) {
                    Ok(value) => value,
                    Err(SplHeapMethodError::Message(message)) => {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                    Err(SplHeapMethodError::Runtime(result)) => {
                        match vm.route_throwable_result(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            *result,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_hash_context_runtime_class(&object.class_name())
                && hash_context_method_is_supported(method)
            {
                let value = match vm.call_hash_context_method(&object, method, &values) {
                    Ok(value) => value,
                    Err(message) => {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if spl_runtime_marker(&object).is_some_and(|class| {
                is_spl_file_runtime_class(&class) && spl_file_method_is_supported(method)
            }) {
                let value = match call_spl_file_method_in_state(
                    compiled,
                    state,
                    &object,
                    method,
                    values,
                    &vm.options.runtime_context,
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_date_time_runtime_class(&object.class_name()) {
                let value = match call_date_time_method(object, method, values) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_sqlite_runtime_class(&object.class_name()) {
                let value = match call_sqlite_method(
                    &object,
                    method,
                    values,
                    &mut state.builtins.sqlite,
                    &vm.options.runtime_context,
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_pdo_runtime_class(&object.class_name()) {
                let value = match call_pdo_method(
                    &object,
                    method,
                    values,
                    &mut state.builtins.sqlite,
                    &mut state.builtins.mysql,
                    &mut state.builtins.postgres,
                    &vm.options.runtime_context,
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_phar_runtime_class(&object.class_name()) {
                let value =
                    match call_phar_method(&object, method, values, &vm.options.runtime_context) {
                        Ok(value) => value,
                        Err(message) => {
                            return RichDispatchOutcome::Return(Box::new(
                                vm.runtime_error(output, compiled, stack, message),
                            ));
                        }
                    };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_zip_runtime_class(&object.class_name()) {
                if zip_open_uses_empty_file(method, &values, &vm.options.runtime_context) {
                    emit_zip_open_empty_file_deprecation(
                        compiled,
                        output,
                        stack,
                        state,
                        runtime_source_span(compiled, instruction.span),
                    );
                }
                let value =
                    match call_zip_method(&object, method, values, &vm.options.runtime_context) {
                        Ok(value) => value,
                        Err(message) => {
                            match vm.raise_runtime_error(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_mysqli_runtime_class(&object.class_name()) {
                let value = match call_mysqli_method(
                    &object,
                    method,
                    values,
                    &mut state.builtins.mysql,
                    compiled,
                    stack,
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_redis_runtime_class(&object.class_name()) {
                let value = match call_redis_method(
                    &object,
                    method,
                    values,
                    &mut state.builtins.redis_clients,
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_memcached_runtime_class(&object.class_name()) {
                let value = match call_memcached_method(
                    &object,
                    method,
                    values,
                    &mut state.builtins.memcached_clients,
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_soap_runtime_class(&object.class_name()) {
                let value = match call_soap_method(&object, method, values) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_fileinfo_runtime_class(&object.class_name()) {
                let result = FileinfoMethodCall {
                    vm,
                    compiled,
                    object,
                    method,
                    call_span: Some(instruction.span),
                    output,
                    stack,
                    state,
                }
                .execute(values);
                if !result.status.is_success() {
                    return RichDispatchOutcome::Return(Box::new(result));
                }
                let value = result.return_value.unwrap_or(Value::Null);
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_imagick_runtime_class(&object.class_name()) {
                let value = match call_imagick_method(&object, method, values) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_xsl_runtime_class(&object.class_name()) {
                let value = match call_xsl_method(&object, method, values) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_xml_runtime_class(&object.class_name()) {
                let value = match call_xml_runtime_method(
                    &object,
                    method,
                    values.into_iter().map(|arg| arg.value).collect(),
                    &vm.options.runtime_context,
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            let receiver_class = normalize_class_name(&object.class_name());
            let lowered_method = normalize_method_name(method);
            let scope = current_scope_class(compiled, stack);
            let epoch = state.lookup_epoch();
            let method_callsite =
                method_call_callsite(compiled, function_id, block_id, instruction.id);
            let class = match lookup_class_in_state(compiled, state, &object.class_name()) {
                Some(class) => class,
                None => {
                    return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
                            object.class_name()
                        ),
                    )));
                }
            };
            let receiver_class_owner = class_owner_in_state(compiled, state, &class.name);
            let has_magic_call = class_has_public_magic_call(&receiver_class_owner, &class);
            let (cached_target, cache_observation) = vm.lookup_method_call_inline_cache(
                IrInlineCacheSite::classic(compiled, function_id, block_id, instruction.id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
            );
            if let Some(target) = cached_target {
                if !matches!(
                    vm.options.native_optimization,
                    NativeOptimizationPolicy::Optimizing
                ) || method_direct_call_target_is_eligible(MethodDirectCallEligibility {
                    compiled,
                    state,
                    target: &target,
                    class: &class,
                    args,
                    values: &values,
                    has_magic_call,
                    epoch,
                }) {
                    vm.record_counter_method_direct_dispatch_hit();
                    if matches!(
                        vm.options.native_optimization,
                        NativeOptimizationPolicy::Optimizing
                    ) {
                        vm.record_counter_direct_call_hit();
                    }
                    let result = vm.execute_method_call_target(
                        compiled,
                        target,
                        object.clone(),
                        values,
                        Some(instruction.span),
                        output,
                        stack,
                        state,
                        running_fiber,
                        None,
                    );
                    if !result.status.is_success()
                        && let Some(throwable) = state
                            .pending_throw
                            .take()
                            .or_else(|| runtime_error_throwable(&result))
                    {
                        if let Some(target) = handle_throw(
                            compiled,
                            throwable.clone(),
                            stack,
                            state,
                            exception_handlers,
                            pending_control,
                        ) {
                            return RichDispatchOutcome::Jump(target);
                        }
                        return RichDispatchOutcome::Return(Box::new(
                            vm.propagate_exception(output, stack, state, throwable),
                        ));
                    }
                    if !result.status.is_success() {
                        return RichDispatchOutcome::Return(Box::new(result));
                    }
                    if result.fiber_suspension.is_some() {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.propagate_fiber_suspension(
                                result,
                                compiled,
                                FiberContinuationState::new(
                                    *dst,
                                    block_id,
                                    instruction_index + 1,
                                    foreach_iterators,
                                    exception_handlers,
                                    pending_control,
                                ),
                                output,
                                stack,
                            ),
                        ));
                    }
                    diagnostics.extend(result.diagnostics);
                    let return_value = result.return_value.unwrap_or(Value::Null);
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, return_value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                vm.record_counter_method_direct_dispatch_fallback();
                if matches!(
                    vm.options.native_optimization,
                    NativeOptimizationPolicy::Optimizing
                ) {
                    vm.record_counter_direct_call_fallback();
                }
            } else if let Some(observation) = cache_observation {
                if observation.fallback_call {
                    vm.record_counter_method_direct_dispatch_fallback();
                }
                if matches!(
                    vm.options.native_optimization,
                    NativeOptimizationPolicy::Optimizing
                ) {
                    vm.record_counter_direct_call_fallback();
                }
            }
            let resolved = match lookup_resolved_method_in_state(
                compiled,
                state,
                &class.name,
                method,
                scope.as_deref(),
            ) {
                Ok(Some(method)) => method,
                Ok(None) => {
                    vm.record_counter_method_call_profile(method_call_profile_observation(
                        &method_callsite,
                        &lowered_method,
                        &receiver_class,
                        &class,
                        None,
                        scope.as_deref(),
                        epoch,
                        class_has_public_magic_call(&receiver_class_owner, &class),
                        true,
                        method_call_args_are_simple_positional(args),
                        method_call_has_by_ref_argument(args, None, None),
                        false,
                        false,
                        vec!["missing_declared_method"],
                    ));
                    if let Some(inner) = spl_inner_iterator_delegation_target(&object)
                        && (spl_delegation_target_supports_method(compiled, state, &inner, method)
                            || match vm.spl_iterator_chain_has_userland_method(
                                compiled, state, &inner, method,
                            ) {
                                Ok(result) => result,
                                Err(message) => {
                                    return RichDispatchOutcome::Return(Box::new(
                                        vm.runtime_error(output, compiled, stack, message),
                                    ));
                                }
                            })
                    {
                        let result = vm.call_object_method_callable(
                            ExecutionCursor::new(compiled, output, stack, state),
                            inner,
                            method,
                            values,
                            Some(instruction.span),
                        );
                        if !result.status.is_success() {
                            return RichDispatchOutcome::Return(Box::new(result));
                        }
                        diagnostics.extend(result.diagnostics);
                        let return_value = result.return_value.unwrap_or(Value::Null);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, return_value)
                        {
                            return RichDispatchOutcome::Return(Box::new(
                                vm.runtime_error(output, compiled, stack, message),
                            ));
                        }
                        return RichDispatchOutcome::Continue;
                    }
                    if class_is_or_extends_internal_throwable_in_state(compiled, state, &class.name)
                        .unwrap_or(false)
                    {
                        let value = match internal_throwable_method_value(
                            &object,
                            method,
                            values.into_iter().map(|arg| arg.value).collect(),
                        ) {
                            Ok(value) => value,
                            Err(message) => {
                                return RichDispatchOutcome::Return(Box::new(
                                    vm.runtime_error(output, compiled, stack, message),
                                ));
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, value)
                        {
                            return RichDispatchOutcome::Return(Box::new(
                                vm.runtime_error(output, compiled, stack, message),
                            ));
                        }
                        return RichDispatchOutcome::Continue;
                    }
                    let result = match vm.call_magic_instance_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        object.clone(),
                        "__call",
                        method,
                        values,
                        Some(instruction.span),
                    ) {
                        Ok(Some(result)) => result,
                        Ok(None) => {
                            let message = format!(
                                "E_PHP_VM_UNKNOWN_METHOD: Call to undefined method {}::{}()",
                                object.display_name(),
                                method
                            );
                            match vm.raise_runtime_error(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                        Err(result) => return RichDispatchOutcome::Return(Box::new(*result)),
                    };
                    if !result.status.is_success() {
                        return RichDispatchOutcome::Return(Box::new(result));
                    }
                    diagnostics.extend(result.diagnostics);
                    let return_value = result.return_value.unwrap_or(Value::Null);
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, return_value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let method_entry = &resolved.method;
            let declaring_class = &resolved.class;
            let class_owner = class_owner_in_state(compiled, state, &declaring_class.name);
            let simple_positional_arguments = method_call_args_are_simple_positional(args);
            let has_by_ref_argument = method_call_has_by_ref_argument(
                args,
                Some(&class_owner),
                Some(method_entry.function),
            );
            let callee_jit_eligible =
                method_callee_shape_is_jit_eligible(&class_owner, method_entry.function);
            // PHP permits calling a static method through an instance
            // (`$obj->staticMethod()`); it runs as a static call. Fall
            // through to the normal dispatch — a static body never uses
            // `$this`, so the bound receiver is inert.
            vm.record_counter_method_call_profile(method_call_profile_observation(
                &method_callsite,
                &lowered_method,
                &receiver_class,
                &class,
                Some((&class_owner, declaring_class, method_entry)),
                scope.as_deref(),
                epoch,
                has_magic_call,
                false,
                simple_positional_arguments,
                has_by_ref_argument,
                callee_jit_eligible,
                false,
                Vec::new(),
            ));
            if let Some(reason) = method_tiny_inline_rejection_reason(
                &class_owner,
                declaring_class,
                method_entry,
                args,
                has_magic_call,
            ) {
                vm.record_counter_method_tiny_inline_rejection(reason);
            } else {
                vm.record_counter_method_tiny_inline_candidate();
            }
            if let Err(message) = validate_method_callable_in_state_scope(
                compiled,
                state,
                scope.as_deref(),
                declaring_class,
                method_entry,
            ) {
                if method_entry.flags.is_private || method_entry.flags.is_protected {
                    let result = match vm.call_magic_instance_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        object.clone(),
                        "__call",
                        method,
                        values,
                        Some(instruction.span),
                    ) {
                        Ok(Some(result)) => result,
                        Ok(None) => {
                            let result = vm.runtime_error(output, compiled, stack, message);
                            if let Some(throwable) = runtime_error_throwable(&result) {
                                tag_throwable_location(&throwable, compiled, instruction.span);
                                state.pending_trace =
                                    Some(capture_backtrace_string(compiled, stack));
                                if let Some(target) = handle_throw(
                                    compiled,
                                    throwable.clone(),
                                    stack,
                                    state,
                                    exception_handlers,
                                    pending_control,
                                ) {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                return RichDispatchOutcome::Return(Box::new(
                                    vm.propagate_exception(output, stack, state, throwable),
                                ));
                            }
                            return RichDispatchOutcome::Return(Box::new(result));
                        }
                        Err(result) => return RichDispatchOutcome::Return(Box::new(*result)),
                    };
                    if !result.status.is_success() {
                        return RichDispatchOutcome::Return(Box::new(result));
                    }
                    diagnostics.extend(result.diagnostics);
                    let return_value = result.return_value.unwrap_or(Value::Null);
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, return_value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                let result = vm.runtime_error(output, compiled, stack, message);
                if let Some(throwable) = runtime_error_throwable(&result) {
                    tag_throwable_location(&throwable, compiled, instruction.span);
                    state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                    if let Some(target) = handle_throw(
                        compiled,
                        throwable.clone(),
                        stack,
                        state,
                        exception_handlers,
                        pending_control,
                    ) {
                        return RichDispatchOutcome::Jump(target);
                    }
                    return RichDispatchOutcome::Return(Box::new(
                        vm.propagate_exception(output, stack, state, throwable),
                    ));
                }
                return RichDispatchOutcome::Return(Box::new(result));
            }
            let declaring_dynamic_owner_index =
                dynamic_class_owner_index_in_state(state, &declaring_class.name);
            let method_guard = method_call_guard_metadata(
                &values,
                &class,
                declaring_class,
                method_entry,
                scope.as_deref(),
                epoch,
                has_magic_call,
                has_by_ref_argument,
            );
            let method_target = Rc::new(MethodCallResolvedTarget {
                declaring_class: declaring_class.name.clone(),
                function: method_entry.function,
                guard: method_guard,
                // The rich arm lacks the owner plan here; hits
                // keep the legacy re-resolving path.
                route: None,
            });
            let target = match declaring_dynamic_owner_index {
                Some(unit_index) => MethodCallCacheTarget::DynamicUnit {
                    unit_index,
                    target: method_target.clone(),
                },
                None => MethodCallCacheTarget::CurrentUnit {
                    target: method_target,
                },
            };
            let direct_call_cacheable = simple_positional_arguments
                && !has_by_ref_argument
                && !has_magic_call
                && !method_entry.flags.is_static;
            if vm.options.inline_caches.enabled()
                || !matches!(
                    vm.options.native_optimization,
                    NativeOptimizationPolicy::Optimizing
                )
                || direct_call_cacheable
            {
                vm.install_method_call_inline_cache(
                    IrInlineCacheSite::classic(compiled, function_id, block_id, instruction.id),
                    &lowered_method,
                    &receiver_class,
                    scope.as_deref(),
                    epoch,
                    target,
                );
            }
            let result = vm.execute_function(
                &class_owner,
                method_entry.function,
                FunctionCall::new(values, Vec::new())
                    .with_call_site_strict_types(call_site_strictness(
                        compiled,
                        Some(instruction.span),
                    ))
                    .with_call_span(instruction.span)
                    .with_this(object.clone())
                    .with_class_context_handles(
                        vm.class_name_handles(&declaring_class.name).normalized,
                        vm.class_name_handles(&class.display_name).display,
                        vm.class_name_handles(&declaring_class.name).normalized,
                    )
                    .inherit_fiber_context(running_fiber),
                output,
                stack,
                state,
            );
            if (!result.status.is_success() || state.pending_throw.is_some())
                && let Some(throwable) = state
                    .pending_throw
                    .take()
                    .or_else(|| runtime_error_throwable(&result))
            {
                if let Some(target) = handle_throw(
                    compiled,
                    throwable.clone(),
                    stack,
                    state,
                    exception_handlers,
                    pending_control,
                ) {
                    return RichDispatchOutcome::Jump(target);
                }
                return RichDispatchOutcome::Return(Box::new(
                    vm.propagate_exception(output, stack, state, throwable),
                ));
            }
            if !result.status.is_success() {
                return RichDispatchOutcome::Return(Box::new(result));
            }
            if result.fiber_suspension.is_some() {
                return RichDispatchOutcome::Return(Box::new(vm.propagate_fiber_suspension(
                    result,
                    compiled,
                    FiberContinuationState::new(
                        *dst,
                        block_id,
                        instruction_index + 1,
                        foreach_iterators,
                        exception_handlers,
                        pending_control,
                    ),
                    output,
                    stack,
                )));
            }
            diagnostics.extend(result.diagnostics);
            let return_value = result.return_value.unwrap_or(Value::Null);
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame is active")
                .registers
                .set(*dst, return_value)
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::CallStaticMethod {
            dst,
            class_name,
            method,
            args,
        } => {
            if is_closure_runtime_class(class_name) {
                let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                    Ok(values) => values,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                let value = match closure_static_method_value(
                    compiled,
                    state,
                    stack,
                    method,
                    values,
                    output,
                    runtime_source_span(compiled, instruction.span),
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_fiber_runtime_class(class_name) && normalize_method_name(method) == "suspend" {
                let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                    Ok(values) => values,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                match vm.suspend_current_fiber(
                    compiled,
                    running_fiber,
                    values,
                    FiberContinuationState::new(
                        *dst,
                        block_id,
                        instruction_index + 1,
                        foreach_iterators,
                        exception_handlers,
                        pending_control,
                    ),
                    output,
                    stack,
                ) {
                    Ok(result) => return RichDispatchOutcome::Return(Box::new(result)),
                    Err(result) => {
                        if let Some(throwable) = state
                            .pending_throw
                            .take()
                            .or_else(|| runtime_error_throwable(&result))
                        {
                            tag_throwable_location(&throwable, compiled, instruction.span);
                            state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                            if let Some(target) = handle_throw(
                                compiled,
                                throwable.clone(),
                                stack,
                                state,
                                exception_handlers,
                                pending_control,
                            ) {
                                return RichDispatchOutcome::Jump(target);
                            }
                            return RichDispatchOutcome::Return(Box::new(
                                vm.propagate_exception(output, stack, state, throwable),
                            ));
                        }
                        return RichDispatchOutcome::Return(Box::new(*result));
                    }
                }
            }
            if is_php_token_runtime_class(class_name) {
                let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                    Ok(values) => values,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                let trace_values = values
                    .iter()
                    .map(|arg| arg.value.clone())
                    .collect::<Vec<_>>();
                let result = match php_token_static_method_value_with_diagnostics(
                    class_name, method, values,
                ) {
                    Ok(result) => result,
                    Err(message) => {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                let trace_context = TokenizerStaticCallTraceContext {
                    call: format!("{class_name}::{method}"),
                    values: trace_values,
                    call_span: instruction.span,
                };
                if let Err(result) = vm.route_tokenizer_static_method_diagnostics(
                    compiled,
                    output,
                    stack,
                    state,
                    result.diagnostics,
                    Some(&trace_context),
                ) {
                    return RichDispatchOutcome::Return(Box::new(*result));
                }
                let value = result.value;
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if internal_extension_static_class(class_name) {
                let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                    Ok(values) => values,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                let value = match call_internal_extension_static_method(
                    class_name,
                    method,
                    values.into_iter().map(|arg| arg.value).collect(),
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if let Err(result) = vm.autoload_static_class_if_missing(
                ExecutionCursor::new(compiled, output, stack, state),
                class_name,
                instruction.span,
                Some((
                    compiled_unit_cache_key(compiled),
                    function_id,
                    block_id,
                    instruction.id,
                )),
            ) {
                match vm.route_throwable_result(
                    ExecutionCursor::new(compiled, output, stack, state),
                    exception_handlers,
                    pending_control,
                    *result,
                ) {
                    RaiseOutcome::Caught(target) => {
                        return RichDispatchOutcome::Jump(target);
                    }
                    RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                }
            }
            let class = match resolve_static_class_name(compiled, state, stack, class_name) {
                Ok(class) => class,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            if let Err(result) =
                vm.autoload_class_parents_if_missing(compiled, &class, output, stack, state)
            {
                match vm.route_throwable_result(
                    ExecutionCursor::new(compiled, output, stack, state),
                    exception_handlers,
                    pending_control,
                    *result,
                ) {
                    RaiseOutcome::Caught(target) => {
                        return RichDispatchOutcome::Jump(target);
                    }
                    RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                }
            }
            let scope = method_lookup_scope_for_static_call(compiled, stack, class_name);
            let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                Ok(values) => values,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            if class_extends_php_token(compiled, state, &class)
                && normalize_method_name(method) == "tokenize"
            {
                let trace_values = values
                    .iter()
                    .map(|arg| arg.value.clone())
                    .collect::<Vec<_>>();
                let result = match vm.php_token_static_method_value_for_class(
                    compiled, state, &class, class_name, values,
                ) {
                    Ok(result) => result,
                    Err(PhpTokenStaticMethodError::RuntimeClass(error)) => {
                        match vm.raise_runtime_class_entry_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            error,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                    Err(PhpTokenStaticMethodError::Runtime(message)) => {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                let trace_context = TokenizerStaticCallTraceContext {
                    call: format!("{class_name}::{method}"),
                    values: trace_values,
                    call_span: instruction.span,
                };
                if let Err(result) = vm.route_tokenizer_static_method_diagnostics(
                    compiled,
                    output,
                    stack,
                    state,
                    result.diagnostics,
                    Some(&trace_context),
                ) {
                    return RichDispatchOutcome::Return(Box::new(*result));
                }
                let value = result.value;
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if normalize_method_name(method) == "__construct"
                && is_supported_spl_runtime_class(&class.name)
            {
                let Some(object) = current_this_object(compiled, stack) else {
                    return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_NON_STATIC_METHOD_CALL: Non-static method {}::__construct() cannot be called statically",
                                        class.display_name
                                    ),
                                )));
                };
                let init_values = if is_spl_iterator_runtime_class(&class.name) {
                    match vm.prepare_spl_iterator_constructor_args(
                        compiled,
                        &class.name,
                        values,
                        output,
                        stack,
                        state,
                    ) {
                        Ok(values) => values,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    }
                } else {
                    values
                };
                if let Err(message) = initialize_spl_runtime_subclass_storage(
                    &object,
                    &class.name,
                    init_values,
                    &vm.options.runtime_context,
                    Some(&mut state.resources),
                ) {
                    match vm.raise_runtime_error(
                        ExecutionCursor::new(compiled, output, stack, state),
                        exception_handlers,
                        pending_control,
                        instruction.span,
                        message,
                    ) {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                    }
                }
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, Value::Null)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if normalize_method_name(method) == "__construct"
                && is_instantiable_internal_throwable(&class.name)
            {
                let Some(object) = current_this_object(compiled, stack) else {
                    return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_NON_STATIC_METHOD_CALL: Non-static method {}::__construct() cannot be called statically",
                                        class.display_name
                                    ),
                                )));
                };
                if let Err(message) = initialize_internal_throwable_object(
                    compiled,
                    stack,
                    &object,
                    &class.name,
                    &values,
                    instruction.span,
                ) {
                    match vm.raise_runtime_error(
                        ExecutionCursor::new(compiled, output, stack, state),
                        exception_handlers,
                        pending_control,
                        instruction.span,
                        message,
                    ) {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                    }
                }
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, Value::Null)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            if is_supported_spl_runtime_class(&class.name)
                && let Some(object) = current_this_object(compiled, stack)
                && spl_runtime_marker(&object).is_some_and(|spl_class| {
                    internal_runtime_class_is_or_extends(&spl_class, &class.name)
                })
            {
                if normalize_class_name(&class.name) == "appenditerator"
                    && matches!(
                        normalize_method_name(method).as_str(),
                        "append" | "rewind" | "next"
                    )
                {
                    let value = match vm.call_spl_append_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        values.clone(),
                        Some(instruction.span),
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if normalize_class_name(&class.name) == "norewinditerator"
                    && matches!(
                        normalize_method_name(method).as_str(),
                        "rewind" | "valid" | "current" | "key" | "next"
                    )
                {
                    let value = match vm.call_spl_no_rewind_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        values.clone(),
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if matches!(
                    normalize_class_name(&class.name).as_str(),
                    "recursiveiteratoriterator" | "recursivetreeiterator"
                ) && matches!(
                    normalize_method_name(method).as_str(),
                    "rewind" | "valid" | "current" | "next" | "callhaschildren" | "callgetchildren"
                ) {
                    let value = match vm.call_spl_recursive_iterator_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        object.clone(),
                        method,
                        values.clone(),
                        Some(instruction.span),
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            match vm.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                if let Some(result) = call_spl_runtime_method(
                    &object,
                    &class.name,
                    method,
                    values.clone(),
                    &vm.options.runtime_context,
                ) {
                    let value = match result {
                        Ok(value) => value,
                        Err(message) => {
                            match vm.raise_runtime_error(
                                ExecutionCursor::new(compiled, output, stack, state),
                                exception_handlers,
                                pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    return RichDispatchOutcome::Jump(target);
                                }
                                RaiseOutcome::Done(result) => {
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    };
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
            }
            if class.flags.is_enum
                && matches!(
                    normalize_method_name(method).as_str(),
                    "cases" | "from" | "tryfrom"
                )
            {
                let value =
                    match enum_static_method(compiled, state, &class, method, values, &|value| {
                        vm.constant_value(compiled.unit(), value)
                    }) {
                        Ok(value) => value,
                        Err(message) => {
                            return RichDispatchOutcome::Return(Box::new(
                                vm.runtime_error(output, compiled, stack, message),
                            ));
                        }
                    };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            let resolved = match lookup_resolved_method_in_state(
                compiled,
                state,
                &class.name,
                method,
                scope.as_deref(),
            ) {
                Ok(Some(method)) => method,
                Ok(None) => {
                    if let Some(object) = current_this_object(compiled, stack) {
                        let result = match vm.call_magic_instance_method(
                            ExecutionCursor::new(compiled, output, stack, state),
                            object,
                            "__call",
                            method,
                            values.clone(),
                            Some(instruction.span),
                        ) {
                            Ok(Some(result)) => result,
                            Ok(None) => {
                                let called_class = called_class_for_static_call(
                                    compiled, stack, class_name, &class,
                                );
                                match vm.call_magic_static_method(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    MagicStaticCallRequest {
                                        class: &class,
                                        magic_method: "__callStatic",
                                        called_method: method,
                                        args: values,
                                        called_class,
                                        call_span: Some(instruction.span),
                                    },
                                ) {
                                    Ok(Some(result)) => result,
                                    Ok(None) => {
                                        return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                                                        output,
                                                        compiled,
                                                        stack,
                                                        format!(
                                                            "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                                                            class.name, method
                                                        ),
                                                    )));
                                    }
                                    Err(result) => {
                                        return RichDispatchOutcome::Return(Box::new(*result));
                                    }
                                }
                            }
                            Err(result) => return RichDispatchOutcome::Return(Box::new(*result)),
                        };
                        if !result.status.is_success() {
                            return RichDispatchOutcome::Return(Box::new(result));
                        }
                        diagnostics.extend(result.diagnostics);
                        let return_value = result.return_value.unwrap_or(Value::Null);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, return_value)
                        {
                            return RichDispatchOutcome::Return(Box::new(
                                vm.runtime_error(output, compiled, stack, message),
                            ));
                        }
                        return RichDispatchOutcome::Continue;
                    }
                    let called_class =
                        called_class_for_static_call(compiled, stack, class_name, &class);
                    let result = match vm.call_magic_static_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        MagicStaticCallRequest {
                            class: &class,
                            magic_method: "__callStatic",
                            called_method: method,
                            args: values,
                            called_class,
                            call_span: Some(instruction.span),
                        },
                    ) {
                        Ok(Some(result)) => result,
                        Ok(None) => {
                            return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                                    class.name, method
                                ),
                            )));
                        }
                        Err(result) => return RichDispatchOutcome::Return(Box::new(*result)),
                    };
                    if !result.status.is_success() {
                        return RichDispatchOutcome::Return(Box::new(result));
                    }
                    diagnostics.extend(result.diagnostics);
                    let return_value = result.return_value.unwrap_or(Value::Null);
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame is active")
                        .registers
                        .set(*dst, return_value)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let method_entry = &resolved.method;
            let declaring_class = &resolved.class;
            let is_constructor_call = normalize_method_name(method) == "__construct";
            if internal_throwable_instanceof(&declaring_class.name, "throwable").is_some()
                && let Some(object) = current_this_object(compiled, stack)
                && class_is_a_in_state(compiled, state, &object.class_name(), &declaring_class.name)
                    .unwrap_or(false)
            {
                let value = match internal_throwable_method_value(
                    &object,
                    method,
                    values.into_iter().map(|arg| arg.value).collect(),
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            let bound_this_for_scoped_call = if method_entry.flags.is_static {
                None
            } else {
                let bound_this = current_this_object(compiled, stack).filter(|object| {
                    class_is_a_in_state(
                        compiled,
                        state,
                        &object.class_name(),
                        &declaring_class.name,
                    )
                    .unwrap_or(false)
                });
                if bound_this.is_none() {
                    let message = format!(
                        "E_PHP_VM_NON_STATIC_METHOD_CALL: Non-static method {}::{}() cannot be called statically",
                        declaring_class.display_name, method_entry.name
                    );
                    match vm.raise_runtime_error(
                        ExecutionCursor::new(compiled, output, stack, state),
                        exception_handlers,
                        pending_control,
                        instruction.span,
                        message,
                    ) {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                    }
                }
                bound_this
            };
            // A private/protected static method that is inaccessible
            // from the current scope routes to __callStatic (or fails);
            // one that IS accessible (e.g. a class calling its own
            // private static method) falls through to a normal call.
            if !is_constructor_call
                && (method_entry.flags.is_private || method_entry.flags.is_protected)
                && let Err(inaccessible) = validate_method_callable_in_state_scope(
                    compiled,
                    state,
                    current_scope_class(compiled, stack).as_deref(),
                    declaring_class,
                    method_entry,
                )
            {
                let called_class =
                    called_class_for_static_call(compiled, stack, class_name, &class);
                let result = match vm.call_magic_static_method(
                    ExecutionCursor::new(compiled, output, stack, state),
                    MagicStaticCallRequest {
                        class: &class,
                        magic_method: "__callStatic",
                        called_method: method,
                        args: values,
                        called_class,
                        call_span: Some(instruction.span),
                    },
                ) {
                    Ok(Some(result)) => result,
                    Ok(None) => {
                        match vm.raise_runtime_error(
                            ExecutionCursor::new(compiled, output, stack, state),
                            exception_handlers,
                            pending_control,
                            instruction.span,
                            inaccessible,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                    Err(result) => return RichDispatchOutcome::Return(Box::new(*result)),
                };
                if !result.status.is_success() {
                    return RichDispatchOutcome::Return(Box::new(result));
                }
                diagnostics.extend(result.diagnostics);
                let return_value = result.return_value.unwrap_or(Value::Null);
                if let Err(message) = stack
                    .frame_mut(frame_index)
                    .expect("frame is active")
                    .registers
                    .set(*dst, return_value)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                return RichDispatchOutcome::Continue;
            }
            let visibility = if is_constructor_call {
                validate_scoped_constructor_callable_in_state_scope(
                    compiled,
                    state,
                    scope.as_deref(),
                    declaring_class,
                    method_entry,
                )
            } else {
                validate_method_callable_in_state_scope(
                    compiled,
                    state,
                    current_scope_class(compiled, stack).as_deref(),
                    declaring_class,
                    method_entry,
                )
            };
            if let Err(message) = visibility {
                match vm.raise_runtime_error(
                    ExecutionCursor::new(compiled, output, stack, state),
                    exception_handlers,
                    pending_control,
                    instruction.span,
                    message,
                ) {
                    RaiseOutcome::Caught(target) => {
                        return RichDispatchOutcome::Jump(target);
                    }
                    RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                }
            }
            let class_owner = class_owner_in_state(compiled, state, &declaring_class.name);
            let called_class = called_class_for_static_call(compiled, stack, class_name, &class);
            let mut call = FunctionCall::new(values, Vec::new())
                .with_call_site_strict_types(call_site_strictness(compiled, Some(instruction.span)))
                .with_call_span(instruction.span)
                .with_class_context_handles(
                    vm.class_name_handles(&declaring_class.name).normalized,
                    vm.class_name_handles(&called_class).display,
                    vm.class_name_handles(&declaring_class.name).normalized,
                )
                .inherit_fiber_context(running_fiber);
            if let Some(bound_this) = bound_this_for_scoped_call {
                call = call.with_this(bound_this);
            }
            let result = vm.execute_function(
                &class_owner,
                method_entry.function,
                call,
                output,
                stack,
                state,
            );
            if !result.status.is_success() {
                match vm.route_throwable_result(
                    ExecutionCursor::new(compiled, output, stack, state),
                    exception_handlers,
                    pending_control,
                    result,
                ) {
                    RaiseOutcome::Caught(target) => {
                        return RichDispatchOutcome::Jump(target);
                    }
                    RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                }
            }
            if result.fiber_suspension.is_some() {
                return RichDispatchOutcome::Return(Box::new(vm.propagate_fiber_suspension(
                    result,
                    compiled,
                    FiberContinuationState::new(
                        *dst,
                        block_id,
                        instruction_index + 1,
                        foreach_iterators,
                        exception_handlers,
                        pending_control,
                    ),
                    output,
                    stack,
                )));
            }
            diagnostics.extend(result.diagnostics);
            let return_value = result.return_value.unwrap_or(Value::Null);
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame is active")
                .registers
                .set(*dst, return_value)
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::MakeClosure {
            dst,
            function,
            captures,
        } => {
            let captured = match evaluate_closure_captures(unit, stack, captures) {
                Ok(captures) => captures,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let value = make_closure_value(compiled, state, stack, *function, captured);
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame was pushed")
                .registers
                .set(*dst, value)
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::CallClosure { dst, callee, args } => {
            let callee = match read_operand_at_frame(unit, stack, frame_index, *callee) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let Some(payload) = callee.as_closure() else {
                return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_CALL_NON_CLOSURE: value is not a closure",
                )));
            };
            let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                Ok(values) => values,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let mut call = FunctionCall::new(values, payload.captures.clone())
                .with_call_site_strict_types(call_site_strictness(compiled, Some(instruction.span)))
                .inherit_fiber_context(running_fiber)
                .with_call_span(instruction.span)
                .with_error_context(compiled.clone());
            if let Some(bound_this) = &payload.bound_this {
                call = call.with_this(bound_this.clone());
            }
            if let Some(scope_class) = &payload.context.scope_class {
                call = call.with_class_context_handles(
                    scope_class.clone(),
                    payload
                        .context
                        .called_class
                        .as_ref()
                        .unwrap_or(scope_class)
                        .clone(),
                    payload
                        .context
                        .declaring_class
                        .as_ref()
                        .unwrap_or(scope_class)
                        .clone(),
                );
            }
            let closure_owner = closure_owner_for_function(
                compiled,
                state,
                payload.function,
                payload.debug.as_deref(),
                payload.context.owner_unit,
            );
            let result = vm.execute_function(
                &closure_owner,
                FunctionId::new(payload.function),
                call,
                output,
                stack,
                state,
            );
            if !result.status.is_success()
                && let Some(throwable) = state
                    .pending_throw
                    .take()
                    .or_else(|| runtime_error_throwable(&result))
            {
                if let Some(target) = handle_throw(
                    compiled,
                    throwable.clone(),
                    stack,
                    state,
                    exception_handlers,
                    pending_control,
                ) {
                    return RichDispatchOutcome::Jump(target);
                }
                return RichDispatchOutcome::Return(Box::new(
                    vm.propagate_exception(output, stack, state, throwable),
                ));
            }
            if !result.status.is_success() {
                return RichDispatchOutcome::Return(Box::new(result));
            }
            if result.fiber_suspension.is_some() {
                return RichDispatchOutcome::Return(Box::new(vm.propagate_fiber_suspension(
                    result,
                    compiled,
                    FiberContinuationState::new(
                        *dst,
                        block_id,
                        instruction_index + 1,
                        foreach_iterators,
                        exception_handlers,
                        pending_control,
                    ),
                    output,
                    stack,
                )));
            }
            diagnostics.extend(result.diagnostics);
            let return_value = result.return_value.unwrap_or(Value::Null);
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame is active")
                .registers
                .set(*dst, return_value)
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::ResolveCallable { dst, callable } => {
            let value = match resolve_callable(compiled, state, callable) {
                Ok(value) => value,
                Err(message) => {
                    match vm.raise_runtime_error(
                        ExecutionCursor::new(compiled, output, stack, state),
                        exception_handlers,
                        pending_control,
                        instruction.span,
                        message,
                    ) {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                    }
                }
            };
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame was pushed")
                .registers
                .set(*dst, value)
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::AcquireCallable { dst, value } => {
            let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let value = match acquire_callable_value(compiled, state, stack, value) {
                Ok(value) => value,
                Err(message) => {
                    match vm.raise_runtime_error(
                        ExecutionCursor::new(compiled, output, stack, state),
                        exception_handlers,
                        pending_control,
                        instruction.span,
                        message,
                    ) {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                    }
                }
            };
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame was pushed")
                .registers
                .set(*dst, value)
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::CallCallable { dst, callee, args } => {
            let callee = match read_operand_at_frame(unit, stack, frame_index, *callee) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                Ok(values) => values,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let temporary_iterator_arg =
                callable_iterator_function_temporary_arg_value(&callee, args, &values);
            let result = vm.execute_callable_value_call(
                compiled,
                callee,
                values,
                function_id,
                block_id,
                instruction.id,
                Some(instruction.span),
                output,
                stack,
                state,
                running_fiber,
            );
            if !result.status.is_success() {
                let original_throwable = state
                    .pending_throw
                    .take()
                    .or_else(|| runtime_error_throwable(&result));
                if let Some((arg_operand, arg_value)) = temporary_iterator_arg.clone() {
                    if let Err(message) =
                        unset_register_operand_at_frame(stack, frame_index, arg_operand)
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    let candidates = destructor_candidates_for_value(&arg_value);
                    let rooted_object_ids = php_visible_non_register_root_object_ids(stack, state);
                    let sweep = vm.run_destructors_for_unreferenced_candidates_with_roots(
                        ExecutionCursor::new(compiled, output, stack, state),
                        exception_handlers,
                        pending_control,
                        candidates,
                        &rooted_object_ids,
                        None,
                    );
                    if let Some(outcome) = sweep.outcome {
                        match outcome {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                }
                if let Some(throwable) = original_throwable {
                    state.pending_throw = Some(throwable);
                }
            }
            if !result.status.is_success()
                && let Some(throwable) = state
                    .pending_throw
                    .take()
                    .or_else(|| runtime_error_throwable(&result))
            {
                if let Some(target) = handle_throw(
                    compiled,
                    throwable.clone(),
                    stack,
                    state,
                    exception_handlers,
                    pending_control,
                ) {
                    return RichDispatchOutcome::Jump(target);
                }
                return RichDispatchOutcome::Return(Box::new(
                    vm.propagate_exception(output, stack, state, throwable),
                ));
            }
            if !result.status.is_success() {
                return RichDispatchOutcome::Return(Box::new(result));
            }
            diagnostics.extend(result.diagnostics);
            let return_value = result.return_value.unwrap_or(Value::Null);
            if let Some((arg_operand, arg_value)) = temporary_iterator_arg {
                if let Err(message) =
                    unset_register_operand_at_frame(stack, frame_index, arg_operand)
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                let mut rooted_object_ids = php_visible_root_object_ids(stack, state);
                rooted_object_ids.extend(preserved_destructor_object_ids(std::slice::from_ref(
                    &return_value,
                )));
                let candidates = destructor_candidates_for_value(&arg_value);
                let sweep = vm.run_destructors_for_unreferenced_candidates_with_roots(
                    ExecutionCursor::new(compiled, output, stack, state),
                    exception_handlers,
                    pending_control,
                    candidates,
                    &rooted_object_ids,
                    None,
                );
                if let Some(outcome) = sweep.outcome {
                    match outcome {
                        RaiseOutcome::Caught(target) => {
                            return RichDispatchOutcome::Jump(target);
                        }
                        RaiseOutcome::Done(result) => return RichDispatchOutcome::Return(result),
                    }
                }
            }
            if let Err(message) =
                unset_consumed_call_arg_registers_at_frame(stack, frame_index, args, Some(*dst))
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame is active")
                .registers
                .set(*dst, return_value)
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::Pipe {
            dst,
            input,
            callable,
        } => {
            let input = match read_operand_at_frame(unit, stack, frame_index, *input) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let callable = match read_operand_at_frame(unit, stack, frame_index, *callable) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let result = vm.call_callable(
                compiled,
                callable,
                vec![CallArgument::positional(input)],
                output,
                stack,
                state,
            );
            if !result.status.is_success()
                && let Some(throwable) = state
                    .pending_throw
                    .take()
                    .or_else(|| runtime_error_throwable(&result))
            {
                if let Some(target) = handle_throw(
                    compiled,
                    throwable.clone(),
                    stack,
                    state,
                    exception_handlers,
                    pending_control,
                ) {
                    return RichDispatchOutcome::Jump(target);
                }
                return RichDispatchOutcome::Return(Box::new(
                    vm.propagate_exception(output, stack, state, throwable),
                ));
            }
            if !result.status.is_success() {
                return RichDispatchOutcome::Return(Box::new(result));
            }
            diagnostics.extend(result.diagnostics);
            let return_value = result.return_value.unwrap_or(Value::Null);
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame is active")
                .registers
                .set(*dst, return_value)
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        _ => unreachable!("call dispatch received a non-call instruction"),
    }

    RichDispatchOutcome::Continue
}
