//! Method route selection and object/static method invocation.

use super::builtin_adapter::builtin_source_span;
use super::builtin_fileinfo::FileinfoMethodCall;
use super::prelude::*;

impl Vm {
    pub(super) fn try_inline_trivial_method(
        &self,
        owner: &CompiledUnit,
        method_function: FunctionId,
        declaring_class: &php_ir::module::ClassEntry,
        object: &ObjectRef,
        args: &[CallArgument],
    ) -> Option<VmResult> {
        let key = (compiled_unit_cache_key(owner), method_function.raw());
        let plan = {
            let plans = self.trivial_method_plans.borrow();
            plans.get(&key).cloned()
        };
        let plan = match plan {
            Some(plan) => plan,
            None => {
                let plan = owner
                    .unit()
                    .functions
                    .get(method_function.index())
                    .and_then(classify_trivial_method);
                if plan.is_some() {
                    self.record_counter_method_inline("candidate", None);
                }
                self.trivial_method_plans
                    .borrow_mut()
                    .insert(key, plan.clone());
                plan
            }
        }?;
        // The classifier guarantees by-value parameters, so the by-ref
        // capability metadata carried by local-sourced arguments is inert;
        // named arguments keep generic binding.
        if args.iter().any(|arg| arg.name.is_some()) {
            self.record_counter_method_inline("fallback", Some("argument_shape"));
            return None;
        }
        match plan {
            TrivialMethodPlan::Getter { property } => {
                if !args.is_empty() {
                    self.record_counter_method_inline("fallback", Some("argument_shape"));
                    return None;
                }
                let slot = match object.declared_slot_index(&property) {
                    Some(slot) => slot,
                    None => {
                        self.record_counter_method_inline("fallback", Some("slot_missing"));
                        return None;
                    }
                };
                match object.get_declared_slot(slot, object.class_layout_epoch()) {
                    Some(Value::Uninitialized) | None => {
                        self.record_counter_method_inline(
                            "fallback",
                            Some("uninitialized_or_unset"),
                        );
                        None
                    }
                    Some(value) => {
                        self.record_counter_method_inline("hit", None);
                        Some(VmResult::success_no_output(Some(value)))
                    }
                }
            }
            TrivialMethodPlan::Setter {
                property,
                returns_this,
            } => {
                if args.len() != 1 {
                    self.record_counter_method_inline("fallback", Some("argument_shape"));
                    return None;
                }
                let Some(entry) = declaring_class
                    .properties
                    .iter()
                    .find(|entry| entry.name == property)
                else {
                    self.record_counter_method_inline("fallback", Some("slot_missing"));
                    return None;
                };
                if entry.type_.is_some()
                    || entry.flags.is_readonly
                    || entry.flags.is_static
                    || entry.flags.set_is_private
                    || entry.flags.set_is_protected
                    || entry.hooks.get.is_some()
                    || entry.hooks.set.is_some()
                    || declaring_class.flags.is_readonly
                {
                    self.record_counter_method_inline("fallback", Some("guarded_property"));
                    return None;
                }
                let slot = match object.declared_slot_index(&property) {
                    Some(slot) => slot,
                    None => {
                        self.record_counter_method_inline("fallback", Some("slot_missing"));
                        return None;
                    }
                };
                let epoch = object.class_layout_epoch();
                if matches!(
                    object.get_declared_slot(slot, epoch),
                    Some(Value::Reference(_)) | None
                ) {
                    self.record_counter_method_inline("fallback", Some("reference_or_unset"));
                    return None;
                }
                // Mirror argument binding: a reference argument passes its
                // current value to a by-value parameter.
                let value = match &args[0].value {
                    Value::Reference(cell) => cell.get(),
                    other => other.clone(),
                };
                if !object.set_declared_slot(slot, epoch, value) {
                    self.record_counter_method_inline("fallback", Some("slot_missing"));
                    return None;
                }
                self.record_counter_method_inline("hit", None);
                let value = if returns_this {
                    Value::Object(object.clone())
                } else {
                    Value::Null
                };
                Some(VmResult::success_no_output(Some(value)))
            }
        }
    }

