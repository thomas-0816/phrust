macro_rules! execute_rich_object_instruction {
    (
        $vm:expr,
        $compiled:ident,
        $unit:ident,
        $function_id:ident,
        $block_id:ident,
        $instruction:ident,
        $instruction_index:ident,
        $frame_index:ident,
        $output:ident,
        $stack:ident,
        $state:ident,
        $diagnostics:ident,
        $foreach_iterators:ident,
        $running_fiber:ident,
        $exception_handlers:ident,
        $pending_control:ident,
        $dispatch:lifetime
    ) => {{
        let compiled = $compiled;
        let unit = $unit;
        let function_id = $function_id;
        let block_id = $block_id;
        let instruction = $instruction;
        let instruction_index = $instruction_index;
        let frame_index = $frame_index;
        let output = &mut *$output;
        let stack = &mut *$stack;
        let state = &mut *$state;

        match &instruction.kind {
                    InstructionKind::DynamicNewObject {
                        dst,
                        class_name,
                        args,
                    } => {
                        let (class_name, display_class_name) = match read_operand(
                            unit,
                            stack,
                            *class_name,
                        ) {
                            Ok(Value::String(value)) => {
                                let display = display_class_name(&value.to_string_lossy());
                                (normalize_class_name(&display), display)
                            }
                            Ok(Value::Object(object)) => (
                                normalize_class_name(&object.class_name()),
                                object.display_name(),
                            ),
                            Ok(other) => {
                                return $vm.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        format!(
                                            "E_PHP_VM_INVALID_DYNAMIC_CLASS_NAME: class name must be string or object, {} given",
                                            value_type_name(&other)
                                        ),
                                    );
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                            Ok(values) => values,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if is_instantiable_internal_throwable(&class_name) {
                            let object = match new_internal_throwable_object(
                                compiled,
                                stack,
                                &class_name,
                                &values,
                                instruction.span,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(&class_name) {
                            if !values.is_empty() {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_TOO_MANY_ARGS: constructor for class {class_name} does not accept arguments"
                                    ),
                                );
                            }
                            let object =
                                ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_php_token_runtime_class(&class_name) {
                            let object = match new_php_token_object(values) {
                                Ok(object) => object,
                                Err(message) => {
                                    match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_fileinfo_runtime_class(&class_name) {
                            let object = match new_fileinfo_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_date_time_runtime_class(&class_name) {
                            let object = match new_date_time_object(
                                &class_name,
                                values,
                                &state.default_timezone,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_sqlite_runtime_class(&class_name) {
                            let object = match new_sqlite_object(
                                &class_name,
                                values,
                                &mut state.builtins.sqlite,
                                &$vm.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_mysqli_runtime_class(&class_name) {
                            let object = match new_mysqli_object(
                                &class_name,
                                values,
                                &mut state.builtins.mysql,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_redis_runtime_class(&class_name) {
                            let object = match new_redis_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_memcached_runtime_class(&class_name) {
                            let object = match new_memcached_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_soap_runtime_class(&class_name) {
                            let object = match new_soap_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_imagick_runtime_class(&class_name) {
                            let object = match new_imagick_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xsl_runtime_class(&class_name) {
                            let object = match new_xsl_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_pdo_runtime_class(&class_name) {
                            let object = match new_pdo_object(
                                &class_name,
                                values,
                                &mut state.builtins.sqlite,
                                &mut state.builtins.mysql,
                                &mut state.builtins.postgres,
                                &$vm.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        $vm.runtime_error(output, compiled, stack, message);
                                    match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_phar_runtime_class(&class_name) {
                            let object = match new_phar_object(
                                &class_name,
                                values,
                                &$vm.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        $vm.runtime_error(output, compiled, stack, message);
                                    match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_zip_runtime_class(&class_name) {
                            let object = match new_zip_object(
                                &class_name,
                                values,
                                &$vm.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xml_runtime_class(&class_name) {
                            let object = match new_xml_runtime_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        let class = if let Some(class) =
                            $vm.cached_class_entry(compiled, state, &class_name)
                        {
                            class
                        } else {
                            match $vm.class_like_exists_with_autoload_cache(ExecutionCursor::new(compiled, output, stack, state), &display_class_name, AutoloadClassLookupKind::Class, true, Some((
                                    compiled_unit_cache_key(compiled),
                                    function_id,
                                    block_id,
                                    instruction.id,
                                ))) {
                                Ok(_) => {}
                                Err(result) => return *result,
                            }
                            let Some(class) = $vm.cached_class_entry(compiled, state, &class_name)
                            else {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, format!(
                                        "E_PHP_VM_UNKNOWN_CLASS: Class \"{display_class_name}\" not found"
                                    )) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            };
                            class
                        };
                        if let Err(result) = $vm.autoload_class_parents_if_missing(
                            compiled, &class, output, stack, state,
                        ) {
                            return *result;
                        }
                        let class_owner = class_owner_in_state(compiled, state, &class.name);
                        let runtime_class =
                            match $vm.cached_runtime_class_entry(&class_owner, state, &class) {
                                Ok(class) => class,
                                Err(error) => {
                                    match $vm.raise_runtime_class_entry_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, error) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                        if let Err(message) = validate_object_mvp(&runtime_class) {
                            match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        let spl_runtime_parent =
                            spl_runtime_parent_for_class(compiled, state, &class);
                        let slot_template = $vm.cached_default_slot_template(
                            &class_owner,
                            state,
                            &runtime_class,
                            &display_class_name,
                        );
                        let object = ObjectRef::from_layout_slots(
                            &runtime_class,
                            display_class_name,
                            (*slot_template).clone(),
                        );
                        if let Some(spl_class) = spl_runtime_parent.as_deref() {
                            object.set_property(
                                SPL_RUNTIME_CLASS_PROPERTY,
                                Value::string(spl_class.as_bytes().to_vec()),
                            );
                        }
                        let caller_scope = current_scope_class(compiled, stack);
                        let constructor = match $vm.cached_constructor_resolution(
                            compiled,
                            state,
                            &class.name,
                            caller_scope.as_deref(),
                        ) {
                            Ok(constructor) => constructor,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Some(constructor) = constructor {
                            if let Err(message) = validate_constructor_callable_in_state_scope(
                                compiled,
                                state,
                                caller_scope.as_deref(),
                                &constructor.class,
                                &constructor.method,
                            ) {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            let class_owner =
                                dynamic_class_owner_in_state(state, &constructor.class.name)
                                    .unwrap_or_else(|| compiled.clone());
                            let result = $vm.execute_function(
                                &class_owner,
                                constructor.method.function,
                                FunctionCall::new(values, Vec::new())
                                    .with_call_site_strict_types(call_site_strictness(
                                        compiled,
                                        Some(instruction.span),
                                    ))
                                    .with_call_span(instruction.span)
                                    .with_this(object.clone())
                                    .with_class_context_handles(
                                        $vm.class_name_handles(&constructor.class.name).normalized,
                                        object_called_class_handle(&object),
                                        $vm.class_name_handles(&constructor.class.name).normalized,
                                    )
                                .inherit_fiber_context(&$running_fiber),
                                output,
                                stack,
                                state,
                            );
                            if !result.status.is_success() {
                                match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            if result.fiber_suspension.is_some() {
                                return $vm.propagate_fiber_suspension(
                                    result,
                                    compiled,
                                    FiberContinuationState::new(
                                        *dst,
                                        block_id,
                                        instruction_index + 1,
                                        &$foreach_iterators,
                                        &$exception_handlers,
                                        &$pending_control,
                                    ),
                                    output,
                                    stack,
                                );
                            }
                            $diagnostics.extend(result.diagnostics);
                        } else if let Some(spl_class) = spl_runtime_parent {
                            let init_values = if is_spl_iterator_runtime_class(&spl_class) {
                                match $vm.prepare_spl_iterator_constructor_args(
                                    compiled, &spl_class, values, output, stack, state,
                                ) {
                                    Ok(values) => values,
                                    Err(result) => {
                                        match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                            RaiseOutcome::Caught(target) => {
                                                $block_id = target;
                                                continue $dispatch;
                                            }
                                            RaiseOutcome::Done(result) => return *result,
                                        }
                                    }
                                }
                            } else {
                                values
                            };
                            if let Err(message) = initialize_spl_runtime_subclass_storage(
                                &object,
                                &spl_class,
                                init_values,
                                &$vm.options.runtime_context,
                                Some(&mut state.resources),
                            ) {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                        }
                        $vm.register_destructor_if_needed(compiled, &class, object.clone(), state);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, Value::Object(object))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = unset_indirect_temporary_call_arg_registers_at_frame(
                            stack,
                            frame_index,
                            args,
                        ) {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::NewObject {
                        dst,
                        display_class_name,
                        class_name,
                        args,
                    } => {
                        let (resolved_class_name, resolved_display_class_name) =
                            if is_special_static_class_name(class_name) {
                                match resolve_static_class_name(compiled, state, stack, class_name)
                                {
                                    Ok(class) => (class.name.clone(), class.display_name.clone()),
                                    Err(message) => {
                                        match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                            RaiseOutcome::Caught(target) => {
                                                $block_id = target;
                                                continue $dispatch;
                                            }
                                            RaiseOutcome::Done(result) => return *result,
                                        }
                                    }
                                }
                            } else {
                                (class_name.clone(), display_class_name.clone())
                            };
                        let class_name = resolved_class_name.as_str();
                        let display_class_name = resolved_display_class_name.as_str();
                        if is_closure_runtime_class(class_name) {
                            match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, "E_PHP_VM_CLOSURE_INSTANTIATION: Instantiation of class Closure is not allowed"
                                    .to_owned()) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if is_fiber_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let fiber = match new_fiber_object(values) {
                                Ok(fiber) => fiber,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Fiber(fiber))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_reflection_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let values =
                                values.into_iter().map(|arg| arg.value).collect::<Vec<_>>();
                            if let Err(result) = $vm.preflight_reflection_constructor(
                                compiled, class_name, &values, output, stack, state,
                            ) {
                                match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            let object = match $vm.reflection_new_object(
                                compiled, class_name, values, output, stack, state,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        $vm.runtime_error(output, compiled, stack, message);
                                    match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_spl_iterator_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let values = match $vm.prepare_spl_iterator_constructor_args(
                                compiled, class_name, values, output, stack, state,
                            ) {
                                Ok(values) => values,
                                Err(result) => {
                                    match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            let object = match new_spl_iterator_object(
                                class_name,
                                values,
                                &$vm.options.runtime_context,
                                Some(&mut state.resources),
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_spl_container_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_spl_container_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        $vm.runtime_error(output, compiled, stack, message);
                                    match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_spl_heap_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_spl_heap_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        $vm.runtime_error(output, compiled, stack, message);
                                    match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_spl_file_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_spl_file_object(
                                class_name,
                                values,
                                &$vm.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        $vm.runtime_error(output, compiled, stack, message);
                                    match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            if !values.is_empty() {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_TOO_MANY_ARGS: constructor for class {class_name} does not accept arguments"
                                    ),
                                );
                            }
                            let object =
                                ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_php_token_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_php_token_object(values) {
                                Ok(object) => object,
                                Err(message) => {
                                    match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_fileinfo_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_fileinfo_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_date_time_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_date_time_object(
                                class_name,
                                values,
                                &state.default_timezone,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_sqlite_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_sqlite_object(
                                class_name,
                                values,
                                &mut state.builtins.sqlite,
                                &$vm.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_pdo_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_pdo_object(
                                class_name,
                                values,
                                &mut state.builtins.sqlite,
                                &mut state.builtins.mysql,
                                &mut state.builtins.postgres,
                                &$vm.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_mysqli_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_mysqli_object(
                                class_name,
                                values,
                                &mut state.builtins.mysql,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_redis_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_redis_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_memcached_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_memcached_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_soap_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_soap_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_imagick_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_imagick_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xsl_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_xsl_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_phar_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_phar_object(
                                class_name,
                                values,
                                &$vm.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_zip_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_zip_object(
                                class_name,
                                values,
                                &$vm.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xml_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_xml_runtime_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_instantiable_internal_throwable(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_internal_throwable_object(
                                compiled,
                                stack,
                                class_name,
                                &values,
                                instruction.span,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        let class = match $vm.cached_class_entry(compiled, state, class_name) {
                            Some(class) => class,
                            None => {
                                if is_spl_iterator_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let values = match $vm.prepare_spl_iterator_constructor_args(
                                        compiled, class_name, values, output, stack, state,
                                    ) {
                                        Ok(values) => values,
                                        Err(result) => {
                                            match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                                RaiseOutcome::Caught(target) => {
                                                    $block_id = target;
                                                    continue $dispatch;
                                                }
                                                RaiseOutcome::Done(result) => return *result,
                                            }
                                        }
                                    };
                                    let object = match new_spl_iterator_object(
                                        class_name,
                                        values,
                                        &$vm.options.runtime_context,
                                        Some(&mut state.resources),
                                    ) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                                RaiseOutcome::Caught(target) => {
                                                    $block_id = target;
                                                    continue $dispatch;
                                                }
                                                RaiseOutcome::Done(result) => return *result,
                                            }
                                        }
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_spl_container_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let object = match new_spl_container_object(class_name, values)
                                    {
                                        Ok(object) => object,
                                        Err(message) => {
                                            let result = $vm
                                                .runtime_error(output, compiled, stack, message);
                                            match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                                RaiseOutcome::Caught(target) => {
                                                    $block_id = target;
                                                    continue $dispatch;
                                                }
                                                RaiseOutcome::Done(result) => return *result,
                                            }
                                        }
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_spl_heap_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let object = match new_spl_heap_object(class_name, values) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            let result = $vm
                                                .runtime_error(output, compiled, stack, message);
                                            match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                                RaiseOutcome::Caught(target) => {
                                                    $block_id = target;
                                                    continue $dispatch;
                                                }
                                                RaiseOutcome::Done(result) => return *result,
                                            }
                                        }
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_spl_file_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let object = match new_spl_file_object(
                                        class_name,
                                        values,
                                        &$vm.options.runtime_context,
                                    ) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            let result = $vm
                                                .runtime_error(output, compiled, stack, message);
                                            match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                                RaiseOutcome::Caught(target) => {
                                                    $block_id = target;
                                                    continue $dispatch;
                                                }
                                                RaiseOutcome::Done(result) => return *result,
                                            }
                                        }
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_std_class_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    if !values.is_empty() {
                                        return $vm.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            format!(
                                                "E_PHP_VM_TOO_MANY_ARGS: constructor for class {class_name} does not accept arguments"
                                            ),
                                        );
                                    }
                                    let object = ObjectRef::new_with_display_name(
                                        &std_class_entry(),
                                        "stdClass",
                                    );
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_date_time_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let object = match new_date_time_object(
                                        class_name,
                                        values,
                                        &state.default_timezone,
                                    ) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if class_name
                                    .trim_start_matches('\\')
                                    .to_ascii_lowercase()
                                    .starts_with("reflection")
                                {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values
                                            .into_iter()
                                            .map(|arg| arg.value)
                                            .collect::<Vec<_>>(),
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    if let Err(result) = $vm.preflight_reflection_constructor(
                                        compiled, class_name, &values, output, stack, state,
                                    ) {
                                        match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                            RaiseOutcome::Caught(target) => {
                                                $block_id = target;
                                                continue $dispatch;
                                            }
                                            RaiseOutcome::Done(result) => return *result,
                                        }
                                    }
                                    let object = match $vm.reflection_new_object(
                                        compiled, class_name, values, output, stack, state,
                                    ) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            let result = $vm
                                                .runtime_error(output, compiled, stack, message);
                                            match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                                RaiseOutcome::Caught(target) => {
                                                    $block_id = target;
                                                    continue $dispatch;
                                                }
                                                RaiseOutcome::Done(result) => return *result,
                                            }
                                        }
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                match $vm.class_like_exists_with_autoload_cache(ExecutionCursor::new(compiled, output, stack, state), display_class_name, AutoloadClassLookupKind::Class, true, Some((
                                        compiled_unit_cache_key(compiled),
                                        function_id,
                                        block_id,
                                        instruction.id,
                                    ))) {
                                    Ok(_) => {}
                                    Err(result) => {
                                        match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                            RaiseOutcome::Caught(target) => {
                                                $block_id = target;
                                                continue $dispatch;
                                            }
                                            RaiseOutcome::Done(result) => return *result,
                                        }
                                    }
                                }
                                if let Some(class) =
                                    $vm.cached_class_entry(compiled, state, class_name)
                                {
                                    class
                                } else {
                                    return $vm.runtime_error_with_bringup_context(
                                        ExecutionView::new(compiled, output, stack, state),
                                        runtime_source_span(compiled, instruction.span),
                                        format!(
                                            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
                                        ),
                                        BringupDiagnosticInput {
                                            error_class: Some("autoload_lookup"),
                                            requested_name: Some(class_name.to_owned()),
                                            lookup_kind: Some("class"),
                                            autoload_enabled: Some(true),
                                            ..BringupDiagnosticInput::default()
                                        },
                                    );
                                }
                            }
                        };
                        if let Err(result) = $vm.autoload_class_parents_if_missing(
                            compiled, &class, output, stack, state,
                        ) {
                            match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        let class_owner = class_owner_in_state(compiled, state, &class.name);
                        let runtime_class =
                            match $vm.cached_runtime_class_entry(&class_owner, state, &class) {
                                Ok(class) => class,
                                Err(error) => {
                                    match $vm.raise_runtime_class_entry_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, error) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                        if let Err(message) = validate_object_mvp(&runtime_class) {
                            match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                            Ok(values) => values,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let spl_runtime_parent =
                            spl_runtime_parent_for_class(compiled, state, &class);
                        let slot_template = $vm.cached_default_slot_template(
                            &class_owner,
                            state,
                            &runtime_class,
                            display_class_name,
                        );
                        let object = ObjectRef::from_layout_slots(
                            &runtime_class,
                            display_class_name,
                            (*slot_template).clone(),
                        );
                        if let Some(spl_class) = spl_runtime_parent.as_deref() {
                            object.set_property(
                                SPL_RUNTIME_CLASS_PROPERTY,
                                Value::string(spl_class.as_bytes().to_vec()),
                            );
                        }
                        let caller_scope = current_scope_class(compiled, stack);
                        let constructor = match $vm.cached_constructor_resolution(
                            compiled,
                            state,
                            &class.name,
                            caller_scope.as_deref(),
                        ) {
                            Ok(constructor) => constructor,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Some(constructor) = constructor {
                            if let Err(message) = validate_constructor_callable_in_state_scope(
                                compiled,
                                state,
                                caller_scope.as_deref(),
                                &constructor.class,
                                &constructor.method,
                            ) {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            let class_owner =
                                dynamic_class_owner_in_state(state, &constructor.class.name)
                                    .unwrap_or_else(|| compiled.clone());
                            let result = $vm.execute_function(
                                &class_owner,
                                constructor.method.function,
                                FunctionCall::new(values, Vec::new())
                                    .with_call_site_strict_types(call_site_strictness(
                                        compiled,
                                        Some(instruction.span),
                                    ))
                                    .with_call_span(instruction.span)
                                    .with_this(object.clone())
                                    .with_class_context_handles(
                                        $vm.class_name_handles(&constructor.class.name).normalized,
                                        object_called_class_handle(&object),
                                        $vm.class_name_handles(&constructor.class.name).normalized,
                                    )
                                .inherit_fiber_context(&$running_fiber),
                                output,
                                stack,
                                state,
                            );
                            if !result.status.is_success() {
                                match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, result) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            if result.fiber_suspension.is_some() {
                                return $vm.propagate_fiber_suspension(
                                    result,
                                    compiled,
                                    FiberContinuationState::new(
                                        *dst,
                                        block_id,
                                        instruction_index + 1,
                                        &$foreach_iterators,
                                        &$exception_handlers,
                                        &$pending_control,
                                    ),
                                    output,
                                    stack,
                                );
                            }
                            $diagnostics.extend(result.diagnostics);
                        } else if let Some(spl_class) = spl_runtime_parent {
                            let init_values = if is_spl_iterator_runtime_class(&spl_class) {
                                match $vm.prepare_spl_iterator_constructor_args(
                                    compiled, &spl_class, values, output, stack, state,
                                ) {
                                    Ok(values) => values,
                                    Err(result) => {
                                        match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                            RaiseOutcome::Caught(target) => {
                                                $block_id = target;
                                                continue $dispatch;
                                            }
                                            RaiseOutcome::Done(result) => return *result,
                                        }
                                    }
                                }
                            } else {
                                values
                            };
                            if let Err(message) = initialize_spl_runtime_subclass_storage(
                                &object,
                                &spl_class,
                                init_values,
                                &$vm.options.runtime_context,
                                Some(&mut state.resources),
                            ) {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                        }
                        $vm.register_destructor_if_needed(compiled, &class, object.clone(), state);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, Value::Object(object))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = unset_indirect_temporary_call_arg_registers_at_frame(
                            stack,
                            frame_index,
                            args,
                        ) {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::CloneObject { dst, object } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        match $vm.clone_object_value(
                            compiled,
                            &object,
                            instruction.span,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(value) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            }
                            Err(ClassConstantFetch::Throwable(result)) => {
                                match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(ClassConstantFetch::Raise(span, message)) => {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, span, message) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(ClassConstantFetch::Fatal(message)) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        }
                    }
                    InstructionKind::CloneWith {
                        dst,
                        object,
                        replacements,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_CLONE_NON_OBJECT: cannot clone {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let replacements = match read_operand_at_frame(
                            unit,
                            stack,
                            frame_index,
                            *replacements,
                        ) {
                            Ok(Value::Array(replacements)) => replacements,
                            Ok(other) => {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_CLONE_WITH_REPLACEMENTS: clone-with replacements must be array, got {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let class =
                            match lookup_class_in_state(compiled, state, &object.class_name()) {
                                Some(class) => class,
                                None => {
                                    return $vm.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        format!(
                                            "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
                                            object.class_name()
                                        ),
                                    );
                                }
                            };
                        let runtime_class = match runtime_class_entry(
                            compiled,
                            state,
                            &class,
                            &|value| $vm.constant_value(compiled.unit(), value),
                            &|reference| class_constant_reference_value(compiled, state, reference),
                            &|reference| named_constant_reference_value(compiled, state, reference),
                        ) {
                            Ok(class) => class,
                            Err(error) => {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    error.into_message(),
                                );
                            }
                        };
                        if let Err(message) = validate_object_mvp(&runtime_class) {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                        let copy = match $vm.clone_object_with_magic(
                            compiled,
                            object.clone(),
                            &class,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(copy) => copy,
                            Err(result) => {
                                match $vm.route_throwable_result(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, *result) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                        };
                        $vm.register_destructor_if_needed(compiled, &class, copy.clone(), state);
                        for (key, value) in replacements.iter() {
                            let property = match clone_with_property_name(&key) {
                                Ok(property) => property,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            let Some(ir_property) =
                                class.properties.iter().find(|entry| entry.name == property)
                            else {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_UNKNOWN_PROPERTY: property {}::${property} is not declared",
                                        object.class_name()
                                    ),
                                );
                            };
                            if let Err(message) =
                                validate_property_access(compiled, stack, &class, ir_property)
                            {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            if let Err(message) =
                                validate_property_set_access(compiled, stack, &class, ir_property)
                            {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            if ir_property.flags.is_readonly || class.flags.is_readonly {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, format!(
                                        "E_PHP_VM_READONLY_PROPERTY_WRITE: Cannot modify protected(set) readonly property {}::${property} from global scope",
                                        class.display_name
                                    )) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            if ir_property.flags.is_static {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, format!(
                                        "E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER: property {}::${property} uses modifiers outside the reflection-clone clone-with MVP",
                                        class.display_name
                                    )) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            let storage_name = property_storage_name(&class, ir_property);
                            let Some(entry) = runtime_class
                                .properties
                                .iter()
                                .find(|entry| entry.name == storage_name)
                            else {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_UNKNOWN_PROPERTY: property {}::${property} is not declared",
                                        object.class_name()
                                    ),
                                );
                            };
                            if let Err(message) = check_property_type(
                                compiled,
                                Some(state),
                                class.display_name.as_str(),
                                &property,
                                &entry.type_,
                                value,
                                $vm.typecheck_fast_path_context(),
                            ) {
                                match $vm.raise_runtime_error(ExecutionCursor::new(compiled, output, stack, state), &mut $exception_handlers, &mut $pending_control, instruction.span, message) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            if let Some(function_id) = entry.hooks.set_function_id {
                                match $vm.call_property_hook(ExecutionCursor::new(compiled, output, stack, state), copy.clone(), &class, ir_property, FunctionId::new(function_id), vec![CallArgument::positional(value.clone())]) {
                                    Ok(_) => continue,
                                    Err(result) => return *result,
                                }
                            }
                            if !entry.hooks.backed
                                && (entry.hooks.get_function_id.is_some()
                                    || entry.hooks.set_function_id.is_some())
                            {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_VIRTUAL_PROPERTY_WRITE: property {}::${property} has no backing storage",
                                        object.class_name()
                                    ),
                                );
                            }
                            copy.set_property(property, value.clone());
                        }
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Object(copy))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
            _ => unreachable!("non-object instruction reached rich object dispatch"),
        }
    }};
}

pub(super) use execute_rich_object_instruction;
