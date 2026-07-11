use super::*;

impl Vm {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn load_local_value(
        &self,
        compiled: &CompiledUnit,
        function: &IrFunction,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        local: LocalId,
        span: IrSpan,
        pre_call_by_ref_out_param: bool,
    ) -> Result<Value, VmResult> {
        if is_globals_local(function, local) {
            self.record_counter_local_slot_fast_path(false);
            return Ok(Value::Array(state.globals.globals_array()));
        }
        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(stack, local));
        let local_value = stack.current().expect("frame was pushed").locals.get(local);
        match local_value {
            Some(Value::Uninitialized) if is_this_local(function, local) => {
                let result = self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_THIS_OUTSIDE_METHOD: Using $this when not in object context"
                        .to_owned(),
                );
                if let Some(throwable) = runtime_error_throwable(&result) {
                    tag_throwable_location(&throwable, compiled, span);
                    state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                    state.pending_throw = Some(throwable);
                    Err(VmResult::propagating_exception(output.clone()))
                } else {
                    Err(result)
                }
            }
            Some(Value::Uninitialized) if pre_call_by_ref_out_param => Ok(Value::Null),
            Some(Value::Uninitialized) => {
                let local_name = function
                    .locals
                    .get(local.index())
                    .cloned()
                    .unwrap_or_else(|| format!("local:{}", local.raw()));
                let diagnostic = if is_auto_global_name(&local_name) {
                    undefined_global_variable_warning(
                        local_name,
                        runtime_source_span(compiled, span),
                        stack_trace(compiled, stack),
                    )
                } else {
                    undefined_variable_warning(
                        local_name,
                        runtime_source_span(compiled, span),
                        stack_trace(compiled, stack),
                    )
                };
                let handled = self.dispatch_error_handler(
                    compiled,
                    output,
                    stack,
                    state,
                    php_runtime::PHP_E_WARNING,
                    &diagnostic,
                )?;
                if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
                    emit_vm_diagnostic(
                        output,
                        state,
                        &diagnostic,
                        php_runtime::PhpDiagnosticChannel::Warning,
                        php_runtime::PHP_E_WARNING,
                    );
                    diagnostics.push(diagnostic);
                }
                Ok(Value::Null)
            }
            Some(value) => Ok(value),
            None => Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!("invalid local local:{}", local.raw()),
            )),
        }
    }

    #[cold]
    #[inline(never)]
    fn dense_runtime_error(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        span: IrSpan,
        message: String,
    ) -> VmResult {
        // Match the rich raise path: the emitted diagnostic carries the
        // source span, not just the throwable location tag.
        let result = self.runtime_error_with_bringup_context(
            output,
            compiled,
            stack,
            state,
            runtime_source_span(compiled, span),
            message,
            BringupDiagnosticInput {
                autoload_enabled: Some(true),
                ..BringupDiagnosticInput::default()
            },
        );
        if let Some(throwable) = runtime_error_throwable(&result) {
            tag_throwable_location(&throwable, compiled, span);
            state.pending_trace = Some(capture_backtrace_string(compiled, stack));
            state.pending_throw = Some(throwable);
            VmResult::propagating_exception(output.clone())
        } else {
            result
        }
    }

    #[cold]
    #[inline(never)]
    pub(super) fn runtime_error_at_optional_span(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        span: Option<IrSpan>,
        message: String,
    ) -> VmResult {
        if let Some(span) = span {
            self.dense_runtime_error(compiled, output, stack, state, span, message)
        } else {
            self.runtime_error(output, compiled, stack, message)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dense_fetch_property_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
        property: &str,
        object_value: Value,
        span: IrSpan,
    ) -> Result<Value, VmResult> {
        let object = match object_value {
            Value::Object(object) => object,
            Value::Reference(cell) => match cell.get() {
                Value::Object(object) => object,
                other => {
                    self.record_counter_dense_property_fallback("non_object");
                    return Err(self.dense_runtime_error(
                        compiled,
                        output,
                        stack,
                        state,
                        span,
                        format!(
                            "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot fetch property {property} from {}",
                            value_type_name(&other)
                        ),
                    ));
                }
            },
            other => {
                self.record_counter_dense_property_fallback("non_object");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!(
                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot fetch property {property} from {}",
                        value_type_name(&other)
                    ),
                ));
            }
        };

        let class_handle = object.class_name_handle();
        if internal_throwable_instanceof(&class_handle, "throwable").is_some()
            || is_php_token_runtime_class(&class_handle)
            || is_std_class_runtime_class(&class_handle)
            || is_date_time_runtime_class(&class_handle)
            || is_pdo_runtime_class(&class_handle)
        {
            self.record_counter_dense_property_fallback("internal_runtime_object");
            return Ok(object.get_property(property).unwrap_or(Value::Null));
        }
        if class_name_is(&class_handle, &["simplexmlelement"]) {
            self.record_counter_dense_property_fallback("simplexml_property");
            return Ok(php_runtime::xml::simplexml_property(&object, property));
        }
        if spl_array_object_uses_array_as_props(&object) {
            // ARRAY_AS_PROPS routes property reads through the container's
            // array storage; the rich arm takes the same branch.
            self.record_counter_dense_property_fallback("spl_array_as_props");
            return spl_container_offset_get(
                &object,
                &Value::String(PhpString::from_test_str(property)),
            )
            .map_err(|message| {
                self.dense_runtime_error(compiled, output, stack, state, span, message)
            });
        }

        let scope = current_scope_class(compiled, stack);
        let normalized_scope = scope.as_deref().map(normalize_class_name);
        let receiver_class = normalize_class_name(&class_handle);
        let lookup_epoch = state.lookup_epoch();

        let cached_target = if let Some(id) = cache_id {
            self.lookup_dense_property_fetch_inline_cache(
                id,
                function_id,
                instruction_id,
                property,
                &receiver_class,
                normalized_scope.as_deref(),
                lookup_epoch,
            )
        } else {
            self.observe_dense_property_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                InlineCacheKind::PropertyFetch,
            );
            self.lookup_property_fetch_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                property,
                &receiver_class,
                normalized_scope.as_deref(),
                lookup_epoch,
            )
        };
        if let Some(target) = cached_target {
            match self.read_property_fetch_target(compiled, target, &object, stack, state) {
                Ok(PropertyFetchCacheRead::Value(value)) => {
                    self.record_counter_dense_property_ic_reuse();
                    self.record_counter_dense_property_fetch_hit();
                    return Ok(value);
                }
                Ok(PropertyFetchCacheRead::Fallback) => {
                    self.record_counter_dense_property_fallback("inline_cache_guard");
                }
                Err(message) => {
                    return Err(
                        self.dense_runtime_error(compiled, output, stack, state, span, message)
                    );
                }
            }
        }

        // Only the resolve/miss path needs the class entry and the magic-get
        // flag; keep the class-table lookup and hierarchy walk off the
        // cache-hit path.
        let class = match lookup_class_in_state(compiled, state, &class_handle) {
            Some(class) => class,
            None => {
                self.record_counter_dense_property_fallback("receiver_class_missing");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!("E_PHP_VM_UNKNOWN_CLASS: class {class_handle} is not defined"),
                ));
            }
        };
        let receiver_has_magic_get = class_has_public_magic_get(compiled, &class);
        let resolved = match lookup_resolved_property_in_state(
            compiled,
            state,
            &class,
            property,
            scope.as_deref(),
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                self.record_counter_dense_property_fallback("dynamic_property");
                let property_callsite =
                    property_fetch_callsite(compiled, function_id, block_id, instruction_id);
                if let Some(value) = object.get_property(property) {
                    self.record_counter_property_fetch_profile(property_fetch_profile_observation(
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
                    ));
                    return Ok(value);
                }
                self.record_counter_property_fetch_profile(property_fetch_profile_observation(
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
                ));
                match self.call_magic_property_method(
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
                        self.record_counter_dense_property_fallback("magic_get");
                        return Ok(value);
                    }
                    Ok(None) => {}
                    Err(result) => return Err(result),
                }
                self.emit_undefined_property_warning(
                    compiled,
                    output,
                    stack,
                    state,
                    diagnostics,
                    &object.display_name(),
                    property,
                    span,
                )?;
                return Ok(Value::Null);
            }
            Err(message) => {
                return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
            }
        };
        let resolved_class = &resolved.class;
        let resolved_property = &resolved.property;

        if let Err(access_error) = validate_property_access_in_state(
            compiled,
            state,
            stack,
            resolved_class,
            resolved_property,
        ) {
            self.record_counter_dense_property_fallback("visibility_mismatch");
            match self.call_magic_property_method(
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
                    self.record_counter_dense_property_fallback("magic_get");
                    return Ok(value);
                }
                Ok(None) => {
                    return Err(self.dense_runtime_error(
                        compiled,
                        output,
                        stack,
                        state,
                        span,
                        access_error,
                    ));
                }
                Err(result) => return Err(result),
            }
        }

        if resolved.property.flags.is_static {
            self.record_counter_dense_property_fallback("static_property");
            emit_static_property_as_non_static_notice(
                compiled,
                output,
                stack,
                state,
                resolved_class,
                resolved_property,
                span,
            );
        }

        if !property_hook_is_active(state, &object, resolved_class, resolved_property)
            && let Some(function) = resolved.property.hooks.get
        {
            self.record_counter_dense_property_fallback("property_hook");
            return self.call_property_hook(
                compiled,
                object,
                resolved_class,
                resolved_property,
                function,
                Vec::new(),
                output,
                stack,
                state,
            );
        }

        let storage_name = property_storage_name(resolved_class, resolved_property);
        let Some(value) = object.get_property(&storage_name) else {
            self.record_counter_dense_property_fallback("storage_missing");
            match self.call_magic_property_method(
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
                    self.record_counter_dense_property_fallback("magic_get");
                    return Ok(value);
                }
                Ok(None) => {}
                Err(result) => return Err(result),
            }
            self.emit_undefined_property_warning(
                compiled,
                output,
                stack,
                state,
                diagnostics,
                &object.display_name(),
                property,
                span,
            )?;
            return Ok(Value::Null);
        };
        if matches!(value, Value::Uninitialized) {
            self.record_counter_dense_property_fallback("typed_property_uninitialized");
            return Err(self.dense_runtime_error(
                compiled,
                output,
                stack,
                state,
                span,
                format!(
                    "E_PHP_VM_UNINITIALIZED_PROPERTY: Typed property {}::${property} must not be accessed before initialization",
                    resolved.class.display_name
                ),
            ));
        }
        self.maybe_install_property_fetch_inline_cache_target(
            compiled,
            function_id,
            block_id,
            instruction_id,
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
            cache_id,
        );
        self.record_counter_dense_property_fetch_hit();
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dense_assign_property_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
        property: &str,
        object_value: Value,
        value: Value,
        span: IrSpan,
    ) -> Result<Value, VmResult> {
        let object = match object_value {
            Value::Object(object) => object,
            Value::Reference(cell) => match cell.get() {
                Value::Object(object) => object,
                other => {
                    self.record_counter_dense_property_fallback("non_object");
                    return Err(self.dense_runtime_error(
                        compiled,
                        output,
                        stack,
                        state,
                        span,
                        format!(
                            "E_PHP_VM_PROPERTY_ASSIGN_NON_OBJECT: cannot assign property {property} on {}",
                            value_type_name(&other)
                        ),
                    ));
                }
            },
            Value::Callable(_) => {
                self.record_counter_dense_property_fallback("dynamic_property");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!(
                        "E_PHP_VM_DYNAMIC_PROPERTY_ERROR: Cannot create dynamic property Closure::${property}"
                    ),
                ));
            }
            other => {
                self.record_counter_dense_property_fallback("non_object");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!(
                        "E_PHP_VM_PROPERTY_ASSIGN_NON_OBJECT: cannot assign property {property} on {}",
                        value_type_name(&other)
                    ),
                ));
            }
        };

        if spl_array_object_uses_array_as_props(&object) {
            // ARRAY_AS_PROPS routes property writes into the container's
            // array storage; the rich arms take the same branch before any
            // declared/dynamic property handling.
            self.record_counter_dense_property_fallback("spl_array_as_props");
            if let Err(message) = spl_container_offset_set(
                &object,
                Value::String(PhpString::from_test_str(property)),
                value.clone(),
            ) {
                return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
            }
            return Ok(value);
        }
        let class_handle = object.class_name_handle();
        if is_std_class_runtime_class(&class_handle) {
            self.record_counter_dense_property_fallback("dynamic_property");
            object.set_property(property, value.clone());
            return Ok(value);
        }

        let scope = current_scope_class(compiled, stack);
        let normalized_scope = scope.as_deref().map(normalize_class_name);
        let receiver_class = normalize_class_name(&class_handle);
        let lookup_epoch = state.lookup_epoch();

        let cached_target = if let Some(id) = cache_id {
            self.lookup_dense_property_assign_inline_cache(
                id,
                function_id,
                instruction_id,
                property,
                &receiver_class,
                normalized_scope.as_deref(),
                lookup_epoch,
            )
        } else {
            self.observe_dense_property_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                InlineCacheKind::PropertyAssign,
            );
            self.lookup_property_assign_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                property,
                &receiver_class,
                normalized_scope.as_deref(),
                lookup_epoch,
            )
        };
        if let Some(target) = cached_target {
            match self.write_property_assign_target(
                compiled,
                target,
                &object,
                value.clone(),
                stack,
                state,
            ) {
                Ok(PropertyAssignCacheWrite::Written(value)) => {
                    self.record_counter_dense_property_ic_reuse();
                    self.record_counter_dense_property_assignment_hit();
                    return Ok(value);
                }
                Ok(PropertyAssignCacheWrite::Fallback) => {
                    self.record_counter_dense_property_fallback("inline_cache_guard");
                }
                Err(message) => {
                    return Err(
                        self.dense_runtime_error(compiled, output, stack, state, span, message)
                    );
                }
            }
        }

        // Same as the fetch arm: the class entry and magic-set flag only
        // matter on the resolve/miss path.
        let class = match lookup_class_in_state(compiled, state, &class_handle) {
            Some(class) => class,
            None => {
                self.record_counter_dense_property_fallback("receiver_class_missing");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!("E_PHP_VM_UNKNOWN_CLASS: class {class_handle} is not defined"),
                ));
            }
        };
        let receiver_has_magic_set = class_has_public_magic_set(compiled, &class);
        let resolved = match lookup_resolved_property_in_state(
            compiled,
            state,
            &class,
            property,
            scope.as_deref(),
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                self.record_counter_dense_property_fallback("dynamic_property");
                match self.call_magic_property_method(
                    compiled,
                    object.clone(),
                    "__set",
                    property,
                    vec![
                        CallArgument::positional(Value::String(PhpString::from_test_str(property))),
                        CallArgument::positional(value.clone()),
                    ],
                    output,
                    stack,
                    state,
                ) {
                    Ok(Some(_)) => {
                        self.record_counter_dense_property_fallback("magic_set");
                        return Ok(value);
                    }
                    Ok(None) => {}
                    Err(result) => return Err(result),
                }
                if let Some(diagnostic) = dynamic_property_deprecation_diagnostic(
                    compiled, state, &class, &object, property, stack,
                ) {
                    state.diagnostics.push(diagnostic);
                }
                object.set_property(property, value.clone());
                return Ok(value);
            }
            Err(message) => {
                return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
            }
        };
        let resolved_class = &resolved.class;
        let resolved_property = &resolved.property;

        if resolved.property.flags.is_static {
            self.record_counter_dense_property_fallback("static_property");
            emit_static_property_as_non_static_notice(
                compiled,
                output,
                stack,
                state,
                resolved_class,
                resolved_property,
                span,
            );
        }

        if let Err(message) = validate_property_access_in_state(
            compiled,
            state,
            stack,
            resolved_class,
            resolved_property,
        )
        .and_then(|()| {
            validate_property_set_access_in_state(
                compiled,
                state,
                stack,
                resolved_class,
                resolved_property,
            )
        }) {
            self.record_counter_dense_property_fallback("visibility_mismatch");
            match self.call_magic_property_method(
                compiled,
                object.clone(),
                "__set",
                property,
                vec![
                    CallArgument::positional(Value::String(PhpString::from_test_str(property))),
                    CallArgument::positional(value.clone()),
                ],
                output,
                stack,
                state,
            ) {
                Ok(Some(_)) => {
                    self.record_counter_dense_property_fallback("magic_set");
                    return Ok(value);
                }
                Ok(None) => {
                    return Err(
                        self.dense_runtime_error(compiled, output, stack, state, span, message)
                    );
                }
                Err(result) => return Err(result),
            }
        }

        let property_type = ir_runtime_type(resolved.property.type_.as_ref());
        if let Err(message) = check_property_type(
            compiled,
            Some(state),
            resolved_class.display_name.as_str(),
            property,
            &property_type,
            &value,
            self.typecheck_fast_path_context(),
        ) {
            self.record_counter_dense_property_fallback("typed_property_validation");
            return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
        }
        if let Err(message) =
            validate_property_write(resolved_class, resolved_property, &object, stack, compiled)
        {
            self.record_counter_dense_property_fallback("readonly_or_init_only");
            return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
        }
        if !property_hook_is_active(state, &object, resolved_class, resolved_property)
            && let Some(function) = resolved.property.hooks.set
        {
            self.record_counter_dense_property_fallback("property_hook");
            self.call_property_hook(
                compiled,
                object.clone(),
                resolved_class,
                resolved_property,
                function,
                vec![CallArgument::positional(value.clone())],
                output,
                stack,
                state,
            )?;
            return Ok(value);
        }
        if !resolved.property.hooks.backed
            && (resolved.property.hooks.get.is_some() || resolved.property.hooks.set.is_some())
        {
            self.record_counter_dense_property_fallback("property_hook");
            return Err(self.dense_runtime_error(
                compiled,
                output,
                stack,
                state,
                span,
                format!(
                    "E_PHP_VM_VIRTUAL_PROPERTY_WRITE: property {}::${} has no backing storage",
                    resolved_class.name, resolved_property.name
                ),
            ));
        }

        let storage_name = property_storage_name(resolved_class, resolved_property);
        if matches!(
            object.get_property(&storage_name),
            Some(Value::Reference(_))
        ) {
            self.record_counter_dense_property_fallback("reference_slot");
        }
        write_property_storage_value(&object, &storage_name, value.clone());
        self.maybe_install_property_assign_inline_cache_target(
            compiled,
            function_id,
            block_id,
            instruction_id,
            property,
            &receiver_class,
            &class,
            resolved_class,
            resolved_property,
            &storage_name,
            normalized_scope.as_deref(),
            lookup_epoch,
            receiver_has_magic_set,
            state,
            &object,
            cache_id,
        );
        self.record_counter_dense_property_assignment_hit();
        Ok(value)
    }

    pub(super) fn try_dense_array_fetch_dim_borrowed(
        &self,
        array: &Value,
        key_value: &Value,
        quiet: bool,
    ) -> Option<Value> {
        let Value::Array(array) = array else {
            return None;
        };
        let Ok(key) = array_key_from_value(key_value) else {
            return None;
        };
        let result = match array.get(&key) {
            Some(value) => {
                let _source = layout_source::enter(layout_source::ARRAY_ELEMENT_READ);
                Some(effective_value(value))
            }
            None if quiet => Some(Value::Null),
            None => None,
        };
        if result.is_some()
            && self.options.collect_counters
            && let Some(counters) = self.counters.borrow_mut().as_mut()
        {
            counters.record_array_read_borrow_hit();
        }
        result
    }

    pub(super) fn read_dense_dim_operands(
        &self,
        compiled: &CompiledUnit,
        stack: &CallStack,
        dims: &[DenseOperand],
    ) -> Result<Vec<ArrayKey>, String> {
        dims.iter()
            .map(|operand| {
                self.read_dense_operand(compiled, stack, *operand)
                    .and_then(|value| array_key_from_value(&value))
            })
            .collect()
    }

    pub(super) fn read_dense_dim_values(
        &self,
        compiled: &CompiledUnit,
        stack: &CallStack,
        dims: &[DenseOperand],
    ) -> Result<Vec<Value>, String> {
        dims.iter()
            .map(|operand| self.read_dense_operand(compiled, stack, *operand))
            .collect()
    }

    /// Dense mirror of `evaluate_closure_captures`: by-ref captures bind the
    /// enclosing local's reference cell, by-value captures read the operand.
    pub(super) fn evaluate_dense_closure_captures(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        captures: &[DenseClosureCapture],
    ) -> Result<Vec<ClosureCaptureValue>, String> {
        let mut values = Vec::with_capacity(captures.len());
        for capture in captures {
            let name = dense
                .names
                .get(capture.name as usize)
                .ok_or_else(|| format!("invalid dense bytecode capture name n{}", capture.name))?;
            if capture.by_ref {
                if capture.src.kind != DenseOperandKind::Local {
                    return Err(format!(
                        "E_PHP_VM_BY_REF_CAPTURE_NOT_REFERENCEABLE: closure capture ${name} is not a local variable"
                    ));
                }
                let _source = layout_source::enter(layout_source::CLOSURE_CAPTURE_BINDING);
                let cell = stack
                    .current_mut()
                    .ok_or("no active frame")?
                    .locals
                    .ensure_reference_cell(LocalId::new(capture.src.index))?;
                values.push(ClosureCaptureValue::by_reference(name.clone(), cell));
                continue;
            }
            values.push(ClosureCaptureValue::by_value(
                name.clone(),
                self.read_dense_operand(compiled, stack, capture.src)?,
            ));
        }
        Ok(values)
    }

    pub(super) fn read_dense_call_args(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        args: &[DenseCallArg],
    ) -> Result<Vec<CallArgument>, String> {
        self.read_dense_call_args_with_value_policy(dense, compiled, stack, args, |_, _| false)
    }

    pub(super) fn read_dense_call_args_for_function(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        function: &str,
        args: &[DenseCallArg],
    ) -> Result<Vec<CallArgument>, String> {
        self.read_dense_call_args_with_value_policy(dense, compiled, stack, args, |index, arg| {
            is_quiet_dense_by_ref_internal_builtin_arg(dense, function, index, arg)
        })
    }

    fn read_dense_call_args_with_value_policy(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        args: &[DenseCallArg],
        mut use_null_placeholder: impl FnMut(usize, &DenseCallArg) -> bool,
    ) -> Result<Vec<CallArgument>, String> {
        let mut out = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            let value = if use_null_placeholder(index, arg) {
                Value::Null
            } else {
                let value = self.read_dense_operand_with_source(
                    compiled,
                    stack,
                    arg.value,
                    layout_source::CALL_ARGUMENT_SNAPSHOT,
                )?;
                self.record_counter_value_clone_reason(
                    layout_source::CALL_ARGUMENT_SNAPSHOT.name(),
                );
                value
            };
            let by_ref_dim = arg
                .by_ref_dim
                .as_ref()
                .map(|target| {
                    self.read_dense_dim_operands(compiled, stack, &target.dims)
                        .map(|dims| CallDimTarget {
                            local: LocalId::new(target.local),
                            dims,
                        })
                })
                .transpose()?;
            let by_ref_property = arg
                .by_ref_property
                .as_ref()
                .map(
                    |target| match self.read_dense_operand(compiled, stack, target.object)? {
                        Value::Object(object) => Ok(CallPropertyTarget {
                            object,
                            property: dense
                                .names
                                .get(target.property as usize)
                                .ok_or_else(|| {
                                    format!(
                                        "invalid dense bytecode property name n{}",
                                        target.property
                                    )
                                })?
                                .clone(),
                        }),
                        other => Err(format!(
                            "E_PHP_VM_BY_REF_PROPERTY_NON_OBJECT: cannot bind property n{} on {}",
                            target.property,
                            value_type_name(&other)
                        )),
                    },
                )
                .transpose()?;
            let by_ref_property_dim = arg
                .by_ref_property_dim
                .as_ref()
                .map(
                    |target| match self.read_dense_operand(compiled, stack, target.object)? {
                        Value::Object(object) => {
                            let dims =
                                self.read_dense_dim_operands(compiled, stack, &target.dims)?;
                            Ok(CallPropertyDimTarget {
                                object,
                                property: dense
                                    .names
                                    .get(target.property as usize)
                                    .ok_or_else(|| {
                                        format!(
                                            "invalid dense bytecode property name n{}",
                                            target.property
                                        )
                                    })?
                                    .clone(),
                                dims,
                            })
                        }
                        other => Err(format!(
                            "E_PHP_VM_BY_REF_PROPERTY_DIM_NON_OBJECT: cannot bind property dimension n{} on {}",
                            target.property,
                            value_type_name(&other)
                        )),
                    },
                )
                .transpose()?;
            out.push(CallArgument {
                name: arg
                    .name
                    .map(|name| {
                        dense
                            .names
                            .get(name as usize)
                            .cloned()
                            .ok_or_else(|| format!("invalid dense bytecode argument name n{name}"))
                    })
                    .transpose()?,
                value,
                value_kind: arg.value_kind,
                by_ref_local: arg.by_ref_local.map(LocalId::new),
                by_ref_dim,
                by_ref_property,
                by_ref_property_dim,
            });
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_array_offset_on_scalar_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        base: &Value,
        span: IrSpan,
    ) -> Result<(), VmResult> {
        let diagnostic = array_offset_on_scalar_warning(
            base,
            runtime_source_span(compiled, span),
            stack_trace(compiled, stack),
        );
        match self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        ) {
            Ok(false) if error_reporting_allows(state, php_runtime::PHP_E_WARNING) => {
                emit_vm_diagnostic(
                    output,
                    state,
                    &diagnostic,
                    php_runtime::PhpDiagnosticChannel::Warning,
                    php_runtime::PHP_E_WARNING,
                );
                diagnostics.push(diagnostic);
            }
            Ok(_) => {}
            Err(result) => return Err(result),
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fetch_dim_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        array: &Value,
        key_value: &Value,
        quiet: bool,
        span: IrSpan,
    ) -> Result<Value, VmResult> {
        let base = effective_value(array);
        if let Value::Object(object) = &base
            && spl_runtime_marker(object)
                .is_some_and(|class| is_spl_array_access_runtime_class(&class))
        {
            return self.call_array_access_dim_method(
                compiled,
                object.clone(),
                "offsetGet",
                key_value.clone(),
                Some(span),
                output,
                stack,
                state,
            );
        }
        if let Some(object) = match userland_arrayaccess_object(compiled, state, &base) {
            Ok(object) => object,
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        } {
            return self.call_userland_arrayaccess_method(
                compiled,
                output,
                stack,
                state,
                object,
                "offsetGet",
                vec![CallArgument::positional(key_value.clone())],
                span,
            );
        }
        let key = array_key_from_value(key_value)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        if let Value::Object(object) = &base
            && normalize_class_name(&object.class_name()) == "simplexmlelement"
        {
            return Ok(php_runtime::xml::simplexml_dimension(object, &key));
        }
        if let Value::String(string) = &base {
            return match string_offset_for_read(string, &key) {
                StringOffsetRead::Byte(value) => Ok(value),
                StringOffsetRead::Illegal { value, key_bytes } => {
                    if !quiet {
                        let diagnostic = illegal_string_offset_warning(
                            &key_bytes,
                            runtime_source_span(compiled, span),
                            stack_trace(compiled, stack),
                        );
                        match self.dispatch_error_handler(
                            compiled,
                            output,
                            stack,
                            state,
                            php_runtime::PHP_E_WARNING,
                            &diagnostic,
                        ) {
                            Ok(false)
                                if error_reporting_allows(state, php_runtime::PHP_E_WARNING) =>
                            {
                                emit_vm_diagnostic(
                                    output,
                                    state,
                                    &diagnostic,
                                    php_runtime::PhpDiagnosticChannel::Warning,
                                    php_runtime::PHP_E_WARNING,
                                );
                            }
                            Ok(_) => {}
                            Err(result) => return Err(result),
                        }
                    }
                    Ok(value)
                }
                StringOffsetRead::OutOfRange(index) => {
                    if quiet {
                        Ok(Value::Null)
                    } else {
                        let diagnostic = uninitialized_string_offset_warning(
                            index,
                            runtime_source_span(compiled, span),
                            stack_trace(compiled, stack),
                        );
                        match self.dispatch_error_handler(
                            compiled,
                            output,
                            stack,
                            state,
                            php_runtime::PHP_E_WARNING,
                            &diagnostic,
                        ) {
                            Ok(false)
                                if error_reporting_allows(state, php_runtime::PHP_E_WARNING) =>
                            {
                                emit_vm_diagnostic(
                                    output,
                                    state,
                                    &diagnostic,
                                    php_runtime::PhpDiagnosticChannel::Warning,
                                    php_runtime::PHP_E_WARNING,
                                );
                            }
                            Ok(_) => {}
                            Err(result) => return Err(result),
                        }
                        Ok(Value::string(Vec::new()))
                    }
                }
                StringOffsetRead::NonNumeric => {
                    if quiet {
                        Ok(Value::Null)
                    } else {
                        Err(self.runtime_error_with_source_span(
                            output,
                            compiled,
                            stack,
                            runtime_source_span(compiled, span),
                            "E_PHP_VM_STRING_OFFSET_TYPE: Cannot access offset of type string on string"
                                .to_owned(),
                        ))
                    }
                }
            };
        }
        if quiet && quiet_dim_fetch_scalar_returns_null(&base) {
            return Ok(Value::Null);
        }
        if quiet_dim_fetch_scalar_returns_null(&base) {
            self.emit_array_offset_on_scalar_warning(
                compiled,
                output,
                stack,
                state,
                diagnostics,
                &base,
                span,
            )?;
            return Ok(Value::Null);
        }

        match fetch_dim_value(array, &key) {
            Ok(Some(value)) => Ok(value),
            Ok(None) if quiet => Ok(Value::Null),
            Ok(None) => {
                diagnostics.push(undefined_array_key_warning(
                    &key,
                    runtime_source_span(compiled, span),
                    stack_trace(compiled, stack),
                ));
                Ok(Value::Null)
            }
            Err(message) => Err(self.runtime_error_with_source_span(
                output,
                compiled,
                stack,
                runtime_source_span(compiled, span),
                message,
            )),
        }
    }

    pub(super) fn call_userland_arrayaccess_method(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        object: ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        span: IrSpan,
    ) -> Result<Value, VmResult> {
        let result = self.call_object_method_callable(
            compiled,
            object,
            method,
            args,
            Some(span),
            output,
            stack,
            state,
        );
        if !result.status.is_success()
            || result.yielded.is_some()
            || result.fiber_suspension.is_some()
        {
            return Err(result);
        }
        Ok(result.return_value.unwrap_or(Value::Null))
    }

    pub(super) fn call_array_access_dim_method(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        method: &str,
        key: Value,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let span = call_span.unwrap_or_default();
        self.call_userland_arrayaccess_method(
            compiled,
            output,
            stack,
            state,
            object,
            method,
            vec![CallArgument::positional(key)],
            span,
        )
    }

    pub(super) fn try_userland_arrayaccess_offset_set_local(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        local: LocalId,
        dim_values: &[Value],
        append: bool,
        // Borrowed: the common non-ArrayAccess target returns `Ok(false)`
        // without ever needing the value, so the clone happens only on the
        // actual `offsetSet` dispatch below.
        value: &Value,
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let Some(object) = local_effective_object(stack, local) else {
            return Ok(false);
        };
        let object = match userland_arrayaccess_object_from_object(compiled, state, object) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(false),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        let key = if append {
            if !dim_values.is_empty() {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAYACCESS_NESTED_DIM: nested ArrayAccess writes are not implemented",
                ));
            }
            Value::Null
        } else {
            let [key] = dim_values else {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAYACCESS_DIM: ArrayAccess writes require exactly one dimension",
                ));
            };
            key.clone()
        };
        self.call_userland_arrayaccess_method(
            compiled,
            output,
            stack,
            state,
            object,
            "offsetSet",
            vec![
                CallArgument::positional(key),
                CallArgument::positional(value.clone()),
            ],
            span,
        )?;
        Ok(true)
    }

    pub(super) fn try_userland_arrayaccess_offset_set_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        base: &Value,
        dims: &[ArrayKey],
        append: bool,
        value: Value,
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let object = match userland_arrayaccess_object(compiled, state, base) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(false),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        let key = if append {
            if !dims.is_empty() {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAYACCESS_NESTED_DIM: nested ArrayAccess writes are not implemented",
                ));
            }
            Value::Null
        } else {
            let [key] = dims else {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAYACCESS_DIM: ArrayAccess writes require exactly one dimension",
                ));
            };
            array_key_to_value(key.clone())
        };
        self.call_userland_arrayaccess_method(
            compiled,
            output,
            stack,
            state,
            object,
            "offsetSet",
            vec![
                CallArgument::positional(key),
                CallArgument::positional(value),
            ],
            span,
        )?;
        Ok(true)
    }

    pub(super) fn try_userland_arrayaccess_offset_exists_local(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        local: LocalId,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<Option<bool>, VmResult> {
        let Some(local_value) = read_local_value(stack, local) else {
            return Ok(None);
        };
        let object = match userland_arrayaccess_object(compiled, state, &local_value) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(None),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        self.arrayaccess_dim_isset_value(
            compiled,
            output,
            stack,
            state,
            Value::Object(object),
            dims,
            span,
        )
        .map(Some)
    }

    pub(super) fn try_userland_arrayaccess_offset_empty_local(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        local: LocalId,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<Option<bool>, VmResult> {
        let Some(local_value) = read_local_value(stack, local) else {
            return Ok(None);
        };
        let object = match userland_arrayaccess_object(compiled, state, &local_value) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(None),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        self.arrayaccess_dim_empty_value(
            compiled,
            output,
            stack,
            state,
            Value::Object(object),
            dims,
            span,
        )
        .map(Some)
    }

    #[allow(clippy::too_many_arguments)]
    fn arrayaccess_dim_isset_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        value: Value,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let Some((first, rest)) = dims.split_first() else {
            return Ok(!matches!(
                effective_value(&value),
                Value::Uninitialized | Value::Null
            ));
        };
        let base = effective_value(&value);
        if let Some(object) = match arrayaccess_object(compiled, state, &base) {
            Ok(object) => object,
            Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
        } {
            let key_value = array_key_to_value(first.clone());
            let exists = self.call_array_access_dim_method(
                compiled,
                object.clone(),
                "offsetExists",
                key_value.clone(),
                Some(span),
                output,
                stack,
                state,
            )?;
            if !to_bool(&exists)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?
            {
                return Ok(false);
            }
            if rest.is_empty() {
                return Ok(true);
            }
            let child = self.call_array_access_dim_method(
                compiled,
                object,
                "offsetGet",
                key_value,
                Some(span),
                output,
                stack,
                state,
            )?;
            return self
                .arrayaccess_dim_isset_value(compiled, output, stack, state, child, rest, span);
        }
        let value = fetch_dim_path_value(&base, dims).ok().flatten();
        Ok(!matches!(value, None | Some(Value::Null)))
    }

    #[allow(clippy::too_many_arguments)]
    fn arrayaccess_dim_empty_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        value: Value,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let Some((first, rest)) = dims.split_first() else {
            return php_empty(&value)
                .map_err(|message| self.runtime_error(output, compiled, stack, message));
        };
        let base = effective_value(&value);
        if let Some(object) = match arrayaccess_object(compiled, state, &base) {
            Ok(object) => object,
            Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
        } {
            let key_value = array_key_to_value(first.clone());
            let exists = self.call_array_access_dim_method(
                compiled,
                object.clone(),
                "offsetExists",
                key_value.clone(),
                Some(span),
                output,
                stack,
                state,
            )?;
            if !to_bool(&exists)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?
            {
                return Ok(true);
            }
            let child = self.call_array_access_dim_method(
                compiled,
                object,
                "offsetGet",
                key_value,
                Some(span),
                output,
                stack,
                state,
            )?;
            if rest.is_empty() {
                return php_empty_access_value(&child)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message));
            }
            return self
                .arrayaccess_dim_empty_value(compiled, output, stack, state, child, rest, span);
        }
        let value = fetch_dim_path_value(&base, dims)
            .ok()
            .flatten()
            .unwrap_or(Value::Uninitialized);
        php_empty_access_value(&value)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))
    }

    pub(super) fn try_userland_arrayaccess_offset_unset_local(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        local: LocalId,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let Some(local_value) = read_local_value(stack, local) else {
            return Ok(false);
        };
        let object = match userland_arrayaccess_object(compiled, state, &local_value) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(false),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        let [key] = dims else {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_ARRAYACCESS_NESTED_DIM: nested ArrayAccess unset is not implemented",
            ));
        };
        self.call_userland_arrayaccess_method(
            compiled,
            output,
            stack,
            state,
            object,
            "offsetUnset",
            vec![CallArgument::positional(array_key_to_value(key.clone()))],
            span,
        )?;
        Ok(true)
    }

    #[cold]
    #[inline(never)]
    pub(super) fn invalid_bytecode_operand_shape(
        &self,
        output: &mut OutputBuffer,
        compiled: &CompiledUnit,
        stack: &CallStack,
        instruction: &DenseInstruction,
    ) -> VmResult {
        self.runtime_error(
            output,
            compiled,
            stack,
            format!(
                "E_PHP_VM_DENSE_BYTECODE_OPERAND_SHAPE: opcode {} has invalid operand payload",
                instruction.opcode.as_str()
            ),
        )
    }
}