    /// Executes a resolved userland body through its dense plan when the
    /// owning unit is the current unit and the plan marks the function
    /// dense; anything unproven falls back to the rich interpreter with an
    /// attributed reason. Dense callers thread method, static-method, and
    /// constructor bodies through here so they stop silently dropping to
    /// rich execution.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn method_dispatch_route(
        &self,
        owner: &CompiledUnit,
        function: FunctionId,
        declaring_class: &php_ir::module::ClassEntry,
    ) -> Option<MethodCallDispatchRoute> {
        let canonical_class = owner.lookup_class_arc(&declaring_class.name)?;
        if canonical_class.id != declaring_class.id {
            return None;
        }
        let method_slot_index = canonical_class
            .methods
            .iter()
            .position(|method| method.function == function)?
            .try_into()
            .ok()?;
        let plan = self.get_or_build_dense_execution_plan(owner).ok()?;
        if !matches!(
            plan.function_plan(function.index()),
            Some(DenseFunctionPlan::Dense)
        ) {
            return None;
        }
        Some(MethodCallDispatchRoute {
            identity: MethodCallRouteIdentity {
                owner_unit_identity: owner.cache_identity(),
                declaring_class_id: canonical_class.id,
                function,
                method_slot_index,
            },
            owner: owner.clone(),
            plan,
            declaring_class: canonical_class,
            declaring_class_handle: self.class_name_handles(&declaring_class.name).normalized,
        })
    }

    /// Dispatches a routed method call straight into the dense executor. The
    /// fill-time route guarantees the function is planned dense in `plan`;
    /// call-context continuations still take the general path.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_routed_dense_method(
        &self,
        route: &MethodCallDispatchRoute,
        function: FunctionId,
        call: FunctionCall<'_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let owner = &route.owner;
        let plan = route.plan.as_ref();
        #[cfg(feature = "jit-copy-patch")]
        if let Some(ir_function) = owner.unit().functions.get(function.index())
            && let Some(result) = self.try_execute_copy_patch_leaf(
                owner,
                function,
                ir_function,
                &call,
                output,
                stack,
                state,
            )
        {
            return result;
        }
        self.record_counter_dense_method_dispatch_attempt();
        if call.resume_continuation.is_some()
            || call.resume_fiber_continuation.is_some()
            || call.running_generator.is_some()
            || call.running_fiber.is_some()
        {
            self.record_counter_dense_method_dispatch_fallback("generator_or_fiber_context");
            return self.execute_function(owner, function, call, output, stack, state);
        }
        let (Some(dense_function), Some(ir_function)) = (
            plan.unit.functions.get(function.index()),
            owner.unit().functions.get(function.index()),
        ) else {
            self.record_counter_dense_method_dispatch_fallback("dense_body_missing");
            return self.execute_function(owner, function, call, output, stack, state);
        };
        self.record_counter_dense_method_dispatch_hit();
        let profile_boundary = self.request_profile_boundary_start();
        let function_profile = profile_boundary
            .is_some()
            .then(|| (ir_function.name.clone(), ir_function.flags.is_method));
        let result = self.execute_bytecode_function(
            DenseExecutionRequest {
                compiled: owner,
                dense: &plan.unit,
                plan: Some(plan),
                dense_function,
                ir_function,
                function_id: function,
                call,
            },
            output,
            stack,
            state,
        );
        if let Some((name, is_method)) = function_profile {
            self.record_counter_function_profile(&name, is_method, profile_boundary);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_method_call_target(
        &self,
        compiled: &CompiledUnit,
        target: MethodCallCacheTarget,
        object: ObjectRef,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        running_fiber: &Option<FiberRef>,
        dense_plan: Option<&DenseExecutionPlan>,
    ) -> VmResult {
        // Routed fast path: the cache key pinned method, receiver, scope, and
        // epoch at fill time, and the guard epochs invalidate on class-table
        // changes — so a hit re-uses the resolved owner, plan, and class
        // without lookups or re-validation.
        if let Some(route) = target.resolved_target().route.clone() {
            let function = target.resolved_target().function;
            if let Some(result) = self.try_inline_trivial_method(
                &route.owner,
                function,
                &route.declaring_class,
                &object,
                &args,
            ) {
                return result;
            }
            let call = FunctionCall::new(args, Vec::new())
                .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                .with_optional_call_span(call_span)
                .with_this(object.clone())
                .with_class_context_handles(
                    route.declaring_class_handle.clone(),
                    object_called_class_handle(&object),
                    route.declaring_class_handle.clone(),
                )
                .inherit_fiber_context(running_fiber);
            return self.execute_routed_dense_method(&route, function, call, output, stack, state);
        }
        let declaring_class_name = target.resolved_target().declaring_class.clone();
        let function = target.resolved_target().function;
        let owner = match target {
            MethodCallCacheTarget::CurrentUnit { .. } => {
                class_owner_in_state(compiled, state, &declaring_class_name)
            }
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
        let Some(declaring_class) = owner.lookup_class(&declaring_class_name).or_else(|| {
            dynamic_class_entry_in_state(state, &declaring_class_name)
                .map(|entry| entry.class.as_ref())
        }) else {
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
        // A static method reached through an instance/callable runs as a static
        // call; PHP allows this, so do not reject it here.
        if let Err(message) = validate_method_callable_in_state_scope(
            compiled,
            state,
            current_scope_class(compiled, stack).as_deref(),
            declaring_class,
            method_entry,
        ) {
            return self.runtime_error(output, compiled, stack, message);
        }
        let method_function = method_entry.function;
        if let Some(result) =
            self.try_inline_trivial_method(&owner, method_function, declaring_class, &object, &args)
        {
            return result;
        }
        let declaring_class_name = declaring_class.name.clone();
        self.execute_function_with_dense_plan(
            compiled,
            &owner,
            dense_plan,
            method_function,
            FunctionCall::new(args, Vec::new())
                .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                .with_optional_call_span(call_span)
                .with_this(object.clone())
                .with_class_context_handles(
                    self.class_name_handles(&declaring_class_name).normalized,
                    object_called_class_handle(&object),
                    self.class_name_handles(&declaring_class_name).normalized,
                )
                .inherit_fiber_context(running_fiber),
            output,
            stack,
            state,
        )
    }

    pub(super) fn call_bound_method_callable(
        &self,
        compiled: &CompiledUnit,
        target: CallableMethodTarget,
        method: &str,
        scope: Option<String>,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        match target {
            CallableMethodTarget::Object(object) => self.call_bound_object_method_callable(
                compiled, object, method, scope, args, call_span, output, stack, state,
            ),
            CallableMethodTarget::Class(class_name) => self.call_bound_static_method_callable(
                compiled,
                &class_name,
                method,
                scope,
                args,
                call_span,
                output,
                stack,
                state,
            ),
        }
    }

    pub(super) fn call_bound_object_method_callable(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        method: &str,
        scope: Option<String>,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let Some(_class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
                    object.class_name()
                ),
            );
        };
        let resolved = match lookup_resolved_method_in_state(
            compiled,
            state,
            &object.class_name(),
            method,
            scope.as_deref(),
        ) {
            Ok(Some(method)) => method,
            Ok(None) => {
                if matches!(
                    spl_runtime_marker(&object).as_deref(),
                    Some("recursiveiteratoriterator" | "recursivetreeiterator")
                ) && matches!(
                    normalize_method_name(method).as_str(),
                    "rewind"
                        | "valid"
                        | "current"
                        | "next"
                        | "callhaschildren"
                        | "callgetchildren"
                        | "beginchildren"
                        | "endchildren"
                ) {
                    return match self.call_spl_recursive_iterator_iterator_method(
                        compiled, object, method, args, call_span, output, stack, state,
                    ) {
                        Ok(value) => VmResult::success_no_output(Some(value)),
                        Err(result) => result,
                    };
                }
                if let Some(inner) = spl_inner_iterator_delegation_target(&object)
                    && spl_delegation_target_supports_method(compiled, state, &inner, method)
                {
                    return self.call_object_method_callable(
                        compiled, inner, method, args, call_span, output, stack, state,
                    );
                }
                if class_is_or_extends_internal_throwable_in_state(
                    compiled,
                    state,
                    &object.class_name(),
                )
                .unwrap_or(false)
                {
                    return match internal_throwable_method_value(
                        &object,
                        method,
                        args.into_iter().map(|arg| arg.value).collect(),
                    ) {
                        Ok(value) => VmResult::success_no_output(Some(value)),
                        Err(message) => self.runtime_error(output, compiled, stack, message),
                    };
                }
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
                            "E_PHP_VM_UNKNOWN_METHOD: Call to undefined method {}::{}()",
                            object.display_name(),
                            method
                        ),
                    ),
                    Err(result) => result,
                };
            }
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if let Err(message) = validate_method_callable_in_state_scope(
            compiled,
            state,
            scope.as_deref(),
            &resolved.class,
            &resolved.method,
        ) {
            return self.runtime_error(output, compiled, stack, message);
        }
        let class_owner = class_owner_in_state(compiled, state, &resolved.class.name);
        self.execute_function(
            &class_owner,
            resolved.method.function,
            FunctionCall::new(args, Vec::new())
                .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                .with_optional_call_span(call_span)
                .with_this(object.clone())
                .with_class_context_handles(
                    self.class_name_handles(&resolved.class.name).normalized,
                    object_called_class_handle(&object),
                    self.class_name_handles(&resolved.class.name).normalized,
                ),
            output,
            stack,
            state,
        )
    }

    pub(super) fn call_bound_static_method_callable(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        method: &str,
        scope: Option<String>,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"),
            );
        };
        let resolved = match lookup_resolved_method_in_state(
            compiled,
            state,
            class_name,
            method,
            scope.as_deref(),
        ) {
            Ok(Some(method)) => method,
            Ok(None) => {
                let called_class = class.display_name.clone();
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
                            class.display_name, method
                        ),
                    ),
                    Err(result) => result,
                };
            }
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if !resolved.method.flags.is_static {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_NON_STATIC_METHOD_CALL: Non-static method {}::{}() cannot be called statically",
                    resolved.class.display_name, method
                ),
            );
        }
        if let Err(message) = validate_method_callable_in_state_scope(
            compiled,
            state,
            scope.as_deref(),
            &resolved.class,
            &resolved.method,
        ) {
            return self.runtime_error(output, compiled, stack, message);
        }
        let class_owner = class_owner_in_state(compiled, state, &resolved.class.name);
        self.execute_function(
            &class_owner,
            resolved.method.function,
            FunctionCall::new(args, Vec::new())
                .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                .with_class_context_handles(
                    self.class_name_handles(&resolved.class.name).normalized,
                    self.class_name_handles(&class.display_name).display,
                    self.class_name_handles(&resolved.class.name).normalized,
                )
                .with_optional_call_span(call_span),
            output,
            stack,
            state,
        )
    }

    pub(super) fn call_object_callable(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        self.call_object_method_callable(
            compiled, object, "__invoke", args, call_span, output, stack, state,
        )
    }

    pub(super) fn call_object_method_callable(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if is_hash_context_runtime_class(&object.class_name())
            && hash_context_method_is_supported(method)
        {
            return match self.call_hash_context_method(&object, method, &args) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if is_php_token_runtime_class(&object.class_name()) {
            let values = args.into_iter().map(|arg| arg.value).collect::<Vec<_>>();
            let value = match php_token_method_value(&object, method, values) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            };
            return VmResult::success_no_output(Some(value));
        }
        if is_date_time_runtime_class(&object.class_name()) {
            return match call_date_time_method(object, method, args) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if spl_runtime_marker(&object).is_some_and(|class| is_spl_heap_runtime_class(&class))
            && spl_heap_method_is_supported(method)
            && normalize_method_name(method) != "compare"
        {
            match self.spl_object_has_userland_method(compiled, state, &object, method) {
                Ok(false) => {
                    return match self
                        .call_spl_heap_method(compiled, object, method, args, output, stack, state)
                    {
                        Ok(value) => VmResult::success_no_output(Some(value)),
                        Err(SplHeapMethodError::Message(message)) => {
                            self.runtime_error(output, compiled, stack, message)
                        }
                        Err(SplHeapMethodError::Runtime(result)) => *result,
                    };
                }
                Ok(true) => {}
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            }
        }
        if spl_runtime_marker(&object).is_some_and(|class| {
            is_spl_file_runtime_class(&class) && spl_file_method_is_supported(method)
        }) && normalize_method_name(method) == "fpassthru"
            && !spl_file_is_initialized(&object)
        {
            return match call_spl_file_method_in_state(
                compiled,
                state,
                &object,
                method,
                args,
                &self.options.runtime_context,
            ) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
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
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        self.record_runtime_trace_event(|| {
                            format!(
                                "object-dispatch class={} method={} declaring_class={}",
                                object.class_name(),
                                resolved.method.name,
                                resolved.class.name
                            )
                        });
                        let class_owner =
                            class_owner_in_state(compiled, state, &resolved.class.name);
                        return self.execute_function(
                            &class_owner,
                            resolved.method.function,
                            FunctionCall::new(args, Vec::new())
                                .with_call_site_strict_types(call_site_strictness(
                                    compiled, call_span,
                                ))
                                .with_optional_call_span(call_span)
                                .with_this(object.clone())
                                .with_class_context_handles(
                                    self.class_name_handles(&resolved.class.name).normalized,
                                    object_called_class_handle(&object),
                                    self.class_name_handles(&resolved.class.name).normalized,
                                ),
                            output,
                            stack,
                            state,
                        );
                    }
                    Ok(_) => {}
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
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
                return match self.call_spl_append_iterator_method(
                    compiled, &object, method, args, output, stack, state, call_span,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(result) => result,
                };
            }
            if spl_runtime_marker(&object).as_deref() == Some("multipleiterator")
                && matches!(
                    normalize_method_name(method).as_str(),
                    "attachiterator" | "additerator" | "offsetset"
                )
            {
                return match self.call_spl_multiple_iterator_attach_method(
                    compiled, &object, method, args, output, stack,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(result) => result,
                };
            }
            if spl_runtime_marker(&object).as_deref() == Some("multipleiterator")
                && matches!(
                    normalize_method_name(method).as_str(),
                    "rewind" | "valid" | "current" | "key" | "next"
                )
            {
                return match self.call_spl_multiple_iterator_method(
                    compiled, &object, method, args, output, stack, state,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(result) => result,
                };
            }
            if spl_runtime_marker(&object).as_deref() == Some("limititerator")
                && spl_limit_iterator_uses_live_inner(&object)
                && matches!(
                    normalize_method_name(method).as_str(),
                    "rewind" | "valid" | "current" | "key" | "next" | "seek" | "getposition"
                )
            {
                return match self.call_spl_limit_iterator_method(
                    compiled, &object, method, args, output, stack, state,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(result) => result,
                };
            }
            if spl_runtime_marker(&object)
                .is_some_and(|class| is_spl_caching_iterator_class(&class))
                && spl_caching_iterator_uses_live_inner(&object)
                && matches!(
                    normalize_method_name(method).as_str(),
                    "rewind" | "valid" | "current" | "key" | "next"
                )
            {
                return match self.call_spl_caching_iterator_method(
                    compiled, &object, method, args, output, stack, state,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(result) => result,
                };
            }
            if spl_runtime_marker(&object).as_deref() == Some("norewinditerator")
                && matches!(
                    normalize_method_name(method).as_str(),
                    "rewind" | "valid" | "current" | "key" | "next"
                )
            {
                return match self.call_spl_no_rewind_iterator_method(
                    compiled, &object, method, args, output, stack, state,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(result) => result,
                };
            }
            if spl_runtime_marker(&object).as_deref() == Some("infiniteiterator")
                && matches!(
                    normalize_method_name(method).as_str(),
                    "rewind" | "valid" | "current" | "key" | "next"
                )
            {
                return match self.call_spl_infinite_iterator_method(
                    compiled, &object, method, args, output, stack, state,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(result) => result,
                };
            }
            if spl_runtime_marker(&object)
                .is_some_and(|class| is_spl_caching_iterator_class(&class))
                && normalize_method_name(method) == "__tostring"
            {
                if let Err(message) =
                    validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0)
                {
                    return self.runtime_error(output, compiled, stack, message);
                }
                return match self.spl_caching_iterator_to_string(
                    compiled,
                    &object,
                    builtin_source_span(compiled, call_span),
                    output,
                    stack,
                    state,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(Value::String(value))),
                    Err(result) => result,
                };
            }
            if spl_runtime_marker(&object)
                .is_some_and(|class| is_spl_caching_iterator_class(&class))
                && matches!(
                    normalize_method_name(method).as_str(),
                    "offsetget" | "offsetexists"
                )
            {
                return self.call_spl_caching_iterator_offset_access_method(
                    compiled, &object, method, args, call_span, output, stack, state,
                );
            }
            if normalize_method_name(method) == "valid"
                && spl_filtering_iterator_accepts_current(&object)
                && self
                    .spl_object_has_userland_method(compiled, state, &object, "accept")
                    .unwrap_or(false)
            {
                return match self
                    .call_spl_userland_filter_valid(compiled, object, output, stack, state)
                {
                    Ok(value) => VmResult::success_no_output(Some(Value::Bool(value))),
                    Err(result) => result,
                };
            }
            if matches!(
                spl_runtime_marker(&object).as_deref(),
                Some("recursiveiteratoriterator" | "recursivetreeiterator")
            ) && matches!(
                normalize_method_name(method).as_str(),
                "rewind"
                    | "valid"
                    | "current"
                    | "next"
                    | "callhaschildren"
                    | "callgetchildren"
                    | "beginchildren"
                    | "endchildren"
            ) {
                return match self.call_spl_recursive_iterator_iterator_method(
                    compiled, object, method, args, call_span, output, stack, state,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(result) => result,
                };
            }
            return match call_spl_iterator_method(
                object.clone(),
                method,
                args,
                &self.options.runtime_context,
            ) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if spl_runtime_marker(&object).is_some_and(|class| {
            is_spl_container_runtime_class(&class) && spl_container_method_is_supported(method)
        }) {
            return match self.call_spl_container_method_with_magic(
                compiled, object, method, args, None, output, stack, state,
            ) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(result) => result,
            };
        }
        if spl_runtime_marker(&object).is_some_and(|class| {
            is_spl_heap_runtime_class(&class) && spl_heap_method_is_supported(method)
        }) {
            return match self
                .call_spl_heap_method(compiled, object, method, args, output, stack, state)
            {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(SplHeapMethodError::Message(message)) => {
                    self.runtime_error(output, compiled, stack, message)
                }
                Err(SplHeapMethodError::Runtime(result)) => *result,
            };
        }
        if spl_runtime_marker(&object).is_some_and(|class| {
            is_spl_file_runtime_class(&class) && spl_file_method_is_supported(method)
        }) {
            return match call_spl_file_method_in_state(
                compiled,
                state,
                &object,
                method,
                args,
                &self.options.runtime_context,
            ) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if internal_throwable_instanceof(&object.class_name_handle(), "throwable").is_some() {
            return match internal_throwable_method_value(
                &object,
                method,
                args.into_iter().map(|arg| arg.value).collect(),
            ) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if is_pdo_runtime_class(&object.class_name()) {
            return match call_pdo_method(
                &object,
                method,
                args,
                &mut state.builtins.sqlite,
                &mut state.builtins.mysql,
                &mut state.builtins.postgres,
                &self.options.runtime_context,
            ) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if is_sqlite_runtime_class(&object.class_name()) {
            return match call_sqlite_method(
                &object,
                method,
                args,
                &mut state.builtins.sqlite,
                &self.options.runtime_context,
            ) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if is_redis_runtime_class(&object.class_name()) {
            return match call_redis_method(&object, method, args, &mut state.builtins.redis_clients)
            {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if is_memcached_runtime_class(&object.class_name()) {
            return match call_memcached_method(
                &object,
                method,
                args,
                &mut state.builtins.memcached_clients,
            ) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if is_soap_runtime_class(&object.class_name()) {
            return match call_soap_method(&object, method, args) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if is_fileinfo_runtime_class(&object.class_name()) {
            return FileinfoMethodCall {
                vm: self,
                compiled,
                object,
                method,
                call_span,
                output,
                stack,
                state,
            }
            .execute(args);
        }
        if is_phar_runtime_class(&object.class_name()) {
            return match call_phar_method(&object, method, args, &self.options.runtime_context) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if is_zip_runtime_class(&object.class_name()) {
            if zip_open_uses_empty_file(method, &args, &self.options.runtime_context) {
                emit_zip_open_empty_file_deprecation(
                    compiled,
                    output,
                    stack,
                    state,
                    builtin_source_span(compiled, call_span),
                );
            }
            return match call_zip_method(&object, method, args, &self.options.runtime_context) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        if is_xml_runtime_class(&object.class_name()) {
            let values = args.into_iter().map(|arg| arg.value).collect();
            return match call_xml_runtime_method(
                &object,
                method,
                values,
                &self.options.runtime_context,
            ) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }
        let Some(_class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
                    object.class_name()
                ),
            );
        };
        let scope = current_scope_class(compiled, stack);
        let resolved = match lookup_resolved_method_in_state(
            compiled,
            state,
            &object.class_name(),
            method,
            scope.as_deref(),
        ) {
            Ok(Some(method)) => method,
            Ok(None) => {
                if let Some(inner) = spl_inner_iterator_delegation_target(&object)
                    && (spl_delegation_target_supports_method(compiled, state, &inner, method)
                        || match self
                            .spl_iterator_chain_has_userland_method(compiled, state, &inner, method)
                        {
                            Ok(result) => result,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        })
                {
                    return self.call_object_method_callable(
                        compiled, inner, method, args, call_span, output, stack, state,
                    );
                }
                if class_is_or_extends_internal_throwable_in_state(
                    compiled,
                    state,
                    &object.class_name(),
                )
                .unwrap_or(false)
                {
                    return match internal_throwable_method_value(
                        &object,
                        method,
                        args.into_iter().map(|arg| arg.value).collect(),
                    ) {
                        Ok(value) => VmResult::success_no_output(Some(value)),
                        Err(message) => self.runtime_error(output, compiled, stack, message),
                    };
                }
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
        // PHP allows reaching a static method through an instance; run it.
        if (method_entry.flags.is_private || method_entry.flags.is_protected)
            && let Err(message) = validate_method_callable_in_state_scope(
                compiled,
                state,
                scope.as_deref(),
                declaring_class,
                method_entry,
            )
        {
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
                Ok(None) => self.runtime_error(output, compiled, stack, message),
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
            return self.runtime_error(output, compiled, stack, message);
        }
        self.record_runtime_trace_event(|| {
            format!(
                "object-dispatch class={} method={} declaring_class={}",
                object.class_name(),
                method_entry.name,
                declaring_class.name
            )
        });
        let class_owner = class_owner_in_state(compiled, state, &declaring_class.name);
        self.execute_function(
            &class_owner,
            method_entry.function,
            FunctionCall::new(args, Vec::new())
                .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                .with_optional_call_span(call_span)
                .with_this(object.clone())
                .with_class_context_handles(
                    self.class_name_handles(&declaring_class.name).normalized,
                    object_called_class_handle(&object),
                    self.class_name_handles(&declaring_class.name).normalized,
                ),
            output,
            stack,
            state,
        )
    }

    pub(super) fn call_object_method_value(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        method: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let result = self.call_object_method_callable(
            compiled,
            object,
            method,
            Vec::new(),
            None,
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

    pub(super) fn call_object_method_value_with_positional_args(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        method: &str,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let result = self.call_object_method_callable(
            compiled,
            object,
            method,
            args.into_iter().map(CallArgument::positional).collect(),
            None,
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

    pub(super) fn call_magic_instance_method(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        magic_method: &str,
        called_method: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Option<VmResult>, VmResult> {
        let Some(_class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
            return Ok(None);
        };
        let resolved = match lookup_resolved_method_in_state(
            compiled,
            state,
            &object.class_name(),
            magic_method,
            None,
        ) {
            Ok(Some(method)) => method,
            Ok(None) => return Ok(None),
            Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
        };
        if resolved.method.flags.is_static
            || resolved.method.flags.is_private
            || resolved.method.flags.is_protected
        {
            return Ok(None);
        }
        let guard = MagicMethodCall {
            receiver: format!("object:{}", object.id()),
            magic_method: normalize_method_name(magic_method),
            called_method: normalize_method_name(called_method),
        };
        if state
            .magic_method_stack
            .iter()
            .any(|active| active == &guard)
        {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_MAGIC_METHOD_RECURSION: recursive {magic_method} for {}::{called_method}",
                    object.class_name()
                ),
            ));
        }
        let magic_args = vec![
            CallArgument::positional(Value::String(PhpString::from_test_str(called_method))),
            CallArgument::positional(magic_args_array(args)),
        ];
        state.magic_method_stack.push(guard);
        let class_owner = class_owner_in_state(compiled, state, &resolved.class.name);
        let result = self.execute_function(
            &class_owner,
            resolved.method.function,
            FunctionCall::new(magic_args, Vec::new())
                .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                .with_optional_call_span(call_span)
                .with_this(object.clone())
                .with_class_context_handles(
                    self.class_name_handles(&resolved.class.name).normalized,
                    object_called_class_handle(&object),
                    self.class_name_handles(&resolved.class.name).normalized,
                ),
            output,
            stack,
            state,
        );
        let _ = state.magic_method_stack.pop();
        Ok(Some(result))
    }

    pub(super) fn call_magic_static_method(
        &self,
        compiled: &CompiledUnit,
        class: &php_ir::module::ClassEntry,
        magic_method: &str,
        called_method: &str,
        args: Vec<CallArgument>,
        called_class: String,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Option<VmResult>, VmResult> {
        let resolved =
            match lookup_resolved_method_in_state(compiled, state, &class.name, magic_method, None)
            {
                Ok(Some(method)) => method,
                Ok(None) => return Ok(None),
                Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
            };
        if !resolved.method.flags.is_static
            || resolved.method.flags.is_private
            || resolved.method.flags.is_protected
        {
            return Ok(None);
        }
        let guard = MagicMethodCall {
            receiver: format!("class:{}", normalize_class_name(&class.name)),
            magic_method: normalize_method_name(magic_method),
            called_method: normalize_method_name(called_method),
        };
        if state
            .magic_method_stack
            .iter()
            .any(|active| active == &guard)
        {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_MAGIC_METHOD_RECURSION: recursive {magic_method} for {}::{called_method}",
                    class.name
                ),
            ));
        }
        let magic_args = vec![
            CallArgument::positional(Value::String(PhpString::from_test_str(called_method))),
            CallArgument::positional(magic_args_array(args)),
        ];
        state.magic_method_stack.push(guard);
        let class_owner = class_owner_in_state(compiled, state, &resolved.class.name);
        let result = self.execute_function(
            &class_owner,
            resolved.method.function,
            FunctionCall::new(magic_args, Vec::new())
                .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                .with_optional_call_span(call_span)
                .with_class_context_handles(
                    self.class_name_handles(&resolved.class.name).normalized,
                    self.class_name_handles(&called_class).display,
                    self.class_name_handles(&resolved.class.name).normalized,
                ),
            output,
            stack,
            state,
        );
        let _ = state.magic_method_stack.pop();
        Ok(Some(result))
    }
}
