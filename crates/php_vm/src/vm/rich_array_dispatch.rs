use super::*;

pub(super) fn execute_rich_array_instruction(
    vm: &Vm,
    compiled: &CompiledUnit,
    site: dispatch_contract::RichInstructionSite<'_>,
    output: &mut OutputBuffer,
    stack: &mut CallStack,
    state: &mut ExecutionState,
    diagnostics: &mut Vec<RuntimeDiagnostic>,
    exception_handlers: &mut Vec<ExceptionHandler>,
    pending_control: &mut Option<PendingControl>,
) -> RichDispatchOutcome {
    let dispatch_contract::RichInstructionSite {
        unit,
        function,
        function_id,
        block_id,
        instruction,
        instruction_index: _,
        frame_index,
    } = site;

    match &instruction.kind {
        InstructionKind::ArrayInsert {
            array,
            key,
            value,
            by_ref_local,
        } => {
            let key = match key {
                Some(key) => {
                    match read_operand_at_frame(unit, stack, frame_index, *key)
                        .and_then(|value| array_key_from_value(&value))
                    {
                        Ok(key) => Some(key),
                        Err(message) => {
                            return RichDispatchOutcome::Return(Box::new(
                                vm.runtime_error(output, compiled, stack, message),
                            ));
                        }
                    }
                }
                None => None,
            };
            let value = if let Some(local) = by_ref_local {
                let _source = layout_source::enter(layout_source::BY_REF_ARGUMENT_BINDING);
                match stack
                    .frame_mut(frame_index)
                    .expect("frame was pushed")
                    .locals
                    .ensure_reference_cell(*local)
                {
                    Ok(cell) => Value::Reference(cell),
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                }
            } else {
                match read_operand_at_frame(unit, stack, frame_index, *value) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                }
            };
            let Some(Value::Array(array_value)) = stack
                .frame_mut(frame_index)
                .expect("frame was pushed")
                .registers
                .get_mut(*array)
            else {
                return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAY_INSERT_TARGET: target is not an array register",
                )));
            };
            let was_packed = array_value.is_packed_fast();
            if let Some(key) = key {
                array_value.insert(key, value);
            } else {
                array_value.append(value);
                if was_packed && array_value.is_packed_fast() {
                    vm.record_counter_array_packed_append_fast_path_hit();
                }
            }
            if was_packed && !array_value.is_packed_fast() {
                vm.record_counter_array_packed_to_mixed_transition();
            }
        }
        InstructionKind::ArraySpread { array, source } => {
            let source = match read_operand_at_frame(unit, stack, frame_index, *source) {
                Ok(Value::Array(array)) => array,
                Ok(other) => {
                    return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_ARRAY_SPREAD_NON_ARRAY: cannot unpack {} into array literal",
                            value_type_name(&other)
                        ),
                    )));
                }
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let Some(Value::Array(array_value)) = stack
                .frame_mut(frame_index)
                .expect("frame was pushed")
                .registers
                .get_mut(*array)
            else {
                return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAY_SPREAD_TARGET: target is not an array register",
                )));
            };
            array_value.spread_extend(&source);
        }
        InstructionKind::FetchDim {
            dst,
            array,
            key,
            quiet,
        } => {
            let _profile = vm.request_profile_operation_start(
                RequestProfileOperationCategory::Array,
                "dim_fetch",
            );
            let _clone_source = layout_source::enter(layout_source::ARRAY_ELEMENT_READ);
            let array_ref = if let Operand::Local(local) = array
                && is_globals_local(function, *local)
            {
                DenseOperandRead::Owned(Value::Array(state.globals.globals_array()))
            } else {
                match read_operand_ref_at_frame(unit, stack, frame_index, *array) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                }
            };
            let key_ref = match read_operand_ref_at_frame(unit, stack, frame_index, *key) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let value = vm.try_quickened_packed_array_int_key(
                function_id,
                block_id,
                instruction.id,
                array_ref.as_value(),
                key_ref.as_value(),
            );
            let value = match value {
                Some(value) => value,
                None => {
                    if let Some(value) = vm.try_dense_array_fetch_dim_borrowed(
                        array_ref.as_value(),
                        key_ref.as_value(),
                        *quiet,
                    ) {
                        value
                    } else {
                        let array = array_ref.into_owned();
                        let key_value = key_ref.into_owned();
                        let base = effective_value(&array);
                        if let Some(object) =
                            match userland_arrayaccess_object(compiled, state, &base) {
                                Ok(object) => object,
                                Err(message) => {
                                    match vm.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                        {
                            match vm.call_userland_arrayaccess_method(
                                compiled,
                                output,
                                stack,
                                state,
                                object,
                                "offsetGet",
                                vec![CallArgument::positional(key_value.clone())],
                                instruction.span,
                            ) {
                                Ok(value) => value,
                                Err(result) => {
                                    return RichDispatchOutcome::Return(Box::new(result));
                                }
                            }
                        } else if let Value::Object(object) = &base
                            && spl_runtime_marker(object)
                                .is_some_and(|class| is_spl_array_access_runtime_class(&class))
                        {
                            match spl_container_offset_get(object, &key_value) {
                                Ok(value) => value,
                                Err(message) => {
                                    match vm.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                        } else {
                            let key = match array_key_from_value(&key_value) {
                                Ok(key) => key,
                                Err(message) => {
                                    match vm.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Value::Object(object) = &base
                                && normalize_class_name(&object.class_name()) == "simplexmlelement"
                            {
                                php_runtime::api::xml::simplexml_dimension(object, &key)
                            } else if let Value::String(string) = &base {
                                match string_offset_for_read(string, &key) {
                                    StringOffsetRead::Byte(value) => value,
                                    StringOffsetRead::Illegal { value, key_bytes } => {
                                        if !*quiet {
                                            let diagnostic = illegal_string_offset_warning(
                                                &key_bytes,
                                                runtime_source_span(compiled, instruction.span),
                                                stack_trace(compiled, stack),
                                            );
                                            match vm.dispatch_error_handler(
                                                compiled,
                                                output,
                                                stack,
                                                state,
                                                php_runtime::api::PHP_E_WARNING,
                                                &diagnostic,
                                            ) {
                                                Ok(false)
                                                    if error_reporting_allows(
                                                        state,
                                                        php_runtime::api::PHP_E_WARNING,
                                                    ) =>
                                                {
                                                    emit_vm_diagnostic(
                                                        output,
                                                        state,
                                                        &diagnostic,
                                                        php_runtime::api::PhpDiagnosticChannel::Warning,
                                                        php_runtime::api::PHP_E_WARNING,
                                                    );
                                                    diagnostics.push(diagnostic);
                                                }
                                                Ok(_) => {}
                                                Err(result) => {
                                                    return RichDispatchOutcome::Return(Box::new(
                                                        result,
                                                    ));
                                                }
                                            }
                                        }
                                        value
                                    }
                                    StringOffsetRead::OutOfRange(index) => {
                                        if *quiet {
                                            Value::Null
                                        } else {
                                            let diagnostic = uninitialized_string_offset_warning(
                                                index,
                                                runtime_source_span(compiled, instruction.span),
                                                stack_trace(compiled, stack),
                                            );
                                            match vm.dispatch_error_handler(
                                                compiled,
                                                output,
                                                stack,
                                                state,
                                                php_runtime::api::PHP_E_WARNING,
                                                &diagnostic,
                                            ) {
                                                Ok(false)
                                                    if error_reporting_allows(
                                                        state,
                                                        php_runtime::api::PHP_E_WARNING,
                                                    ) =>
                                                {
                                                    emit_vm_diagnostic(
                                                        output,
                                                        state,
                                                        &diagnostic,
                                                        php_runtime::api::PhpDiagnosticChannel::Warning,
                                                        php_runtime::api::PHP_E_WARNING,
                                                    );
                                                    diagnostics.push(diagnostic);
                                                }
                                                Ok(_) => {}
                                                Err(result) => {
                                                    return RichDispatchOutcome::Return(Box::new(
                                                        result,
                                                    ));
                                                }
                                            }
                                            Value::string(Vec::new())
                                        }
                                    }
                                    StringOffsetRead::NonNumeric => {
                                        if *quiet {
                                            Value::Null
                                        } else {
                                            let result = vm.runtime_error_with_source_span(
                                                    output,
                                                    compiled,
                                                    stack,
                                                    runtime_source_span(
                                                        compiled,
                                                        instruction.span,
                                                    ),
                                                    "E_PHP_VM_STRING_OFFSET_TYPE: Cannot access offset of type string on string"
                                                        .to_owned(),
                                                );
                                            if let Some(throwable) = state
                                                .pending_throw
                                                .take()
                                                .or_else(|| runtime_error_throwable(&result))
                                            {
                                                tag_throwable_location(
                                                    &throwable,
                                                    compiled,
                                                    instruction.span,
                                                );
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
                                                    vm.propagate_exception(
                                                        output, stack, state, throwable,
                                                    ),
                                                ));
                                            }
                                            return RichDispatchOutcome::Return(Box::new(result));
                                        }
                                    }
                                }
                            } else if let Value::Array(array_value) = &base {
                                match vm.try_array_shape_lookup(array_value, &key) {
                                    Some(Some(value)) => value,
                                    Some(None) if *quiet => Value::Null,
                                    Some(None) => {
                                        diagnostics.push(undefined_array_key_warning(
                                            &key,
                                            runtime_source_span(compiled, instruction.span),
                                            stack_trace(compiled, stack),
                                        ));
                                        Value::Null
                                    }
                                    None => match fetch_dim_value(&array, &key) {
                                        Ok(Some(value)) => value,
                                        Ok(None) if *quiet => Value::Null,
                                        Ok(None) => {
                                            diagnostics.push(undefined_array_key_warning(
                                                &key,
                                                runtime_source_span(compiled, instruction.span),
                                                stack_trace(compiled, stack),
                                            ));
                                            Value::Null
                                        }
                                        Err(message) => {
                                            match vm.raise_runtime_error(
                                                compiled,
                                                output,
                                                stack,
                                                state,
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
                                    },
                                }
                            } else if *quiet && quiet_dim_fetch_scalar_returns_null(&base) {
                                Value::Null
                            } else if quiet_dim_fetch_scalar_returns_null(&base) {
                                match vm.emit_array_offset_on_scalar_warning(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    diagnostics,
                                    &base,
                                    instruction.span,
                                ) {
                                    Ok(()) => Value::Null,
                                    Err(result) => {
                                        return RichDispatchOutcome::Return(Box::new(result));
                                    }
                                }
                            } else {
                                match fetch_dim_value(&array, &key) {
                                    Ok(Some(value)) => value,
                                    Ok(None) if *quiet => Value::Null,
                                    Ok(None) => {
                                        diagnostics.push(undefined_array_key_warning(
                                            &key,
                                            runtime_source_span(compiled, instruction.span),
                                            stack_trace(compiled, stack),
                                        ));
                                        Value::Null
                                    }
                                    Err(message) => {
                                        match vm.raise_runtime_error(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                            }
                        }
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
        InstructionKind::AssignDim {
            dst,
            local,
            dims,
            value,
        } => {
            let _profile = vm.request_profile_operation_start(
                RequestProfileOperationCategory::Array,
                "dim_assign",
            );
            let _clone_source = layout_source::enter(layout_source::ARRAY_ELEMENT_WRITE);
            let dim_values = match read_dim_operand_values_at_frame(unit, stack, frame_index, dims)
            {
                Ok(dims) => dims,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            match vm.try_userland_arrayaccess_offset_set_local(
                compiled,
                output,
                stack,
                state,
                *local,
                &dim_values,
                false,
                &value,
                instruction.span,
            ) {
                Ok(true) => {
                    vm.record_lvalue_trace_event("array-write-dim", *local, &[]);
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
                    return RichDispatchOutcome::Continue;
                }
                Ok(false) => {}
                Err(result) => return RichDispatchOutcome::Return(Box::new(result)),
            }
            let dims = match dim_values_to_array_keys(&dim_values) {
                Ok(dims) => dims,
                Err(message) => {
                    if let Some(object) =
                        spl_multiple_iterator_local_object_at_frame(stack, frame_index, *local)
                        && dim_values.len() == 1
                        && matches!(effective_value(&dim_values[0]), Value::Object(_))
                    {
                        match vm.spl_multiple_iterator_offset_set(
                            compiled,
                            &object,
                            dim_values[0].clone(),
                            value.clone(),
                            output,
                            stack,
                        ) {
                            Ok(()) => {
                                vm.record_lvalue_trace_event("array-write-dim", *local, &[]);
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
                                return RichDispatchOutcome::Continue;
                            }
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
                                        return RichDispatchOutcome::Return(result);
                                    }
                                }
                            }
                        }
                    }
                    if let Some(object) =
                        spl_object_storage_local_object_at_frame(stack, frame_index, *local)
                        && dim_values.len() == 1
                    {
                        if let Err(message) =
                            spl_container_offset_set(&object, dim_values[0].clone(), value.clone())
                        {
                            return RichDispatchOutcome::Return(Box::new(
                                vm.runtime_error(output, compiled, stack, message),
                            ));
                        }
                        vm.record_lvalue_trace_event("array-write-dim", *local, &[]);
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
                        return RichDispatchOutcome::Continue;
                    }
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            if !is_globals_local(function, *local)
                && dims.len() == 1
                && let Some(Value::Object(object)) =
                    read_local_value_at_frame(stack, frame_index, *local)
                        .map(|value| effective_value(&value))
                && spl_runtime_marker(&object)
                    .is_some_and(|class| is_spl_array_access_runtime_class(&class))
            {
                if let Err(message) = spl_container_offset_set(
                    &object,
                    array_key_to_value(dims[0].clone()),
                    value.clone(),
                ) {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                vm.record_lvalue_trace_event("array-write-dim", *local, &dims);
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
                return RichDispatchOutcome::Continue;
            }
            let result = if is_globals_local(function, *local) {
                assign_globals_dim(&mut state.globals, &dims, value.clone(), false)
            } else {
                let was_packed = local_array_is_packed_fast_at_frame(stack, frame_index, *local);
                let cow_or_reference =
                    local_array_has_cow_or_reference_fallback_at_frame(stack, frame_index, *local);
                let result = assign_dim_local(stack, *local, &dims, value.clone(), false);
                if let Ok(path) = &result {
                    vm.record_counter_map_update_slot_path(*path, &dims, false);
                }
                let result = result.map(|_| ());
                if result.is_ok() && cow_or_reference {
                    vm.record_counter_cow_or_reference_fallback();
                }
                if result.is_ok()
                    && was_packed
                    && !local_array_is_packed_fast_at_frame(stack, frame_index, *local)
                {
                    vm.record_counter_array_packed_to_mixed_transition();
                }
                result
            };
            if let Err(message) = result {
                if let Some(index) = message.negative_string_offset() {
                    emit_string_offset_negative_warning(
                        compiled,
                        output,
                        stack,
                        state,
                        instruction.span,
                        index,
                    );
                    vm.record_lvalue_trace_event("array-write-dim", *local, &dims);
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
                    return RichDispatchOutcome::Continue;
                }
                let message = message.render_message();
                if message.starts_with("E_PHP_VM_STRING_OFFSET_TYPE:") {
                    match vm.raise_runtime_error(
                        compiled,
                        output,
                        stack,
                        state,
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
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            vm.record_lvalue_trace_event("array-write-dim", *local, &dims);
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
        InstructionKind::AppendDim {
            dst,
            local,
            dims,
            value,
        } => {
            let _profile = vm.request_profile_operation_start(
                RequestProfileOperationCategory::Array,
                "dim_append",
            );
            let _clone_source = layout_source::enter(layout_source::ARRAY_ELEMENT_WRITE);
            let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims) {
                Ok(dims) => dims,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let dim_values: Vec<Value> = dims.iter().cloned().map(array_key_to_value).collect();
            match vm.try_userland_arrayaccess_offset_set_local(
                compiled,
                output,
                stack,
                state,
                *local,
                &dim_values,
                true,
                &value,
                instruction.span,
            ) {
                Ok(true) => {
                    vm.record_lvalue_trace_event("array-append-dim", *local, &dims);
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
                    return RichDispatchOutcome::Continue;
                }
                Ok(false) => {}
                Err(result) => return RichDispatchOutcome::Return(Box::new(result)),
            }
            if !is_globals_local(function, *local)
                && let Some(Value::Object(object)) =
                    read_local_value_at_frame(stack, frame_index, *local)
                        .map(|value| effective_value(&value))
                && spl_runtime_marker(&object)
                    .is_some_and(|class| is_spl_caching_iterator_class(&class))
            {
                let key = if dims.is_empty() {
                    Value::Null
                } else if dims.len() == 1 {
                    array_key_to_value(dims[0].clone())
                } else {
                    return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    "E_PHP_VM_SPL_CONTAINER_NESTED_DIM: nested ArrayAccess writes are not implemented"
                                        .to_owned(),
                                )));
                };
                if let Err(message) =
                    spl_caching_iterator_require_full_cache(&object, &object.display_name())
                        .and_then(|()| {
                            spl_caching_iterator_offset_set(&object, &key, value.clone())
                        })
                {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                vm.record_lvalue_trace_event("array-append-dim", *local, &dims);
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
                return RichDispatchOutcome::Continue;
            }
            if !is_globals_local(function, *local)
                && let Some(Value::Object(object)) =
                    read_local_value_at_frame(stack, frame_index, *local)
                        .map(|value| effective_value(&value))
                && spl_runtime_marker(&object)
                    .is_some_and(|class| is_spl_array_access_runtime_class(&class))
            {
                let key = if dims.is_empty() {
                    Value::Null
                } else if dims.len() == 1 {
                    array_key_to_value(dims[0].clone())
                } else {
                    return RichDispatchOutcome::Return(Box::new(vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    "E_PHP_VM_SPL_CONTAINER_NESTED_DIM: nested ArrayAccess writes are not implemented"
                                        .to_owned(),
                                )));
                };
                if let Err(message) = spl_container_offset_set(&object, key, value.clone()) {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
                vm.record_lvalue_trace_event("array-append-dim", *local, &dims);
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
                return RichDispatchOutcome::Continue;
            }
            let result = if is_globals_local(function, *local) {
                assign_globals_dim(&mut state.globals, &dims, value.clone(), true)
            } else {
                let was_packed = local_array_is_packed_fast_at_frame(stack, frame_index, *local);
                let cow_or_reference =
                    local_array_has_cow_or_reference_fallback_at_frame(stack, frame_index, *local);
                let result = assign_dim_local(stack, *local, &dims, value.clone(), true);
                if let Ok(path) = &result {
                    vm.record_counter_map_update_slot_path(*path, &dims, true);
                }
                let result = result.map(|_| ());
                if result.is_ok() && cow_or_reference {
                    vm.record_counter_cow_or_reference_fallback();
                }
                if result.is_ok()
                    && dims.is_empty()
                    && was_packed
                    && !cow_or_reference
                    && local_array_is_packed_fast_at_frame(stack, frame_index, *local)
                {
                    vm.record_counter_array_packed_append_fast_path_hit();
                }
                if result.is_ok()
                    && was_packed
                    && !local_array_is_packed_fast_at_frame(stack, frame_index, *local)
                {
                    vm.record_counter_array_packed_to_mixed_transition();
                }
                result
            };
            if let Err(message) = result {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            vm.record_lvalue_trace_event("array-append-dim", *local, &dims);
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
        InstructionKind::IssetDim { dst, local, dims } => {
            let _profile = vm.request_profile_operation_start(
                RequestProfileOperationCategory::Array,
                "dim_isset",
            );
            let _clone_source = layout_source::enter(layout_source::ARRAY_ELEMENT_READ);
            let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims) {
                Ok(dims) => dims,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            vm.record_counter_local_slot_fast_path(local_slot_is_in_bounds(stack, *local));
            match vm.try_userland_arrayaccess_offset_exists_local(
                compiled,
                output,
                stack,
                state,
                *local,
                &dims,
                instruction.span,
            ) {
                Ok(Some(result)) => {
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame was pushed")
                        .registers
                        .set(*dst, Value::Bool(result))
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Ok(None) => {}
                Err(result) => return RichDispatchOutcome::Return(Box::new(result)),
            }
            let local_value = if is_globals_local(function, *local) {
                Some(Value::Array(state.globals.globals_array()))
            } else {
                read_local_value_at_frame(stack, frame_index, *local)
            };
            let value = if let Some((object, key_value)) = local_value
                .as_ref()
                .and_then(|value| spl_array_access_dim_target(value, &dims))
            {
                let exists = match vm.call_array_access_dim_method(
                    compiled,
                    object,
                    "offsetExists",
                    key_value,
                    Some(instruction.span),
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
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                match to_bool(&exists) {
                    Ok(true) => Some(Value::Bool(true)),
                    Ok(false) => None,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                }
            } else if let (Some(base_value), [key]) = (local_value.as_ref(), dims.as_slice()) {
                let base = effective_value(base_value);
                if let Value::Array(array) = &base {
                    match vm.try_array_shape_lookup(array, key) {
                        Some(value) => value,
                        None => fetch_dim_path_value(base_value, &dims).ok().flatten(),
                    }
                } else {
                    fetch_dim_path_value(base_value, &dims).ok().flatten()
                }
            } else {
                local_value.and_then(|value| fetch_dim_path_value(&value, &dims).ok().flatten())
            };
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame was pushed")
                .registers
                .set(
                    *dst,
                    Value::Bool(!matches!(value, None | Some(Value::Null))),
                )
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::EmptyDim { dst, local, dims } => {
            let _profile = vm.request_profile_operation_start(
                RequestProfileOperationCategory::Array,
                "dim_empty",
            );
            let _clone_source = layout_source::enter(layout_source::ARRAY_ELEMENT_READ);
            let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims) {
                Ok(dims) => dims,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            vm.record_counter_local_slot_fast_path(local_slot_is_in_bounds(stack, *local));
            match vm.try_userland_arrayaccess_offset_empty_local(
                compiled,
                output,
                stack,
                state,
                *local,
                &dims,
                instruction.span,
            ) {
                Ok(Some(result)) => {
                    if let Err(message) = stack
                        .frame_mut(frame_index)
                        .expect("frame was pushed")
                        .registers
                        .set(*dst, Value::Bool(result))
                    {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                    return RichDispatchOutcome::Continue;
                }
                Ok(None) => {}
                Err(result) => return RichDispatchOutcome::Return(Box::new(result)),
            }
            let local_value = if is_globals_local(function, *local) {
                Some(Value::Array(state.globals.globals_array()))
            } else {
                read_local_value_at_frame(stack, frame_index, *local)
            };
            let value = if let Some((object, key_value)) = local_value
                .as_ref()
                .and_then(|value| spl_array_access_dim_target(value, &dims))
            {
                let exists = match vm.call_array_access_dim_method(
                    compiled,
                    object.clone(),
                    "offsetExists",
                    key_value.clone(),
                    Some(instruction.span),
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
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                };
                let exists = match to_bool(&exists) {
                    Ok(value) => value,
                    Err(message) => {
                        return RichDispatchOutcome::Return(Box::new(
                            vm.runtime_error(output, compiled, stack, message),
                        ));
                    }
                };
                if exists {
                    match vm.call_array_access_dim_method(
                        compiled,
                        object,
                        "offsetGet",
                        key_value,
                        Some(instruction.span),
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
                                    return RichDispatchOutcome::Return(result);
                                }
                            }
                        }
                    }
                } else {
                    Value::Uninitialized
                }
            } else {
                local_value
                    .and_then(|value| fetch_dim_path_value(&value, &dims).ok().flatten())
                    .unwrap_or(Value::Uninitialized)
            };
            let result = match php_empty_access_value(&value) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame was pushed")
                .registers
                .set(*dst, Value::Bool(result))
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        InstructionKind::UnsetDim { local, dims } => {
            let _profile = vm.request_profile_operation_start(
                RequestProfileOperationCategory::Array,
                "dim_unset",
            );
            let _clone_source = layout_source::enter(layout_source::ARRAY_ELEMENT_WRITE);
            let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims) {
                Ok(dims) => dims,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            vm.record_counter_local_slot_fast_path(local_slot_is_in_bounds(stack, *local));
            let was_packed = if is_globals_local(function, *local) {
                false
            } else {
                local_array_is_packed_fast_at_frame(stack, frame_index, *local)
            };
            match vm.try_userland_arrayaccess_offset_unset_local(
                compiled,
                output,
                stack,
                state,
                *local,
                &dims,
                instruction.span,
            ) {
                Ok(true) => {
                    vm.record_lvalue_trace_event("array-unset-dim", *local, &dims);
                    return RichDispatchOutcome::Continue;
                }
                Ok(false) => {}
                Err(result) => return RichDispatchOutcome::Return(Box::new(result)),
            }
            let local_value = if is_globals_local(function, *local) {
                Some(Value::Array(state.globals.globals_array()))
            } else {
                read_local_value_at_frame(stack, frame_index, *local)
            };
            if let Some((object, key_value)) = local_value
                .as_ref()
                .and_then(|value| spl_array_access_dim_target(value, &dims))
            {
                match vm.call_array_access_dim_method(
                    compiled,
                    object,
                    "offsetUnset",
                    key_value,
                    Some(instruction.span),
                    output,
                    stack,
                    state,
                ) {
                    Ok(_) => {
                        vm.record_lvalue_trace_event("array-unset-dim", *local, &dims);
                        return RichDispatchOutcome::Continue;
                    }
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
                                return RichDispatchOutcome::Return(result);
                            }
                        }
                    }
                }
            }
            let result = if is_globals_local(function, *local) {
                unset_globals_dim(&mut state.globals, &dims)
            } else {
                unset_dim_local(stack, *local, &dims)
            };
            if let Err(message) = result {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
            if !is_globals_local(function, *local)
                && was_packed
                && !local_array_is_packed_fast_at_frame(stack, frame_index, *local)
            {
                vm.record_counter_array_packed_to_mixed_transition();
            }
            vm.record_lvalue_trace_event("array-unset-dim", *local, &dims);
        }
        _ => unreachable!("array dispatch received a non-array instruction"),
    }

    RichDispatchOutcome::Continue
}
