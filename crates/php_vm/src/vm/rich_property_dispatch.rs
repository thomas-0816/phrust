macro_rules! execute_rich_property_instruction {
    (
        $vm:expr,
        $compiled:ident,
        $unit:ident,
        $function_id:ident,
        $block_id:ident,
        $instruction:ident,
        $frame_index:ident,
        $output:ident,
        $stack:ident,
        $state:ident,
        $diagnostics:ident,
        $exception_handlers:ident,
        $pending_control:ident,
        $dispatch:lifetime
    ) => {{
        let compiled = $compiled;
        let unit = $unit;
        let function_id = $function_id;
        let instruction = $instruction;
        let frame_index = $frame_index;
        let output = &mut *$output;
        let stack = &mut *$stack;
        let state = &mut *$state;

        match &instruction.kind {
                    InstructionKind::FetchProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let _profile = $vm.request_profile_operation_start(
                            RequestProfileOperationCategory::Object,
                            "property_fetch",
                        );
                        let _clone_source =
                            layout_source::enter(layout_source::OBJECT_PROPERTY_READ);
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let receiver_type = value_type_name(&other);
                                if let Err(result) = $vm.emit_non_object_property_read_warning(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $diagnostics,
                                    receiver_type,
                                    property,
                                    instruction.span,
                                ) {
                                    return result;
                                }
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Null)
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if spl_array_object_uses_array_as_props(&object) {
                            let value = match spl_container_offset_get(
                                &object,
                                &Value::String(PhpString::from_test_str(property)),
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if internal_throwable_instanceof(&object.class_name_handle(), "throwable")
                            .is_some()
                        {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_php_token_runtime_class(&object.class_name()) {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(&object.class_name()) {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_date_time_runtime_class(&object.class_name()) {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if normalize_class_name(&object.class_name()) == "simplexmlelement" {
                            let value = php_runtime::api::xml::simplexml_property(&object, property);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_pdo_runtime_class(&object.class_name()) {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
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
                        let scope = current_scope_class(compiled, stack);
                        let normalized_scope = scope.as_deref().map(normalize_class_name);
                        let receiver_class = normalize_class_name(&object.class_name());
                        let lookup_epoch = state.lookup_epoch();
                        let property_callsite = property_fetch_callsite(
                            compiled,
                            function_id,
                            $block_id,
                            instruction.id,
                        );
                        let receiver_has_magic_get = class_has_public_magic_get(compiled, &class);
                        if let Some(target) = $vm.lookup_property_fetch_inline_cache(
                            compiled,
                            function_id,
                            $block_id,
                            instruction.id,
                            property,
                            &receiver_class,
                            normalized_scope.as_deref(),
                            lookup_epoch,
                        ) {
                            match $vm
                                .read_property_fetch_target(compiled, target, &object, stack, state)
                            {
                                Ok(PropertyFetchCacheRead::Value(value)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(PropertyFetchCacheRead::Fallback) => {}
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                if let Some(value) = object.get_property(property) {
                                    $vm.record_counter_property_fetch_profile(
                                        property_fetch_profile_observation(
                                            &property_callsite,
                                            property,
                                            &receiver_class,
                                            &class,
                                            None,
                                            normalized_scope.as_deref(),
                                            lookup_epoch,
                                            receiver_has_magic_get,
                                            false,
                                            true,
                                            false,
                                            false,
                                            Vec::new(),
                                        ),
                                    );
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                $vm.record_counter_property_fetch_profile(
                                    property_fetch_profile_observation(
                                        &property_callsite,
                                        property,
                                        &receiver_class,
                                        &class,
                                        None,
                                        normalized_scope.as_deref(),
                                        lookup_epoch,
                                        receiver_has_magic_get,
                                        false,
                                        false,
                                        false,
                                        false,
                                        Vec::new(),
                                    ),
                                );
                                match $vm.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__get",
                                    property,
                                    vec![CallArgument::positional(Value::String(
                                        PhpString::from_test_str(property),
                                    ))],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(value)) => {
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return result,
                                }
                                if let Err(result) = $vm.emit_undefined_property_warning(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $diagnostics,
                                    &object.display_name(),
                                    property,
                                    instruction.span,
                                ) {
                                    return result;
                                }
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Null)
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let resolved_class = &resolved.class;
                        let resolved_property = &resolved.property;
                        if resolved.property.flags.is_static {
                            if let Err(access_error) = validate_property_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                resolved_property,
                            ) {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    access_error,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            emit_static_property_as_non_static_notice(
                                compiled,
                                output,
                                stack,
                                state,
                                resolved_class,
                                resolved_property,
                                instruction.span,
                            );
                            let value = match object.get_property(property) {
                                Some(value) => value,
                                None => {
                                    if let Err(result) = $vm.emit_undefined_property_warning(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut $diagnostics,
                                        resolved_class.display_name.as_str(),
                                        property,
                                        instruction.span,
                                    ) {
                                        return result;
                                    }
                                    Value::Null
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if let Err(access_error) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            resolved_class,
                            resolved_property,
                        ) {
                            $vm.record_counter_property_fetch_profile(
                                property_fetch_profile_observation(
                                    &property_callsite,
                                    property,
                                    &receiver_class,
                                    &class,
                                    Some((resolved_class, resolved_property)),
                                    normalized_scope.as_deref(),
                                    lookup_epoch,
                                    receiver_has_magic_get,
                                    property_has_hooks_or_active(
                                        state,
                                        &object,
                                        resolved_class,
                                        resolved_property,
                                    ),
                                    false,
                                    false,
                                    false,
                                    vec!["not_visible"],
                                ),
                            );
                            match $vm.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__get",
                                property,
                                vec![CallArgument::positional(Value::String(
                                    PhpString::from_test_str(property),
                                ))],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(value)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {
                                    if resolved.property.flags.is_private
                                        && normalize_class_name(&class.name)
                                            != normalize_class_name(&resolved_class.name)
                                        && let Some(value) = object.get_property(property)
                                    {
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    let result =
                                        $vm.runtime_error(output, compiled, stack, access_error);
                                    if let Some(throwable) = runtime_error_throwable(&result) {
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
                                            &mut $exception_handlers,
                                            &mut $pending_control,
                                        ) {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        return $vm
                                            .propagate_exception(output, stack, state, throwable);
                                    }
                                    return result;
                                }
                                Err(result) => return result,
                            }
                        }
                        let resolved_has_property_hook = property_has_hooks_or_active(
                            state,
                            &object,
                            resolved_class,
                            resolved_property,
                        );
                        if !property_hook_is_active(
                            state,
                            &object,
                            resolved_class,
                            resolved_property,
                        ) && let Some(function) = resolved.property.hooks.get
                        {
                            $vm.record_counter_property_fetch_profile(
                                property_fetch_profile_observation(
                                    &property_callsite,
                                    property,
                                    &receiver_class,
                                    &class,
                                    Some((resolved_class, resolved_property)),
                                    normalized_scope.as_deref(),
                                    lookup_epoch,
                                    receiver_has_magic_get,
                                    resolved_has_property_hook,
                                    false,
                                    true,
                                    false,
                                    Vec::new(),
                                ),
                            );
                            match $vm.call_property_hook(
                                compiled,
                                object.clone(),
                                resolved_class,
                                resolved_property,
                                function,
                                Vec::new(),
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
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Err(result) => return result,
                            }
                        }
                        let storage_name = property_storage_name(resolved_class, resolved_property);
                        let value = match object.get_property(&storage_name) {
                            Some(value) => value,
                            None => {
                                $vm.record_counter_property_fetch_profile(
                                    property_fetch_profile_observation(
                                        &property_callsite,
                                        property,
                                        &receiver_class,
                                        &class,
                                        Some((resolved_class, resolved_property)),
                                        normalized_scope.as_deref(),
                                        lookup_epoch,
                                        receiver_has_magic_get,
                                        resolved_has_property_hook,
                                        false,
                                        true,
                                        false,
                                        vec!["missing_declared_storage"],
                                    ),
                                );
                                match $vm.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__get",
                                    property,
                                    vec![CallArgument::positional(Value::String(
                                        PhpString::from_test_str(property),
                                    ))],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(value)) => {
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return result,
                                }
                                if let Err(result) = $vm.emit_undefined_property_warning(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $diagnostics,
                                    &object.display_name(),
                                    property,
                                    instruction.span,
                                ) {
                                    return result;
                                }
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Null)
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                        };
                        if matches!(value, Value::Uninitialized) {
                            $vm.record_counter_property_fetch_profile(
                                property_fetch_profile_observation(
                                    &property_callsite,
                                    property,
                                    &receiver_class,
                                    &class,
                                    Some((resolved_class, resolved_property)),
                                    normalized_scope.as_deref(),
                                    lookup_epoch,
                                    receiver_has_magic_get,
                                    resolved_has_property_hook,
                                    false,
                                    true,
                                    true,
                                    Vec::new(),
                                ),
                            );
                            let message = format!(
                                "E_PHP_VM_UNINITIALIZED_PROPERTY: Typed property {}::${property} must not be accessed before initialization",
                                resolved.class.display_name
                            );
                            match $vm.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut $exception_handlers,
                                &mut $pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        $vm.record_counter_property_fetch_profile(
                            property_fetch_profile_observation(
                                &property_callsite,
                                property,
                                &receiver_class,
                                &class,
                                Some((resolved_class, resolved_property)),
                                normalized_scope.as_deref(),
                                lookup_epoch,
                                receiver_has_magic_get,
                                resolved_has_property_hook,
                                false,
                                true,
                                false,
                                Vec::new(),
                            ),
                        );
                        $vm.maybe_install_property_fetch_inline_cache_target(
                            compiled,
                            function_id,
                            $block_id,
                            instruction.id,
                            property,
                            &receiver_class,
                            &class,
                            resolved_class,
                            resolved_property,
                            &storage_name,
                            normalized_scope.as_deref(),
                            lookup_epoch,
                            receiver_has_magic_get,
                            state,
                            &object,
                            None,
                        );
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::FetchStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => {
                        match $vm.fetch_static_property_value(
                            compiled,
                            class_name,
                            property,
                            Some((function_id, $block_id, instruction.id)),
                            None,
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
                                match $vm.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    *result,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(ClassConstantFetch::Raise(span, message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    span,
                                    message,
                                ) {
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
                    InstructionKind::FetchDynamicStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => {
                        let class_name_value =
                            match read_operand_at_frame(unit, stack, frame_index, *class_name) {
                                Ok(value) => value,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let class_name =
                            match dynamic_static_class_name_from_value(&class_name_value) {
                                Ok(name) => name,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                        if let Err(result) = $vm.autoload_static_class_if_missing(
                            compiled,
                            &class_name,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                $block_id,
                                instruction.id,
                            )),
                            output,
                            stack,
                            state,
                        ) {
                            match $vm.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut $exception_handlers,
                                &mut $pending_control,
                                result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        let class =
                            match resolve_static_class_name(compiled, state, stack, &class_name) {
                                Ok(class) => class,
                                Err(message) => {
                                    match $vm.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut $exception_handlers,
                                        &mut $pending_control,
                                        instruction.span,
                                        message,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                        let scope = current_scope_class(compiled, stack);
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                let message = format!(
                                    "E_PHP_VM_UNKNOWN_STATIC_PROPERTY: Access to undeclared static property {}::${property}",
                                    class.display_name
                                );
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if !resolved.property.flags.is_static {
                            return $vm.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is not static",
                                    resolved.class.name, resolved.property.name
                                ),
                            );
                        }
                        if let Err(message) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            &resolved.class,
                            &resolved.property,
                        ) {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                        let key = static_property_key(&resolved.class, &resolved.property);
                        if !state.static_properties.contains_key(&key) {
                            let default = match static_property_default(
                                compiled,
                                state,
                                stack,
                                &resolved.class,
                                &resolved.property,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            state.static_properties.insert(key.clone(), default);
                        }
                        let value = state
                            .static_properties
                            .get(&key)
                            .cloned()
                            .unwrap_or(Value::Uninitialized);
                        if matches!(value, Value::Uninitialized) {
                            let message = format!(
                                "E_PHP_VM_UNINITIALIZED_STATIC_PROPERTY: typed static property {}::${} must not be accessed before initialization",
                                resolved.class.display_name, resolved.property.name
                            );
                            match $vm.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut $exception_handlers,
                                &mut $pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::IssetStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => {
                        let result = match static_property_isset_empty_result(
                            $vm,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            false,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                $block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(result) => result,
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match $vm.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    *result,
                                ) {
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
                            .set(*dst, Value::Bool(result))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => {
                        let result = match static_property_isset_empty_result(
                            $vm,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            true,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                $block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(result) => result,
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match $vm.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    *result,
                                ) {
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
                            .set(*dst, Value::Bool(result))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::IssetStaticPropertyDim {
                        dst,
                        class_name,
                        property,
                        dims,
                    } => {
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let result = match static_property_dim_isset_empty_result(
                            $vm,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            &dims,
                            false,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                $block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(result) => result,
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match $vm.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    *result,
                                ) {
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
                            .set(*dst, Value::Bool(result))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyStaticPropertyDim {
                        dst,
                        class_name,
                        property,
                        dims,
                    } => {
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let result = match static_property_dim_isset_empty_result(
                            $vm,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            &dims,
                            true,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                $block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(result) => result,
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match $vm.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    *result,
                                ) {
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
                            .set(*dst, Value::Bool(result))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::FetchClassConstant {
                        dst,
                        class_name,
                        constant,
                    } => {
                        match $vm.fetch_class_constant_value(
                            compiled,
                            class_name,
                            constant,
                            Some((function_id, $block_id, instruction.id)),
                            None,
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
                                match $vm.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    *result,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(ClassConstantFetch::Raise(span, message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    span,
                                    message,
                                ) {
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
                    InstructionKind::FetchDynamicProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let property = match $vm.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                let receiver_type = value_type_name(&other);
                                if let Err(result) = $vm.emit_non_object_property_read_warning(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $diagnostics,
                                    receiver_type,
                                    &property,
                                    instruction.span,
                                ) {
                                    return result;
                                }
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Null)
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match $vm
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        if spl_array_object_uses_array_as_props(&object) {
                            let value = match spl_container_offset_get(
                                &object,
                                &Value::String(PhpString::from_test_str(&property)),
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if normalize_class_name(&object.class_name()) == "simplexmlelement" {
                            let value = php_runtime::api::xml::simplexml_property(&object, &property);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if let Some(class) =
                            lookup_class_in_state(compiled, state, &object.class_name())
                        {
                            let scope = current_scope_class(compiled, stack);
                            match lookup_resolved_property_in_state(
                                compiled,
                                state,
                                &class,
                                &property,
                                scope.as_deref(),
                            ) {
                                Ok(Some(resolved)) => {
                                    if let Err(access_error) = validate_property_access_in_state(
                                        compiled,
                                        state,
                                        stack,
                                        &resolved.class,
                                        &resolved.property,
                                    ) {
                                        match $vm.call_magic_property_method(
                                            compiled,
                                            object.clone(),
                                            "__get",
                                            &property,
                                            vec![CallArgument::positional(Value::String(
                                                PhpString::from_test_str(&property),
                                            ))],
                                            output,
                                            stack,
                                            state,
                                        ) {
                                            Ok(Some(value)) => {
                                                if let Err(message) = stack
                                                    .frame_mut(frame_index)
                                                    .expect("frame was pushed")
                                                    .registers
                                                    .set(*dst, value)
                                                {
                                                    return $vm.runtime_error(
                                                        output, compiled, stack, message,
                                                    );
                                                }
                                                continue;
                                            }
                                            Ok(None) => {
                                                match $vm.raise_runtime_error(
                                                    compiled,
                                                    output,
                                                    stack,
                                                    state,
                                                    &mut $exception_handlers,
                                                    &mut $pending_control,
                                                    instruction.span,
                                                    access_error,
                                                ) {
                                                    RaiseOutcome::Caught(target) => {
                                                        $block_id = target;
                                                        continue $dispatch;
                                                    }
                                                    RaiseOutcome::Done(result) => return *result,
                                                }
                                            }
                                            Err(result) => return result,
                                        }
                                    }
                                }
                                Ok(None) => {}
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                        let value = match $vm
                            .property_state_value(compiled, state, stack, &object, &property)
                        {
                            Some(value) => value,
                            None => match $vm.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__get",
                                &property,
                                vec![CallArgument::positional(Value::String(
                                    PhpString::from_test_str(&property),
                                ))],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(value)) => value,
                                Ok(None) => object.get_property(&property).unwrap_or(Value::Null),
                                Err(result) => return result,
                            },
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::IssetProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = match $vm
                            .isset_property_value(compiled, &object, property, output, stack, state)
                        {
                            Ok(value) => value,
                            Err(result) => return result,
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::IssetDynamicProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let property = match $vm.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Bool(false))
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                let _ = (other, property);
                                continue;
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match $vm
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let value =
                            $vm.property_state_value(compiled, state, stack, &object, &property);
                        let result = if let Some(value) = value {
                            !matches!(value, Value::Uninitialized | Value::Null)
                        } else {
                            match $vm.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__isset",
                                &property,
                                vec![CallArgument::positional(Value::String(
                                    PhpString::from_test_str(&property),
                                ))],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(value)) => match to_bool(&value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                },
                                Ok(None) => false,
                                Err(result) => return result,
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Bool(result))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = match $vm
                            .empty_property_value(compiled, &object, property, output, stack, state)
                        {
                            Ok(value) => value,
                            Err(result) => return result,
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyDynamicProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let property = match $vm.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Bool(true))
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                let _ = (other, property);
                                continue;
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match $vm
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let result = match $vm
                            .property_state_value(compiled, state, stack, &object, &property)
                        {
                            Some(value) => match php_empty_access_value(&value) {
                                Ok(value) => value,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            },
                            None => {
                                let isset = match $vm.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__isset",
                                    &property,
                                    vec![CallArgument::positional(Value::String(
                                        PhpString::from_test_str(&property),
                                    ))],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(value)) => match to_bool(&value) {
                                        Ok(value) => value,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    },
                                    Ok(None) => false,
                                    Err(result) => return result,
                                };
                                if !isset {
                                    true
                                } else {
                                    match $vm.call_magic_property_method(
                                        compiled,
                                        object.clone(),
                                        "__get",
                                        &property,
                                        vec![CallArgument::positional(Value::String(
                                            PhpString::from_test_str(&property),
                                        ))],
                                        output,
                                        stack,
                                        state,
                                    ) {
                                        Ok(Some(value)) => match php_empty_access_value(&value) {
                                            Ok(value) => value,
                                            Err(message) => {
                                                return $vm.runtime_error(
                                                    output, compiled, stack, message,
                                                );
                                            }
                                        },
                                        Ok(None) => true,
                                        Err(result) => return result,
                                    }
                                }
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Bool(result))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::IssetDynamicPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => Some(object),
                            Ok(_) => None,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match $vm
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = object.as_ref().and_then(|object| {
                            $vm.property_state_value(compiled, state, stack, object, &property)
                                .and_then(|value| {
                                    fetch_dim_path_value(&value, &dims).ok().flatten()
                                })
                        });
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(
                                *dst,
                                Value::Bool(!matches!(value, None | Some(Value::Null))),
                            )
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyDynamicPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => Some(object),
                            Ok(_) => None,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match $vm
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = object
                            .as_ref()
                            .and_then(|object| {
                                $vm.property_state_value(compiled, state, stack, object, &property)
                                    .and_then(|value| {
                                        fetch_dim_path_value(&value, &dims).ok().flatten()
                                    })
                            })
                            .unwrap_or(Value::Uninitialized);
                        let result = match php_empty_access_value(&value) {
                            Ok(value) => value,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Bool(result))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::IssetPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Bool(false))
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                let _ = other;
                                continue;
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        // Borrowed probe: isset must not clone the property
                        // container (the clone shares the array handle and
                        // forces a full copy-on-write separation on the next
                        // write to the same registry-style array).
                        let borrowed = $vm
                            .with_property_state_value(
                                compiled,
                                state,
                                stack,
                                &object,
                                property,
                                &mut |value| match value {
                                    Some(value) => {
                                        with_borrowed_dim_path(value, &dims, &mut |leaf| {
                                            !matches!(leaf, None | Some(Value::Null))
                                        })
                                    }
                                    None => Some(false),
                                },
                            )
                            .flatten();
                        let result = match borrowed {
                            Some(result) => {
                                $vm.record_counter_property_dim_probe_borrowed_hit();
                                result
                            }
                            None => {
                                let value = $vm
                                    .property_state_value(compiled, state, stack, &object, property)
                                    .and_then(|value| {
                                        fetch_dim_path_value(&value, &dims).ok().flatten()
                                    });
                                !matches!(value, None | Some(Value::Null))
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Bool(result))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Bool(true))
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                let _ = other;
                                continue;
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        // Borrowed probe mirroring the isset arm: empty()
                        // only needs a borrowed view of the leaf value.
                        let borrowed = $vm
                            .with_property_state_value(
                                compiled,
                                state,
                                stack,
                                &object,
                                property,
                                &mut |value| match value {
                                    Some(value) => {
                                        with_borrowed_dim_path(value, &dims, &mut |leaf| {
                                            php_empty_access_value(
                                                leaf.unwrap_or(&Value::Uninitialized),
                                            )
                                        })
                                    }
                                    None => Some(php_empty_access_value(&Value::Uninitialized)),
                                },
                            )
                            .flatten();
                        let result = match borrowed {
                            Some(result) => {
                                $vm.record_counter_property_dim_probe_borrowed_hit();
                                result
                            }
                            None => {
                                let value = $vm
                                    .property_state_value(compiled, state, stack, &object, property)
                                    .and_then(|value| {
                                        fetch_dim_path_value(&value, &dims).ok().flatten()
                                    })
                                    .unwrap_or(Value::Uninitialized);
                                php_empty_access_value(&value)
                            }
                        };
                        let result = match result {
                            Ok(value) => value,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Bool(result))
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::UnsetProperty { object, property } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot unset property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        match $vm.unset_property_value(
                            compiled,
                            &object,
                            property,
                            instruction.span,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(()) => {}
                            Err(StaticPropertyAssignError::Vm(result)) => return *result,
                            Err(StaticPropertyAssignError::Raise(span, message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyAssignError::Fatal(message)) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        }
                    }
                    InstructionKind::UnsetPropertyDim {
                        object,
                        property,
                        dims,
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
                                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot unset property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) =
                            unset_property_dim(compiled, state, stack, &object, property, &dims)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::UnsetDynamicProperty { object, property } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let property = match $vm.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot unset property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match $vm
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let class = compiled.lookup_class(&object.class_name());
                        let scope = current_scope_class(compiled, stack);
                        let declared = match class {
                            Some(class) => match lookup_property_in_hierarchy(
                                compiled,
                                class,
                                &property,
                                scope.as_deref(),
                            ) {
                                Ok(property) => property,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            },
                            None => None,
                        };
                        if let Some(resolved) = declared {
                            if let Err(message) = validate_property_access(
                                compiled,
                                stack,
                                resolved.class,
                                resolved.property,
                            ) {
                                match $vm.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__unset",
                                    &property,
                                    vec![CallArgument::positional(Value::String(
                                        PhpString::from_test_str(&property),
                                    ))],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(_)) => {}
                                    Ok(None) => {
                                        match $vm.raise_runtime_error(
                                            compiled,
                                            output,
                                            stack,
                                            state,
                                            &mut $exception_handlers,
                                            &mut $pending_control,
                                            instruction.span,
                                            message,
                                        ) {
                                            RaiseOutcome::Caught(target) => {
                                                $block_id = target;
                                                continue $dispatch;
                                            }
                                            RaiseOutcome::Done(result) => return *result,
                                        }
                                    }
                                    Err(result) => return result,
                                }
                                continue;
                            }
                            let storage_name =
                                property_storage_name(resolved.class, resolved.property);
                            if resolved.property.flags.is_typed {
                                object.set_property(storage_name, Value::Uninitialized);
                            } else {
                                object.unset_property(&storage_name);
                            }
                        } else {
                            match $vm.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__unset",
                                &property,
                                vec![CallArgument::positional(Value::String(
                                    PhpString::from_test_str(&property),
                                ))],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) | Ok(None) => {
                                    object.unset_property(&property);
                                }
                                Err(result) => return result,
                            }
                        }
                    }
                    InstructionKind::FetchObjectClassName { dst, object } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object) {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    format!(
                                        "E_PHP_VM_DYNAMIC_CLASS_NAME_TYPE: Cannot use \"::class\" on {}",
                                        value_type_name(&other)
                                    ),
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(
                                *dst,
                                Value::String(PhpString::from_test_str(&object.display_name())),
                            )
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::AssignProperty {
                        dst,
                        object,
                        property,
                        value,
                    } => {
                        let _profile = $vm.request_profile_operation_start(
                            RequestProfileOperationCategory::Object,
                            "property_assign",
                        );
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object) {
                            Ok(Value::Object(object)) => object,
                            Ok(Value::Callable(_)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    format!(
                                        "E_PHP_VM_DYNAMIC_PROPERTY_ERROR: Cannot create dynamic property Closure::${property}"
                                    ),
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Ok(other) => {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_ASSIGN_NON_OBJECT: cannot assign property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if spl_array_object_uses_array_as_props(&object) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            if let Err(message) = spl_container_offset_set(
                                &object,
                                Value::String(PhpString::from_test_str(property)),
                                value.clone(),
                            ) {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(&object.class_name()) {
                            $vm.record_counter_property_assign_ic_fallback(
                                "dynamic_property_fallback",
                            );
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            object.set_property(property, value.clone());
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
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
                        let scope = current_scope_class(compiled, stack);
                        let normalized_scope = scope.as_deref().map(normalize_class_name);
                        let receiver_class = normalize_class_name(&object.class_name());
                        let lookup_epoch = state.lookup_epoch();
                        let receiver_has_magic_set = class_has_public_magic_set(compiled, &class);
                        if let Some(target) = $vm.lookup_property_assign_inline_cache(
                            compiled,
                            function_id,
                            $block_id,
                            instruction.id,
                            property,
                            &receiver_class,
                            normalized_scope.as_deref(),
                            lookup_epoch,
                        ) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            match $vm.write_property_assign_target(
                                compiled, target, &object, value, stack, state,
                            ) {
                                Ok(PropertyAssignCacheWrite::Written(value)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(PropertyAssignCacheWrite::Fallback) => {}
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                $vm.record_counter_property_assign_ic_fallback(
                                    "dynamic_property_fallback",
                                );
                                let value =
                                    match read_operand_at_frame(unit, stack, frame_index, *value) {
                                        Ok(value) => value,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                match $vm.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__set",
                                    property,
                                    vec![
                                        CallArgument::positional(Value::String(
                                            PhpString::from_test_str(property),
                                        )),
                                        CallArgument::positional(value.clone()),
                                    ],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(_)) => {
                                        $vm.record_counter_property_assign_ic_fallback(
                                            "magic_set_metadata",
                                        );
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return result,
                                }
                                if let Some(diagnostic) = dynamic_property_deprecation_diagnostic(
                                    compiled,
                                    state,
                                    &class,
                                    &object,
                                    property.as_ref(),
                                    stack,
                                ) {
                                    $diagnostics.push(diagnostic);
                                }
                                object.set_property(property, value.clone());
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let resolved_class = &resolved.class;
                        let entry = &resolved.property;
                        if entry.flags.is_static {
                            if let Err(message) = validate_property_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                entry,
                            )
                            .and_then(|()| {
                                validate_property_set_access_in_state(
                                    compiled,
                                    state,
                                    stack,
                                    resolved_class,
                                    entry,
                                )
                            }) {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            emit_static_property_as_non_static_notice(
                                compiled,
                                output,
                                stack,
                                state,
                                resolved_class,
                                entry,
                                instruction.span,
                            );
                            object.set_property(property, value.clone());
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if let Err(message) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            resolved_class,
                            entry,
                        )
                        .and_then(|()| {
                            validate_property_set_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                entry,
                            )
                        }) {
                            $vm.record_counter_property_assign_ic_fallback("visibility_mismatch");
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            match $vm.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__set",
                                property,
                                vec![
                                    CallArgument::positional(Value::String(
                                        PhpString::from_test_str(property),
                                    )),
                                    CallArgument::positional(value.clone()),
                                ],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) => {
                                    $vm.record_counter_property_assign_ic_fallback(
                                        "magic_set_metadata",
                                    );
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {
                                    if entry.flags.is_private
                                        && normalize_class_name(&class.name)
                                            != normalize_class_name(&resolved_class.name)
                                    {
                                        if let Some(diagnostic) =
                                            dynamic_property_deprecation_diagnostic(
                                                compiled,
                                                state,
                                                &class,
                                                &object,
                                                property.as_ref(),
                                                stack,
                                            )
                                        {
                                            $diagnostics.push(diagnostic);
                                        }
                                        object.set_property(property, value.clone());
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    match $vm.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut $exception_handlers,
                                        &mut $pending_control,
                                        instruction.span,
                                        message,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                                Err(result) => return result,
                            }
                        }
                        let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                            Ok(value) => value,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property_type = ir_runtime_type(entry.type_.as_ref());
                        if let Err(message) = check_property_type(
                            compiled,
                            Some(state),
                            resolved.class.display_name.as_str(),
                            property,
                            &property_type,
                            &value,
                            $vm.typecheck_fast_path_context(),
                        ) {
                            $vm.record_counter_property_assign_ic_fallback("type_mismatch");
                            match $vm.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut $exception_handlers,
                                &mut $pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if let Err(message) =
                            validate_property_write(resolved_class, entry, &object, stack, compiled)
                        {
                            $vm.record_counter_property_assign_ic_fallback("readonly_property");
                            match $vm.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut $exception_handlers,
                                &mut $pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if !property_hook_is_active(state, &object, resolved_class, entry)
                            && let Some(function) = entry.hooks.set
                        {
                            $vm.record_counter_property_assign_ic_fallback(
                                "property_hook_present",
                            );
                            match $vm.call_property_hook(
                                compiled,
                                object.clone(),
                                resolved_class,
                                entry,
                                function,
                                vec![CallArgument::positional(value.clone())],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(_) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Err(result) => return result,
                            }
                        }
                        if !entry.hooks.backed
                            && (entry.hooks.get.is_some() || entry.hooks.set.is_some())
                        {
                            $vm.record_counter_property_assign_ic_fallback(
                                "property_hook_present",
                            );
                            return $vm.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_VIRTUAL_PROPERTY_WRITE: property {}::${} has no backing storage",
                                    resolved_class.name, entry.name
                                ),
                            );
                        }
                        let storage_name = property_storage_name(resolved_class, entry);
                        if !entry.flags.is_typed
                            && object.get_property(&storage_name).is_none()
                            && !magic_property_call_is_active(state, &object, "__set", property)
                        {
                            match $vm.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__set",
                                property,
                                vec![
                                    CallArgument::positional(Value::String(
                                        PhpString::from_test_str(property),
                                    )),
                                    CallArgument::positional(value.clone()),
                                ],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {}
                                Err(result) => return result,
                            }
                        }
                        if matches!(
                            object.get_property(&storage_name),
                            Some(Value::Reference(_))
                        ) {
                            $vm.record_counter_property_assign_ic_fallback("reference_slot");
                        }
                        write_property_storage_value(&object, &storage_name, value.clone());
                        $vm.maybe_install_property_assign_inline_cache_target(
                            compiled,
                            function_id,
                            $block_id,
                            instruction.id,
                            property,
                            &receiver_class,
                            &class,
                            resolved_class,
                            entry,
                            &storage_name,
                            normalized_scope.as_deref(),
                            lookup_epoch,
                            receiver_has_magic_set,
                            state,
                            &object,
                            None,
                        );
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::AssignDynamicProperty {
                        dst,
                        object,
                        property,
                        value,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(Value::Callable(_)) => {
                                let property = match $vm.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    format!(
                                        "E_PHP_VM_DYNAMIC_PROPERTY_ERROR: Cannot create dynamic property Closure::${property}"
                                    ),
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Ok(other) => {
                                let property = match $vm.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_ASSIGN_NON_OBJECT: cannot assign property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match $vm
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        if spl_array_object_uses_array_as_props(&object) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            if let Err(message) = spl_container_offset_set(
                                &object,
                                Value::String(PhpString::from_test_str(&property)),
                                value.clone(),
                            ) {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(&object.class_name()) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            object.set_property(&property, value.clone());
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
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
                        let scope = current_scope_class(compiled, stack);
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            &property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                let value =
                                    match read_operand_at_frame(unit, stack, frame_index, *value) {
                                        Ok(value) => value,
                                        Err(message) => {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                match $vm.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__set",
                                    &property,
                                    vec![
                                        CallArgument::positional(Value::String(
                                            PhpString::from_test_str(&property),
                                        )),
                                        CallArgument::positional(value.clone()),
                                    ],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(_)) => {
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return result,
                                }
                                if let Some(diagnostic) = dynamic_property_deprecation_diagnostic(
                                    compiled,
                                    state,
                                    &class,
                                    &object,
                                    property.as_ref(),
                                    stack,
                                ) {
                                    $diagnostics.push(diagnostic);
                                }
                                object.set_property(&property, value.clone());
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let resolved_class = &resolved.class;
                        let entry = &resolved.property;
                        if entry.flags.is_static {
                            if let Err(message) = validate_property_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                entry,
                            )
                            .and_then(|()| {
                                validate_property_set_access_in_state(
                                    compiled,
                                    state,
                                    stack,
                                    resolved_class,
                                    entry,
                                )
                            }) {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            emit_static_property_as_non_static_notice(
                                compiled,
                                output,
                                stack,
                                state,
                                resolved_class,
                                entry,
                                instruction.span,
                            );
                            object.set_property(&property, value.clone());
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if let Err(message) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            resolved_class,
                            entry,
                        )
                        .and_then(|()| {
                            validate_property_set_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                entry,
                            )
                        }) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            match $vm.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__set",
                                &property,
                                vec![
                                    CallArgument::positional(Value::String(
                                        PhpString::from_test_str(&property),
                                    )),
                                    CallArgument::positional(value.clone()),
                                ],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {
                                    if entry.flags.is_private
                                        && normalize_class_name(&class.name)
                                            != normalize_class_name(&resolved_class.name)
                                    {
                                        if let Some(diagnostic) =
                                            dynamic_property_deprecation_diagnostic(
                                                compiled,
                                                state,
                                                &class,
                                                &object,
                                                property.as_ref(),
                                                stack,
                                            )
                                        {
                                            $diagnostics.push(diagnostic);
                                        }
                                        object.set_property(&property, value.clone());
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return $vm
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    match $vm.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut $exception_handlers,
                                        &mut $pending_control,
                                        instruction.span,
                                        message,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                                Err(result) => return result,
                            }
                        }
                        let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                            Ok(value) => value,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property_type = ir_runtime_type(entry.type_.as_ref());
                        if let Err(message) = check_property_type(
                            compiled,
                            Some(state),
                            resolved_class.display_name.as_str(),
                            &property,
                            &property_type,
                            &value,
                            $vm.typecheck_fast_path_context(),
                        ) {
                            match $vm.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut $exception_handlers,
                                &mut $pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if let Err(message) =
                            validate_property_write(resolved_class, entry, &object, stack, compiled)
                        {
                            match $vm.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut $exception_handlers,
                                &mut $pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if !property_hook_is_active(state, &object, resolved_class, entry)
                            && let Some(function) = entry.hooks.set
                        {
                            match $vm.call_property_hook(
                                compiled,
                                object.clone(),
                                resolved_class,
                                entry,
                                function,
                                vec![CallArgument::positional(value.clone())],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(_) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Err(result) => return result,
                            }
                        }
                        if !entry.hooks.backed
                            && (entry.hooks.get.is_some() || entry.hooks.set.is_some())
                        {
                            return $vm.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_VIRTUAL_PROPERTY_WRITE: property {}::${} has no backing storage",
                                    resolved_class.name, entry.name
                                ),
                            );
                        }
                        let storage_name = property_storage_name(resolved_class, entry);
                        if !entry.flags.is_typed
                            && object.get_property(&storage_name).is_none()
                            && !magic_property_call_is_active(state, &object, "__set", &property)
                        {
                            match $vm.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__set",
                                &property,
                                vec![
                                    CallArgument::positional(Value::String(
                                        PhpString::from_test_str(&property),
                                    )),
                                    CallArgument::positional(value.clone()),
                                ],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return $vm
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {}
                                Err(result) => return result,
                            }
                        }
                        object.set_property(storage_name, value.clone());
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::AssignPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                        append,
                        value,
                    } => {
                        let _profile = $vm.request_profile_operation_start(
                            RequestProfileOperationCategory::Object,
                            "property_dim_assign",
                        );
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return $vm.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_DIM_ASSIGN_NON_OBJECT: cannot assign property dimension {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                            Ok(value) => value,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        match $vm.assign_property_dim_value(
                            compiled,
                            object,
                            property,
                            &dims,
                            *append,
                            value,
                            instruction.span,
                            &mut $diagnostics,
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
                            Err(PropertyDimAssign::Raise(span, message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(PropertyDimAssign::Fatal(message)) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                            Err(PropertyDimAssign::Return(result)) => return *result,
                        }
                    }
                    InstructionKind::AssignStaticProperty {
                        dst,
                        class_name,
                        property,
                        value,
                    } => {
                        let value_operand = *value;
                        let value =
                            match read_operand_at_frame(unit, stack, frame_index, value_operand) {
                                Ok(value) => value,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let (value, previous_effective) = match $vm.assign_static_property_value(
                            compiled,
                            class_name,
                            property,
                            value,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                $block_id,
                                instruction.id,
                            )),
                            instruction.span,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(outcome) => outcome,
                            Err(StaticPropertyAssignError::Vm(result)) => {
                                match $vm.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    *result,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyAssignError::Raise(span, message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyAssignError::Fatal(message)) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Some(outcome) = $vm.run_destructors_for_unreferenced_value(
                            compiled,
                            output,
                            stack,
                            state,
                            &mut $exception_handlers,
                            &mut $pending_control,
                            &previous_effective,
                        ) {
                            match outcome {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = unset_consumed_assignment_value_operand_at_frame(
                            stack,
                            frame_index,
                            value_operand,
                            *dst,
                        ) {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::AssignDynamicStaticProperty {
                        dst,
                        class_name,
                        property,
                        value,
                    } => {
                        let class_name_value =
                            match read_operand_at_frame(unit, stack, frame_index, *class_name) {
                                Ok(value) => value,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let class_name =
                            match dynamic_static_class_name_from_value(&class_name_value) {
                                Ok(name) => name,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                        if let Err(result) = $vm.autoload_static_class_if_missing(
                            compiled,
                            &class_name,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                $block_id,
                                instruction.id,
                            )),
                            output,
                            stack,
                            state,
                        ) {
                            match $vm.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut $exception_handlers,
                                &mut $pending_control,
                                result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        let class =
                            match resolve_static_class_name(compiled, state, stack, &class_name) {
                                Ok(class) => class,
                                Err(message) => {
                                    match $vm.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut $exception_handlers,
                                        &mut $pending_control,
                                        instruction.span,
                                        message,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            $block_id = target;
                                            continue $dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                        let scope = current_scope_class(compiled, stack);
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                let message = format!(
                                    "E_PHP_VM_UNKNOWN_STATIC_PROPERTY: Access to undeclared static property {}::${property}",
                                    class.display_name
                                );
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if !resolved.property.flags.is_static {
                            return $vm.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is not static",
                                    resolved.class.name, resolved.property.name
                                ),
                            );
                        }
                        if let Err(message) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            &resolved.class,
                            &resolved.property,
                        ) {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                        let value_operand = *value;
                        let value =
                            match read_operand_at_frame(unit, stack, frame_index, value_operand) {
                                Ok(value) => value,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let property_type = ir_runtime_type(resolved.property.type_.as_ref());
                        if let Err(message) = check_property_type(
                            compiled,
                            Some(state),
                            resolved.class.display_name.as_str(),
                            resolved.property.name.as_str(),
                            &property_type,
                            &value,
                            $vm.typecheck_fast_path_context(),
                        ) {
                            match $vm.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut $exception_handlers,
                                &mut $pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        let key = static_property_key(&resolved.class, &resolved.property);
                        let current = if let Some(value) = state.static_properties.get(&key) {
                            value.clone()
                        } else {
                            match static_property_default(
                                compiled,
                                state,
                                stack,
                                &resolved.class,
                                &resolved.property,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return $vm.runtime_error(output, compiled, stack, message);
                                }
                            }
                        };
                        if let Err(message) = validate_static_property_write(
                            compiled,
                            stack,
                            &resolved.class,
                            &resolved.property,
                            &current,
                        ) {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                        let previous_effective = effective_value(&current);
                        if let Err(message) = write_static_property_lvalue(
                            &mut state.static_properties,
                            key,
                            current.clone(),
                            value.clone(),
                        ) {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                        if let Some(outcome) = $vm.run_destructors_for_unreferenced_value(
                            compiled,
                            output,
                            stack,
                            state,
                            &mut $exception_handlers,
                            &mut $pending_control,
                            &previous_effective,
                        ) {
                            match outcome {
                                RaiseOutcome::Caught(target) => {
                                    $block_id = target;
                                    continue $dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = unset_consumed_assignment_value_operand_at_frame(
                            stack,
                            frame_index,
                            value_operand,
                            *dst,
                        ) {
                            return $vm.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::UnsetStaticPropertyDim {
                        class_name,
                        property,
                        dims,
                    } => {
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return $vm.runtime_error(output, compiled, stack, message);
                            }
                        };
                        match static_property_dim_unset_result(
                            $vm,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            &dims,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                $block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(()) => {}
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match $vm.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match $vm.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut $exception_handlers,
                                    &mut $pending_control,
                                    *result,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        $block_id = target;
                                        continue $dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                        }
                    }
            _ => unreachable!("non-property instruction reached rich property dispatch"),
        }
    }};
}

pub(super) use execute_rich_property_instruction;
