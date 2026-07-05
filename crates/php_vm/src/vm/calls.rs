//! Callable dispatch and dynamic call execution.

use super::prelude::*;

impl Vm {
    /// Dispatches a runtime callable value (callable string, closure,
    /// invokable, callable array) through the shared function-call target
    /// helpers. Both the rich `CallCallable` arm and the dense
    /// `CallCallable` opcode call this, so their semantics cannot diverge:
    /// plain function-name strings take the function-call inline cache,
    /// everything else routes through the generic callable dispatcher.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_callable_value_call(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        values: Vec<CallArgument>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        running_fiber: &Option<FiberRef>,
    ) -> VmResult {
        match &callee {
            Value::String(name) => {
                let display_name = name.to_string_lossy();
                if display_name.contains("::") {
                    self.call_callable_with_call_span(
                        compiled, callee, values, call_span, output, stack, state,
                    )
                } else {
                    let lowered_name = normalize_function_name(&display_name);
                    let interned_name = PhpString::intern(lowered_name.as_bytes());
                    let epoch = state.lookup_epoch();
                    let call_shape = function_call_shape(&values);
                    let target = self
                        .lookup_function_call_inline_cache(
                            compiled,
                            function_id,
                            block_id,
                            instruction_id,
                            &interned_name,
                            epoch,
                            &call_shape,
                        )
                        .or_else(|| {
                            let resolved =
                                self.resolve_function_call_target(compiled, state, &lowered_name)?;
                            if self.options.inline_caches.enabled()
                                && function_call_target_is_builtin(&resolved)
                            {
                                self.record_counter_builtin_call_ic(false);
                            }
                            self.install_function_call_inline_cache(
                                compiled,
                                function_id,
                                block_id,
                                instruction_id,
                                &interned_name,
                                epoch,
                                call_shape.clone(),
                                resolved.clone(),
                            );
                            Some(resolved)
                        });
                    if let Some(target) = target {
                        self.execute_function_call_target(
                            compiled,
                            target,
                            values,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction_id,
                            )),
                            call_span,
                            output,
                            stack,
                            state,
                            running_fiber,
                        )
                    } else {
                        let diagnostic = undefined_function(
                            &display_name,
                            RuntimeSourceSpan::default(),
                            stack_trace(compiled, stack),
                        );
                        VmResult::runtime_error_with_diagnostic(
                            output.clone(),
                            diagnostic.message().to_owned(),
                            diagnostic,
                        )
                    }
                }
            }
            _ => self.call_callable_with_call_span(
                compiled, callee, values, call_span, output, stack, state,
            ),
        }
    }

    pub(super) fn call_callable(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        self.call_callable_inner(
            compiled, callee, args, None, output, stack, state, false, None,
        )
    }

    pub(super) fn call_callable_with_call_span(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        self.call_callable_inner(
            compiled, callee, args, call_span, output, stack, state, false, None,
        )
    }

    pub(super) fn call_callable_with_by_ref_value_warnings(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        self.call_callable_inner(
            compiled, callee, args, None, output, stack, state, true, None,
        )
    }

    pub(super) fn call_callable_inner(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        allow_by_ref_value_warnings: bool,
        by_ref_warning_callable_name: Option<String>,
    ) -> VmResult {
        match callee {
            Value::Callable(callable) => match *callable {
            CallableValue::UserFunction { name } => {
                let make_call = |args, captures| {
                    let call = FunctionCall::new(args, captures)
                        .with_call_site_strict_types(compiled.unit().strict_types)
                        .with_optional_call_span(call_span);
                    if allow_by_ref_value_warnings {
                        call.with_by_ref_value_warnings()
                    } else {
                        call
                    }
                    .with_optional_by_ref_warning_callable_name(
                        by_ref_warning_callable_name.clone(),
                    )
                };
                if let Some(function) = compiled.lookup_function(&name) {
                    self.execute_function(
                        compiled,
                        function,
                        make_call(args, Vec::new()),
                        output,
                        stack,
                        state,
                    )
                } else if let Some((owner, function)) = dynamic_function_in_state(state, &name) {
                    self.execute_function(
                        &owner,
                        function,
                        make_call(args, Vec::new()),
                        output,
                        stack,
                        state,
                    )
                } else {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_UNRESOLVED_CALLABLE: function {name} is not defined"),
                    )
                }
            }
            CallableValue::Closure(payload) => {
                let mut call = FunctionCall::new(args, payload.captures)
                    .with_call_site_strict_types(compiled.unit().strict_types)
                    .with_optional_call_span(call_span)
                    .with_error_context(compiled.clone());
                let closure_owner = closure_owner_for_function(
                    compiled,
                    state,
                    payload.function,
                    payload.debug.as_deref(),
                    payload.context.owner_unit,
                );
                if let Some(bound_this) = payload.bound_this
                    && closure_function_has_this_local(&closure_owner, payload.function)
                {
                    call = call.with_this(bound_this);
                }
                if let Some(scope_class) = payload.context.scope_class {
                    call = call.with_class_context(
                        scope_class.clone(),
                        payload
                            .context
                            .called_class
                            .unwrap_or_else(|| scope_class.clone()),
                        payload
                            .context
                            .declaring_class
                            .unwrap_or_else(|| scope_class.clone()),
                    );
                } else if let Some(this_value) = call.this_value.as_ref() {
                    let scope_class = this_value.display_name();
                    call =
                        call.with_class_context(scope_class.clone(), scope_class.clone(), scope_class);
                }
                let call = if allow_by_ref_value_warnings {
                    call.with_by_ref_value_warnings()
                } else {
                    call
                }
                .with_optional_by_ref_warning_callable_name(
                    by_ref_warning_callable_name.clone(),
                );
                self.execute_function(
                    &closure_owner,
                    FunctionId::new(payload.function),
                    call,
                    output,
                    stack,
                    state,
                )
            }
            CallableValue::InternalBuiltin { name } => {
                if is_array_callback_builtin_name(&name) {
                    return self.call_array_callback_builtin(
                        compiled, &name, args, call_span, output, stack, state,
                    );
                }
                if is_array_sort_builtin_name(&name) {
                    return self.call_array_sort_builtin(compiled, &name, args, output, stack, state);
                }
                if is_autoload_builtin_name(&name) || is_symbol_introspection_builtin_name(&name) {
                    return self.call_autoload_builtin(
                        compiled, &name, args, None, call_span, output, stack, state,
                    );
                }
                if is_config_builtin_name(&name) {
                    return self.call_config_builtin(compiled, &name, args, output, stack, state);
                }
                if is_error_handling_builtin_name(&name) {
                    return self.call_error_handling_builtin(
                        compiled, &name, args, output, stack, state,
                    );
                }
                if is_output_buffering_builtin_name(&name) {
                    return self.call_output_buffering_builtin(
                        compiled, &name, args, output, stack,
                    );
                }
                if is_environment_builtin_name(&name) {
                    return self.call_environment_builtin(
                        compiled, &name, args, output, stack, state,
                    );
                }
                if is_process_builtin_name(&name) {
                    return self.call_process_builtin(compiled, &name, args, output, stack);
                }
                if is_pcre_callback_builtin_name(&name) {
                    return self.call_pcre_callback_builtin(
                        compiled, &name, args, output, stack, state,
                    );
                }
                let values = match call_builtin_args_to_positional(
                    compiled, &name, args, call_span, output, stack, state,
                ) {
                    Ok(values) => values,
                    Err(InternalBuiltinArgError::Message(message)) => {
                        return self.runtime_error(output, compiled, stack, message);
                    }
                    Err(InternalBuiltinArgError::Fatal(result)) => return *result,
                };
                if let Some(result) = self.try_execute_serialization_builtin(
                    compiled, &name, &values, call_span, output, stack, state,
                ) {
                    return result;
                }
                self.execute_internal_registry_builtin(
                    &name,
                    values,
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
            }
            CallableValue::BoundMethod {
                target,
                method,
                scope,
            } => self.call_bound_method_callable(
                compiled, target, &method, scope, args, call_span, output, stack, state,
            ),
            CallableValue::MethodPlaceholder { target } => self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_UNSUPPORTED_METHOD_CALLABLE: method callable {target} is not implemented"
                ),
            ),
            CallableValue::UnresolvedDynamic { target } => self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNRESOLVED_CALLABLE: callable {target} could not be resolved"),
            ),
            },
            Value::String(name) => self.call_named_callable(
                compiled,
                &name.to_string_lossy(),
                args,
                call_span,
                output,
                stack,
                state,
                allow_by_ref_value_warnings,
                by_ref_warning_callable_name.clone(),
            ),
            Value::Array(array) => {
                self.call_array_callable(
                    compiled,
                    &array,
                    args,
                    call_span,
                    output,
                    stack,
                    state,
                    allow_by_ref_value_warnings,
                )
            }
            Value::Object(object) => {
                self.call_object_callable(compiled, object, args, call_span, output, stack, state)
            }
            other => self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_PIPE_RHS_NOT_CALLABLE: {} is not callable",
                    value_type_name(&other)
                ),
            ),
        }
    }
}
