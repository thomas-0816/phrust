use super::prelude::*;

impl Vm {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_dense_method_call(
        &self,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        object: Value,
        method: &str,
        args: Vec<CallArgument>,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let object = match callable_resolve_reference(object) {
            Value::Generator(generator) => {
                self.record_counter_dense_call_fallback("generator_method_receiver");
                let value = match self
                    .call_generator_method(compiled, generator, method, args, output, stack, state)
                {
                    Ok(value) => value,
                    Err(result) => return result,
                };
                return VmResult::success_no_output(Some(value));
            }
            Value::Fiber(fiber) => {
                self.record_counter_dense_call_fallback("fiber_method_receiver");
                let value = match self
                    .call_fiber_method(compiled, fiber, method, args, output, stack, state)
                {
                    Ok(value) => value,
                    Err(result) => return result,
                };
                return VmResult::success_no_output(Some(value));
            }
            Value::Object(object) => object,
            other => {
                self.record_counter_dense_call_fallback("non_object_method_receiver");
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_METHOD_CALL_NON_OBJECT: Call to a member function {method}() on {}",
                        value_type_name(&other)
                    ),
                );
            }
        };

        let receiver_class = object.class_name();
        if is_mysqli_runtime_class(&receiver_class) {
            // Same inline dispatch as the rich CallMethod arm; the generic
            // object-method callable helper does not cover mysqli.
            self.record_counter_dense_call_fallback("runtime_method_receiver");
            let value = match call_mysqli_method(
                &object,
                method,
                args,
                &mut state.mysql,
                compiled,
                stack,
            ) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            };
            return VmResult::success_no_output(Some(value));
        }
        if is_spl_iterator_runtime_class(&receiver_class)
            || is_spl_container_runtime_class(&receiver_class)
            || is_spl_file_runtime_class(&receiver_class)
            || internal_throwable_instanceof(&receiver_class, "throwable").is_some()
            || is_zip_runtime_class(&receiver_class)
            || is_xml_runtime_class(&receiver_class)
        {
            self.record_counter_dense_call_fallback("runtime_method_receiver");
            return self.call_object_method_callable(
                compiled, object, method, args, call_span, output, stack, state,
            );
        }

        let Some(class) = lookup_class_in_state(compiled, state, &receiver_class) else {
            self.record_counter_dense_call_fallback("unknown_class");
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_CLASS: class {receiver_class} is not defined"),
            );
        };
        let class = class.clone();
        let scope = current_scope_class(compiled, stack);
        let lowered_method = normalize_method_name(method);
        let epoch = state.lookup_epoch();
        let has_magic_call =
            match class_has_public_magic_call_in_state(compiled, state, &receiver_class) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            };

        self.observe_dense_call_inline_cache(
            compiled,
            function_id,
            block_id,
            instruction_id,
            InlineCacheKind::MethodCall,
        );
        let (cached_target, observation) = self.lookup_method_call_inline_cache(
            compiled,
            function_id,
            block_id,
            instruction_id,
            &lowered_method,
            &receiver_class,
            scope.as_deref(),
            epoch,
        );
        if let Some(target) = cached_target {
            self.record_counter_dense_call_ic_hit();
            self.record_counter_dense_method_call_hit();
            return self.execute_method_call_target(
                compiled, target, object, args, call_span, output, stack, state, &None, plan,
            );
        }
        if observation.is_some() {
            self.record_counter_dense_call_ic_miss();
        }

        let resolved = match lookup_resolved_method_in_state(
            compiled,
            state,
            &receiver_class,
            method,
            scope.as_deref(),
        ) {
            Ok(Some(method)) => method,
            Ok(None) => {
                self.record_counter_dense_call_fallback("magic_call");
                return match self.call_magic_instance_method(
                    compiled,
                    object.clone(),
                    "__call",
                    method,
                    args,
                    call_span,
                    output,
                    stack,
                    state,
                ) {
                    Ok(Some(result)) => result,
                    Ok(None) => self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                            object.class_name(),
                            method
                        ),
                    ),
                    Err(result) => result,
                };
            }
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };

        let method_entry = &resolved.method;
        let declaring_class = &resolved.class;
        if (method_entry.flags.is_private || method_entry.flags.is_protected)
            && let Err(message) = validate_method_callable_in_state_scope(
                compiled,
                state,
                scope.as_deref(),
                declaring_class,
                method_entry,
            )
        {
            self.record_counter_dense_call_fallback("visibility");
            return match self.call_magic_instance_method(
                compiled,
                object.clone(),
                "__call",
                method,
                args,
                call_span,
                output,
                stack,
                state,
            ) {
                Ok(Some(result)) => result,
                Ok(None) => self.runtime_error_at_optional_span(
                    compiled, output, stack, state, call_span, message,
                ),
                Err(result) => result,
            };
        }
        if let Err(message) = validate_method_callable_in_state_scope(
            compiled,
            state,
            scope.as_deref(),
            declaring_class,
            method_entry,
        ) {
            return self.runtime_error_at_optional_span(
                compiled, output, stack, state, call_span, message,
            );
        }

        let has_by_ref_argument = dense_call_has_by_ref_argument(&args);
        let method_guard = method_call_guard_metadata(
            &args,
            &class,
            declaring_class,
            method_entry,
            scope.as_deref(),
            epoch,
            has_magic_call,
            has_by_ref_argument,
        );
        let method_target = Box::new(MethodCallResolvedTarget {
            receiver_class: receiver_class.clone(),
            declaring_class: declaring_class.name.clone(),
            function: method_entry.function,
            guard: method_guard,
        });
        let declaring_dynamic_owner_index =
            dynamic_class_owner_index_in_state(state, &declaring_class.name);
        let target = match declaring_dynamic_owner_index {
            Some(unit_index) => MethodCallCacheTarget::DynamicUnit {
                unit_index,
                target: method_target,
            },
            None => MethodCallCacheTarget::CurrentUnit {
                target: method_target,
            },
        };
        self.install_method_call_inline_cache(
            compiled,
            function_id,
            block_id,
            instruction_id,
            &lowered_method,
            &receiver_class,
            scope.as_deref(),
            epoch,
            target,
        );
        self.record_counter_dense_method_call_hit();
        let class_owner = class_owner_in_state(compiled, state, &declaring_class.name);
        self.execute_function_with_dense_plan(
            compiled,
            &class_owner,
            plan,
            method_entry.function,
            FunctionCall::new(args, Vec::new())
                .with_call_site_strict_types(compiled.unit().strict_types)
                .with_optional_call_span(call_span)
                .with_this(object.clone())
                .with_class_context(
                    declaring_class.name.clone(),
                    object.display_name(),
                    declaring_class.name.clone(),
                ),
            output,
            stack,
            state,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_dense_static_method_call(
        &self,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        class_name: &str,
        method: &str,
        args: Vec<CallArgument>,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if is_closure_runtime_class(class_name)
            || is_php_token_runtime_class(class_name)
            || internal_extension_static_class(class_name)
        {
            self.record_counter_dense_call_fallback("runtime_static_method_receiver");
            return self.call_static_method_callable(
                compiled, class_name, method, args, call_span, output, stack, state, false, None,
            );
        }

        // Match the rich-IR static-call arm: an unknown class must attempt
        // registered autoloaders before resolution fails.
        if let Err(result) = self.autoload_static_class_if_missing(
            compiled,
            class_name,
            call_span.unwrap_or_default(),
            Some((
                compiled_unit_cache_key(compiled),
                function_id,
                block_id,
                instruction_id,
            )),
            output,
            stack,
            state,
        ) {
            return result;
        }

        let class = match resolve_static_class_name(compiled, state, stack, class_name) {
            Ok(class) => class,
            Err(message) => {
                self.record_counter_dense_call_fallback("unknown_static_class");
                return self.runtime_error(output, compiled, stack, message);
            }
        };
        let scope = method_lookup_scope_for_static_call(compiled, stack, class_name);
        let lowered_method = normalize_method_name(method);
        let epoch = state.lookup_epoch();
        let receiver_class = class.name.clone();
        let called_class = called_class_for_static_call(compiled, stack, class_name, &class);

        if class.flags.is_enum && matches!(lowered_method.as_str(), "cases" | "from" | "tryfrom") {
            self.record_counter_dense_call_fallback("enum_static_method");
            return match enum_static_method(compiled, state, &class, method, args, &|value| {
                self.constant_value(compiled.unit(), value)
            }) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }

        self.observe_dense_call_inline_cache(
            compiled,
            function_id,
            block_id,
            instruction_id,
            InlineCacheKind::MethodCall,
        );
        let (cached_target, observation) = self.lookup_method_call_inline_cache(
            compiled,
            function_id,
            block_id,
            instruction_id,
            &lowered_method,
            &receiver_class,
            scope.as_deref(),
            epoch,
        );
        if let Some(target) = cached_target {
            self.record_counter_dense_call_ic_hit();
            self.record_counter_dense_static_call_hit();
            return self.execute_static_method_call_target(
                compiled,
                plan,
                target,
                called_class,
                args,
                call_span,
                output,
                stack,
                state,
            );
        }
        if observation.is_some() {
            self.record_counter_dense_call_ic_miss();
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
                self.record_counter_dense_call_fallback("magic_static_call");
                return match self.call_magic_static_method(
                    compiled,
                    &class,
                    "__callStatic",
                    method,
                    args,
                    called_class,
                    call_span,
                    output,
                    stack,
                    state,
                ) {
                    Ok(Some(result)) => result,
                    Ok(None) => self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                            class.name, method
                        ),
                    ),
                    Err(result) => result,
                };
            }
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let method_entry = &resolved.method;
        let declaring_class = &resolved.class;
        let is_constructor_call = lowered_method == "__construct";
        let bound_this_for_scoped_call =
            scoped_static_call_this_object(compiled, state, stack, declaring_class, method_entry);
        if !method_entry.flags.is_static && bound_this_for_scoped_call.is_none() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_NON_STATIC_METHOD_CALL: method {}::{} is not static",
                    declaring_class.name, method_entry.name
                ),
            );
        }
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
            self.record_counter_dense_call_fallback("visibility");
            return match self.call_magic_static_method(
                compiled,
                &class,
                "__callStatic",
                method,
                args,
                called_class,
                call_span,
                output,
                stack,
                state,
            ) {
                Ok(Some(result)) => result,
                Ok(None) => self.runtime_error_at_optional_span(
                    compiled,
                    output,
                    stack,
                    state,
                    call_span,
                    inaccessible,
                ),
                Err(result) => result,
            };
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
            return self.runtime_error_at_optional_span(
                compiled, output, stack, state, call_span, message,
            );
        }

        let has_magic_call =
            match class_has_public_magic_call_static_in_state(compiled, state, &class.name) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            };
        let method_guard = method_call_guard_metadata(
            &args,
            &class,
            declaring_class,
            method_entry,
            scope.as_deref(),
            epoch,
            has_magic_call,
            dense_call_has_by_ref_argument(&args),
        );
        let method_target = Box::new(MethodCallResolvedTarget {
            receiver_class: receiver_class.clone(),
            declaring_class: declaring_class.name.clone(),
            function: method_entry.function,
            guard: method_guard,
        });
        let declaring_dynamic_owner_index =
            dynamic_class_owner_index_in_state(state, &declaring_class.name);
        let target = match declaring_dynamic_owner_index {
            Some(unit_index) => MethodCallCacheTarget::DynamicUnit {
                unit_index,
                target: method_target,
            },
            None => MethodCallCacheTarget::CurrentUnit {
                target: method_target,
            },
        };
        self.install_method_call_inline_cache(
            compiled,
            function_id,
            block_id,
            instruction_id,
            &lowered_method,
            &receiver_class,
            scope.as_deref(),
            epoch,
            target,
        );
        self.record_counter_dense_static_call_hit();
        let class_owner = class_owner_in_state(compiled, state, &declaring_class.name);
        self.execute_function_with_dense_plan(
            compiled,
            &class_owner,
            plan,
            method_entry.function,
            {
                let mut call = FunctionCall::new(args, Vec::new())
                    .with_call_site_strict_types(compiled.unit().strict_types)
                    .with_class_context(
                        declaring_class.name.clone(),
                        called_class,
                        declaring_class.name.clone(),
                    )
                    .with_optional_call_span(call_span);
                if let Some(bound_this) = bound_this_for_scoped_call {
                    call = call.with_this(bound_this);
                }
                call
            },
            output,
            stack,
            state,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_static_method_call_target(
        &self,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        target: MethodCallCacheTarget,
        called_class: String,
        args: Vec<CallArgument>,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let declaring_class_name = target.resolved_target().declaring_class.clone();
        let function = target.resolved_target().function;
        let owner = match target {
            MethodCallCacheTarget::CurrentUnit { .. } => compiled.clone(),
            MethodCallCacheTarget::DynamicUnit { unit_index, .. } => {
                let Some(owner) = state.dynamic_units.get(unit_index).cloned() else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_INLINE_CACHE_STALE_DYNAMIC_UNIT: dynamic unit {unit_index} is unavailable"
                        ),
                    );
                };
                owner
            }
        };
        let Some(declaring_class) = owner.lookup_class(&declaring_class_name).cloned() else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_INLINE_CACHE_STALE_METHOD_CLASS: class {declaring_class_name} is unavailable"
                ),
            );
        };
        let Some(method_entry) = declaring_class
            .methods
            .iter()
            .find(|method| method.function == function)
            .cloned()
        else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_INLINE_CACHE_STALE_METHOD: method target {}#{} is unavailable",
                    declaring_class.name,
                    function.index()
                ),
            );
        };
        let is_constructor_call = normalize_method_name(&method_entry.name) == "__construct";
        let bound_this_for_scoped_call =
            scoped_static_call_this_object(compiled, state, stack, &declaring_class, &method_entry);
        if !method_entry.flags.is_static && bound_this_for_scoped_call.is_none() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_NON_STATIC_METHOD_CALL: method {}::{} is not static",
                    declaring_class.name, method_entry.name
                ),
            );
        }
        let visibility = if is_constructor_call {
            validate_scoped_constructor_callable_in_state_scope(
                compiled,
                state,
                current_scope_class(compiled, stack).as_deref(),
                &declaring_class,
                &method_entry,
            )
        } else {
            validate_method_callable_in_state_scope(
                compiled,
                state,
                current_scope_class(compiled, stack).as_deref(),
                &declaring_class,
                &method_entry,
            )
        };
        if let Err(message) = visibility {
            return self.runtime_error_at_optional_span(
                compiled, output, stack, state, call_span, message,
            );
        }
        self.execute_function_with_dense_plan(
            compiled,
            &owner,
            plan,
            method_entry.function,
            {
                let mut call = FunctionCall::new(args, Vec::new())
                    .with_call_site_strict_types(compiled.unit().strict_types)
                    .with_optional_call_span(call_span)
                    .with_class_context(
                        declaring_class.name.clone(),
                        called_class,
                        declaring_class.name,
                    );
                if let Some(bound_this) = bound_this_for_scoped_call {
                    call = call.with_this(bound_this);
                }
                call
            },
            output,
            stack,
            state,
        )
    }

    /// Dense `new` over the same userland-instantiation helpers as the
    /// rich-IR arm: class lookup with autoload, runtime class-entry
    /// construction, instantiation guards, constructor resolution and
    /// dispatch, SPL-subclass storage, and destructor registration.
    /// Builtin runtime classes never reach this path: dense lowering
    /// rejects their names (`dense_new_object_lowering_supported`), so
    /// their dedicated construction paths stay on the rich interpreter.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_dense_new_object(
        &self,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        class_name: &str,
        display_class_name: &str,
        args: Vec<CallArgument>,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        // Autoloaders receive the source-case class name (the reference
        // engine passes the spelling at the new-expression site); the
        // normalized name would fail case-sensitive loader comparisons.
        if let Err(result) = self.autoload_static_class_if_missing(
            compiled,
            display_class_name,
            call_span.unwrap_or_default(),
            Some((
                compiled_unit_cache_key(compiled),
                function_id,
                block_id,
                instruction_id,
            )),
            output,
            stack,
            state,
        ) {
            return result;
        }
        let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"),
            );
        };
        let class = class.clone();
        if let Err(result) =
            self.autoload_class_parents_if_missing(compiled, &class, output, stack, state)
        {
            return result;
        }
        let class_owner = class_owner_in_state(compiled, state, &class.name);
        let runtime_class = match runtime_class_entry(
            &class_owner,
            state,
            &class,
            &|value| self.constant_value(class_owner.unit(), value),
            &|reference| class_constant_reference_value(&class_owner, state, reference),
            &|reference| named_constant_reference_value(&class_owner, state, reference),
        ) {
            Ok(runtime_class) => runtime_class,
            Err(error) => {
                let location_span = error
                    .constant_initializer_span
                    .unwrap_or_else(|| call_span.unwrap_or_default());
                let result = self.runtime_error(output, compiled, stack, error.message);
                if let Some(throwable) = runtime_error_throwable(&result) {
                    tag_throwable_location(&throwable, compiled, location_span);
                    state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                }
                return result;
            }
        };
        if let Err(message) = validate_object_mvp(&runtime_class) {
            return self.runtime_error_at_optional_span(
                compiled, output, stack, state, call_span, message,
            );
        }
        let spl_runtime_parent = spl_runtime_parent_for_class(compiled, state, &class);
        let object = ObjectRef::new_with_display_name(&runtime_class, display_class_name);
        if let Some(spl_class) = spl_runtime_parent.as_deref() {
            object.set_property(
                SPL_RUNTIME_CLASS_PROPERTY,
                Value::string(spl_class.as_bytes().to_vec()),
            );
        }
        let caller_scope = current_scope_class(compiled, stack);
        let constructor = match lookup_resolved_method_in_state(
            compiled,
            state,
            &class.name,
            "__construct",
            caller_scope.as_deref(),
        ) {
            Ok(constructor) => constructor,
            Err(message) => {
                return self.runtime_error(output, compiled, stack, message);
            }
        };
        let mut constructor_diagnostics = Vec::new();
        if let Some(constructor) = constructor {
            if let Err(message) = validate_constructor_callable_in_state_scope(
                compiled,
                state,
                caller_scope.as_deref(),
                &constructor.class,
                &constructor.method,
            ) {
                return self.runtime_error_at_optional_span(
                    compiled, output, stack, state, call_span, message,
                );
            }
            let class_owner = dynamic_class_owner_in_state(state, &constructor.class.name)
                .unwrap_or_else(|| compiled.clone());
            let result = self.execute_function_with_dense_plan(
                compiled,
                &class_owner,
                plan,
                constructor.method.function,
                FunctionCall::new(args, Vec::new())
                    .with_call_site_strict_types(compiled.unit().strict_types)
                    .with_optional_call_span(call_span)
                    .with_this(object.clone())
                    .with_class_context(
                        constructor.class.name.clone(),
                        object.display_name(),
                        constructor.class.name.clone(),
                    ),
                output,
                stack,
                state,
            );
            if !result.status.is_success() {
                return result;
            }
            if result.fiber_suspension.is_some() {
                return VmResult::unsupported(
                    output.clone(),
                    "E_PHP_VM_DENSE_BYTECODE_NEW_FIBER_UNSUPPORTED: dense bytecode object construction does not support fiber suspension yet",
                );
            }
            constructor_diagnostics = result.diagnostics;
        } else if let Some(spl_class) = spl_runtime_parent
            && let Err(message) = initialize_spl_runtime_subclass_storage(
                &object,
                &spl_class,
                args,
                &self.options.runtime_context,
                Some(&mut state.resources),
            )
        {
            return self.runtime_error_at_optional_span(
                compiled, output, stack, state, call_span, message,
            );
        }
        self.register_destructor_if_needed(compiled, &class, object.clone(), state);
        let mut result = VmResult::success_no_output(Some(Value::Object(object)));
        result.diagnostics = constructor_diagnostics;
        result
    }
}
