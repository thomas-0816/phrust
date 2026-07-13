use super::dense_activation::DenseActivationResult;
use super::prelude::*;

pub(super) struct DenseDirectMethodActivation {
    pub(super) function: FunctionId,
    pub(super) receiver: ObjectRef,
    pub(super) class_context: CompactClassContext,
}

pub(super) struct DenseDirectStaticActivation {
    pub(super) function: FunctionId,
    pub(super) receiver: Option<ObjectRef>,
    pub(super) class_context: CompactClassContext,
}

pub(super) struct DenseDirectConstructorActivation {
    pub(super) function: FunctionId,
    pub(super) object: ObjectRef,
    pub(super) class: Rc<php_ir::module::ClassEntry>,
    pub(super) class_context: CompactClassContext,
}

#[allow(clippy::too_many_arguments)]
fn direct_dense_call<'a>(
    args: &'a [DenseCallArg],
    dst: u32,
    compiled: &CompiledUnit,
    dense: &DenseBytecodeUnit,
    move_plan: Option<&crate::last_use::LastUseMovePlan>,
    instruction: &DenseInstruction,
    dense_instruction_index: u32,
    frame_index: usize,
    receiver: Option<ObjectRef>,
    class_context: CompactClassContext,
) -> DirectCall<'a> {
    let move_source = args.iter().find_map(|arg| {
        (arg.value.kind == DenseOperandKind::Register
            && move_plan.is_some_and(|plan| {
                plan.is_move_eligible(dense_instruction_index, arg.value.index)
            }))
        .then(|| RegId::new(arg.value.index))
    });
    DirectCall {
        caller_frame: frame_index,
        argument_sources: DirectArgumentSources::Dense(args),
        destination: CallDestination::Register(RegId::new(dst)),
        strict_types: compiled.unit().strict_types,
        span: dense.spans.get(instruction.span.index()).copied(),
        receiver,
        class_context,
        move_source,
    }
}

impl Vm {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_dense_new_object_operands(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
        class_name: &str,
        display_class_name: &str,
        args: &[DenseCallArg],
        call_span: Option<IrSpan>,
        dst: RegId,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let values = self
            .read_dense_call_args(dense, compiled, stack, args)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let result = self.execute_dense_new_object(
            compiled,
            plan,
            function_id,
            block_id,
            instruction_id,
            cache_id,
            class_name,
            display_class_name,
            values,
            call_span,
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            return Err(result);
        }
        stack
            .current_mut()
            .expect("bytecode caller frame is active")
            .registers
            .set(dst, result.return_value.unwrap_or(Value::Null))
            .map_err(|message| self.runtime_error(output, compiled, stack, message))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_dense_method_operands(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
        object: Value,
        method: &str,
        args: &[DenseCallArg],
        call_span: Option<IrSpan>,
        dst: RegId,
        frame_index: usize,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let values = self
            .read_dense_call_args(dense, compiled, stack, args)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let result = self.execute_dense_method_call(
            compiled,
            plan,
            function_id,
            block_id,
            instruction_id,
            cache_id,
            object,
            method,
            values,
            call_span,
            output,
            stack,
            state,
        );
        let return_value =
            self.dense_method_result_value(result, "method", compiled, output, stack)?;
        stack
            .frame_mut(frame_index)
            .expect("bytecode frame is active")
            .registers
            .set(dst, return_value)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_dense_static_operands(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
        class_name: &str,
        method: &str,
        args: &[DenseCallArg],
        call_span: Option<IrSpan>,
        dst: RegId,
        frame_index: usize,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let values = self
            .read_dense_call_args(dense, compiled, stack, args)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let result = self.execute_dense_static_method_call(
            compiled,
            plan,
            function_id,
            block_id,
            instruction_id,
            cache_id,
            class_name,
            method,
            values,
            call_span,
            output,
            stack,
            state,
        );
        let return_value =
            self.dense_method_result_value(result, "static", compiled, output, stack)?;
        stack
            .frame_mut(frame_index)
            .expect("bytecode frame is active")
            .registers
            .set(dst, return_value)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        unset_consumed_dense_call_arg_registers_at_frame(stack, frame_index, args, Some(dst))
            .map_err(|message| self.runtime_error(output, compiled, stack, message))
    }

    fn dense_method_result_value(
        &self,
        result: VmResult,
        kind: &'static str,
        compiled: &CompiledUnit,
        output: &OutputBuffer,
        stack: &CallStack,
    ) -> Result<Value, VmResult> {
        if !result.status.is_success() {
            self.record_counter_dense_call_fallback("dispatch_error");
            return Err(result);
        }
        if result.fiber_suspension.is_some() {
            self.record_counter_dense_call_fallback("fiber_suspension");
            return Err(VmResult::unsupported(
                output.clone(),
                format!(
                    "E_PHP_VM_DENSE_BYTECODE_CALL_FIBER_UNSUPPORTED: dense bytecode {kind} calls do not support fiber suspension yet"
                ),
            ));
        }
        let _ = (compiled, stack);
        Ok(result.return_value.unwrap_or(Value::Null))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dense_direct_method_transfer(
        &self,
        target: DenseDirectMethodActivation,
        args: &[DenseCallArg],
        dst: u32,
        compiled: &CompiledUnit,
        dense: &DenseBytecodeUnit,
        move_plan: Option<&crate::last_use::LastUseMovePlan>,
        instruction: &DenseInstruction,
        function_id: FunctionId,
        block_index: u32,
        next_instruction_offset: usize,
        dense_instruction_index: u32,
        frame_index: usize,
        foreach_iterators: HashMap<RegId, ForeachIterator>,
        diagnostics: Vec<RuntimeDiagnostic>,
        steps: usize,
    ) -> DenseActivationResult {
        self.record_counter_dense_method_call_hit();
        Self::dense_call_activation_signal(
            target.function,
            direct_dense_call(
                args,
                dst,
                compiled,
                dense,
                move_plan,
                instruction,
                dense_instruction_index,
                frame_index,
                Some(target.receiver),
                target.class_context,
            ),
            function_id,
            block_index,
            next_instruction_offset,
            dense_instruction_index,
            frame_index,
            foreach_iterators,
            diagnostics,
            steps,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dense_direct_static_transfer(
        &self,
        target: DenseDirectStaticActivation,
        args: &[DenseCallArg],
        dst: u32,
        compiled: &CompiledUnit,
        dense: &DenseBytecodeUnit,
        move_plan: Option<&crate::last_use::LastUseMovePlan>,
        instruction: &DenseInstruction,
        function_id: FunctionId,
        block_index: u32,
        next_instruction_offset: usize,
        dense_instruction_index: u32,
        frame_index: usize,
        foreach_iterators: HashMap<RegId, ForeachIterator>,
        diagnostics: Vec<RuntimeDiagnostic>,
        steps: usize,
    ) -> DenseActivationResult {
        self.record_counter_dense_static_call_hit();
        Self::dense_call_activation_signal(
            target.function,
            direct_dense_call(
                args,
                dst,
                compiled,
                dense,
                move_plan,
                instruction,
                dense_instruction_index,
                frame_index,
                target.receiver,
                target.class_context,
            ),
            function_id,
            block_index,
            next_instruction_offset,
            dense_instruction_index,
            frame_index,
            foreach_iterators,
            diagnostics,
            steps,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dense_direct_constructor_transfer(
        &self,
        target: DenseDirectConstructorActivation,
        args: &[DenseCallArg],
        dst: u32,
        compiled: &CompiledUnit,
        dense: &DenseBytecodeUnit,
        move_plan: Option<&crate::last_use::LastUseMovePlan>,
        instruction: &DenseInstruction,
        function_id: FunctionId,
        block_index: u32,
        next_instruction_offset: usize,
        dense_instruction_index: u32,
        frame_index: usize,
        foreach_iterators: HashMap<RegId, ForeachIterator>,
        diagnostics: Vec<RuntimeDiagnostic>,
        steps: usize,
    ) -> DenseActivationResult {
        Self::dense_constructor_activation_signal(
            target.function,
            direct_dense_call(
                args,
                dst,
                compiled,
                dense,
                move_plan,
                instruction,
                dense_instruction_index,
                frame_index,
                Some(target.object.clone()),
                target.class_context,
            ),
            target.object,
            target.class,
            function_id,
            block_index,
            next_instruction_offset,
            dense_instruction_index,
            frame_index,
            foreach_iterators,
            diagnostics,
            steps,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_dense_direct_constructor_activation(
        &self,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        class_name: &str,
        display_class_name: &str,
        args: &[DenseCallArg],
        stack: &CallStack,
        state: &mut ExecutionState,
    ) -> Option<DenseDirectConstructorActivation> {
        let plan = plan?;
        if args.len() > 8 || args.iter().any(|arg| arg.name.is_some()) {
            return None;
        }
        let Some(class) = self.cached_class_entry(compiled, state, class_name) else {
            self.record_counter_dense_call_fallback("direct_constructor_class");
            return None;
        };
        if spl_runtime_parent_for_class(compiled, state, &class).is_some() {
            self.record_counter_dense_call_fallback("direct_constructor_runtime_parent");
            return None;
        }
        let caller_scope = current_scope_class(compiled, stack);
        let Some(constructor) = lookup_resolved_method_in_state(
            compiled,
            state,
            &class.name,
            "__construct",
            caller_scope.as_deref(),
        )
        .ok()
        .flatten() else {
            self.record_counter_dense_call_fallback("direct_constructor_resolution");
            return None;
        };
        if validate_constructor_callable_in_state_scope(
            compiled,
            state,
            caller_scope.as_deref(),
            &constructor.class,
            &constructor.method,
        )
        .is_err()
        {
            self.record_counter_dense_call_fallback("direct_constructor_visibility");
            return None;
        }
        let owner = class_owner_in_state(compiled, state, &constructor.class.name);
        if owner.cache_identity() != compiled.cache_identity()
            || !matches!(
                plan.function_plan(constructor.method.function.index()),
                Some(DenseFunctionPlan::Dense)
            )
        {
            self.record_counter_dense_call_fallback("direct_constructor_owner_or_plan");
            return None;
        }
        let Some(callee) = compiled
            .unit()
            .functions
            .get(constructor.method.function.index())
        else {
            self.record_counter_dense_call_fallback("direct_constructor_callee");
            return None;
        };
        let Some(meta) = plan
            .call_shape_meta
            .get(constructor.method.function.index())
            .copied()
        else {
            self.record_counter_dense_call_fallback("direct_constructor_meta");
            return None;
        };
        if args.len() != callee.params.len()
            || !meta.params_bind_direct
            || !meta.elide_frame_args
            || !callee.attributes.is_empty()
        {
            self.record_counter_dense_call_fallback("direct_constructor_callee_shape");
            return None;
        }
        let Ok(runtime_class) = self.cached_runtime_class_entry(&owner, state, &class) else {
            self.record_counter_dense_call_fallback("direct_constructor_runtime_class");
            return None;
        };
        if validate_object_mvp(&runtime_class).is_err() {
            self.record_counter_dense_call_fallback("direct_constructor_object_shape");
            return None;
        }
        let slot_template =
            self.cached_default_slot_template(&owner, state, &runtime_class, display_class_name);
        let object = ObjectRef::from_layout_slots(
            &runtime_class,
            display_class_name,
            (*slot_template).clone(),
        );
        let declaring = self.class_name_handles(&constructor.class.name).normalized;
        Some(DenseDirectConstructorActivation {
            function: constructor.method.function,
            object: object.clone(),
            class,
            class_context: CompactClassContext {
                scope: Some(declaring.clone()),
                called: Some(object_called_class_handle(&object)),
                declaring: Some(declaring),
            },
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_dense_direct_static_activation(
        &self,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
        class_name: &str,
        method: &str,
        args: &[DenseCallArg],
        stack: &CallStack,
        state: &ExecutionState,
    ) -> Option<DenseDirectStaticActivation> {
        let plan = plan?;
        if args.len() > 8 || args.iter().any(|arg| arg.name.is_some()) {
            return None;
        }
        let class = resolve_static_class_name(compiled, state, stack, class_name).ok()?;
        let scope = method_lookup_scope_for_static_call(compiled, stack, class_name);
        let lowered_method = normalize_method_name(method);
        let epoch = state.lookup_epoch();
        let called_class = called_class_for_static_call(compiled, stack, class_name, &class);
        let cached = if let Some(id) = cache_id {
            self.dense_method_call_inline_cache_has_target(
                id,
                &lowered_method,
                &class.name,
                scope.as_deref(),
                epoch,
            )
        } else {
            self.method_call_inline_cache_has_target(
                IrInlineCacheSite::classic(compiled, function_id, block_id, instruction_id),
                &lowered_method,
                &class.name,
                scope.as_deref(),
                epoch,
            )
        };
        if !cached {
            return None;
        }
        let (target, _) = if let Some(id) = cache_id {
            self.lookup_dense_method_call_inline_cache(
                DenseInlineCacheSite::new(id, function_id, instruction_id),
                &lowered_method,
                &class.name,
                scope.as_deref(),
                epoch,
            )
        } else {
            self.lookup_method_call_inline_cache(
                IrInlineCacheSite::classic(compiled, function_id, block_id, instruction_id),
                &lowered_method,
                &class.name,
                scope.as_deref(),
                epoch,
            )
        };
        let target = target?;
        let resolved = target.resolved_target();
        if resolved.guard.argument_shape
            != (MethodCallShape {
                arity: args.len().try_into().unwrap_or(u32::MAX),
                named_arguments: Vec::new(),
                by_ref_arguments: CallReferenceMask::from_flags(
                    args.iter().map(dense_call_arg_has_reference_metadata),
                ),
            })
            || !resolved.guard.by_ref_compatible
            || !resolved.guard.method_is_static
        {
            return None;
        }
        let route = resolved.route.as_ref()?;
        if route.owner.cache_identity() != compiled.cache_identity()
            || !matches!(
                plan.function_plan(resolved.function.index()),
                Some(DenseFunctionPlan::Dense)
            )
        {
            return None;
        }
        let callee = compiled.unit().functions.get(resolved.function.index())?;
        let meta = plan
            .call_shape_meta
            .get(resolved.function.index())
            .copied()?;
        if args.len() != callee.params.len()
            || !meta.params_bind_direct
            || !meta.elide_frame_args
            || !callee.attributes.is_empty()
        {
            return None;
        }
        let declaring = route.declaring_class_handle.clone();
        Some(DenseDirectStaticActivation {
            function: resolved.function,
            receiver: None,
            class_context: CompactClassContext {
                scope: Some(declaring.clone()),
                called: Some(self.class_name_handles(&called_class).display),
                declaring: Some(declaring),
            },
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_dense_direct_method_activation(
        &self,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
        object: &ObjectRef,
        method: &str,
        args: &[DenseCallArg],
        stack: &CallStack,
        state: &ExecutionState,
    ) -> Option<DenseDirectMethodActivation> {
        let plan = plan?;
        if args.len() > 8 || args.iter().any(|arg| arg.name.is_some()) {
            return None;
        }
        let receiver_class = object.class_name();
        let class = lookup_class_in_state(compiled, state, &receiver_class)?.clone();
        let scope = current_scope_class(compiled, stack);
        let lowered_method = normalize_method_name(method);
        let epoch = state.lookup_epoch();
        let has_magic_call =
            class_has_public_magic_call_in_state(compiled, state, &receiver_class).ok()?;
        let cached = if let Some(id) = cache_id {
            self.dense_method_call_inline_cache_has_target(
                id,
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
            )
        } else {
            self.method_call_inline_cache_has_target(
                IrInlineCacheSite::classic(compiled, function_id, block_id, instruction_id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
            )
        };
        if !cached {
            return None;
        }
        let (target, _) = if let Some(id) = cache_id {
            self.lookup_dense_method_call_inline_cache(
                DenseInlineCacheSite::new(id, function_id, instruction_id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
            )
        } else {
            self.lookup_method_call_inline_cache(
                IrInlineCacheSite::classic(compiled, function_id, block_id, instruction_id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
            )
        };
        let target = target?;
        let argument_shape = MethodCallShape {
            arity: args.len().try_into().unwrap_or(u32::MAX),
            named_arguments: Vec::new(),
            by_ref_arguments: CallReferenceMask::from_flags(
                args.iter().map(dense_call_arg_has_reference_metadata),
            ),
        };
        if !dense_method_direct_call_target_is_eligible(DenseMethodDirectCallEligibility {
            compiled,
            state,
            target: &target,
            class: &class,
            argument_shape,
            has_magic_call,
            epoch,
        }) {
            return None;
        }
        let resolved = target.resolved_target();
        let route = resolved.route.as_ref()?;
        if route.owner.cache_identity() != compiled.cache_identity()
            || !matches!(
                plan.function_plan(resolved.function.index()),
                Some(DenseFunctionPlan::Dense)
            )
        {
            return None;
        }
        let callee = compiled.unit().functions.get(resolved.function.index())?;
        let meta = plan
            .call_shape_meta
            .get(resolved.function.index())
            .copied()?;
        if args.len() != callee.params.len()
            || !meta.params_bind_direct
            || !meta.elide_frame_args
            || !callee.attributes.is_empty()
        {
            return None;
        }
        let declaring = route.declaring_class_handle.clone();
        Some(DenseDirectMethodActivation {
            function: resolved.function,
            receiver: object.clone(),
            class_context: CompactClassContext {
                scope: Some(declaring.clone()),
                called: Some(object_called_class_handle(object)),
                declaring: Some(declaring),
            },
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_dense_method_call(
        &self,
        compiled: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
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
                let value = match self.call_generator_method(
                    ExecutionCursor::new(compiled, output, stack, state),
                    generator,
                    method,
                    args,
                ) {
                    Ok(value) => value,
                    Err(result) => return *result,
                };
                return VmResult::success_no_output(Some(value));
            }
            Value::Fiber(fiber) => {
                self.record_counter_dense_call_fallback("fiber_method_receiver");
                let value = match self.call_fiber_method(
                    ExecutionCursor::new(compiled, output, stack, state),
                    fiber,
                    method,
                    args,
                ) {
                    Ok(value) => value,
                    Err(result) => {
                        if let Some(throwable) = state
                            .pending_throw
                            .take()
                            .or_else(|| runtime_error_throwable(&result))
                        {
                            state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                            state.pending_throw = Some(throwable);
                            return VmResult::propagating_exception(output.clone());
                        }
                        return *result;
                    }
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
        if is_hash_context_runtime_class(&receiver_class)
            && normalize_method_name(method) == "__debuginfo"
        {
            if !args.is_empty() {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_TOO_MANY_ARGS: HashContext::__debugInfo() expects exactly 0 arguments, {} given",
                        args.len()
                    ),
                );
            }
            let Some(properties) = hash_context_debug_info_array(&object) else {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_INVALID_HASH_CONTEXT: invalid HashContext state".to_owned(),
                );
            };
            return VmResult::success_no_output(Some(Value::Array(properties)));
        }
        if is_mysqli_runtime_class(&receiver_class) {
            // Same inline dispatch as the rich CallMethod arm; the generic
            // object-method callable helper does not cover mysqli.
            self.record_counter_dense_call_fallback("runtime_method_receiver");
            let value = match call_mysqli_method(
                &object,
                method,
                args,
                &mut state.builtins.mysql,
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
            || is_spl_heap_runtime_class(&receiver_class)
            || is_spl_file_runtime_class(&receiver_class)
            || spl_runtime_marker(&object).is_some_and(|class| {
                is_spl_iterator_runtime_class(&class)
                    || is_spl_container_runtime_class(&class)
                    || is_spl_heap_runtime_class(&class)
                    || is_spl_file_runtime_class(&class)
            })
            || is_php_token_runtime_class(&receiver_class)
            || is_date_time_runtime_class(&receiver_class)
            || internal_throwable_instanceof(&receiver_class, "throwable").is_some()
            || is_sqlite_runtime_class(&receiver_class)
            || is_pdo_runtime_class(&receiver_class)
            || is_redis_runtime_class(&receiver_class)
            || is_memcached_runtime_class(&receiver_class)
            || is_soap_runtime_class(&receiver_class)
            || is_fileinfo_runtime_class(&receiver_class)
            || is_imagick_runtime_class(&receiver_class)
            || is_xsl_runtime_class(&receiver_class)
            || is_phar_runtime_class(&receiver_class)
            || is_zip_runtime_class(&receiver_class)
            || is_xml_runtime_class(&receiver_class)
        {
            self.record_counter_dense_call_fallback("runtime_method_receiver");
            return self.call_object_method_callable(
                ExecutionCursor::new(compiled, output, stack, state),
                object,
                method,
                args,
                call_span,
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

        let (cached_target, observation) = if let Some(id) = cache_id {
            self.lookup_dense_method_call_inline_cache(
                DenseInlineCacheSite::new(id, function_id, instruction_id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
            )
        } else {
            self.observe_dense_call_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                InlineCacheKind::MethodCall,
            );
            self.lookup_method_call_inline_cache(
                IrInlineCacheSite::classic(compiled, function_id, block_id, instruction_id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
            )
        };
        let had_cached_target = cached_target.is_some();
        if let Some(target) = cached_target {
            self.record_counter_dense_call_ic_hit();
            if !matches!(
                self.options.native_optimization,
                NativeOptimizationPolicy::Optimizing
            ) || dense_method_direct_call_target_is_eligible(DenseMethodDirectCallEligibility {
                compiled,
                state,
                target: &target,
                class: &class,
                argument_shape: method_call_shape(&args),
                has_magic_call,
                epoch,
            }) {
                self.record_counter_dense_method_call_hit();
                if matches!(
                    self.options.native_optimization,
                    NativeOptimizationPolicy::Optimizing
                ) {
                    self.record_counter_direct_call_hit();
                }
                return self.execute_method_call_target(
                    compiled, target, object, args, call_span, output, stack, state, &None, plan,
                );
            }
            self.record_counter_direct_call_fallback();
        }
        if observation.is_some() {
            self.record_counter_dense_call_ic_miss();
        }
        if matches!(
            self.options.native_optimization,
            NativeOptimizationPolicy::Optimizing
        ) && !had_cached_target
        {
            self.record_counter_direct_call_fallback();
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
                    ExecutionCursor::new(compiled, output, stack, state),
                    object.clone(),
                    "__call",
                    method,
                    args,
                    call_span,
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
                    Err(result) => *result,
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
                ExecutionCursor::new(compiled, output, stack, state),
                object.clone(),
                "__call",
                method,
                args,
                call_span,
            ) {
                Ok(Some(result)) => result,
                Ok(None) => self.runtime_error_at_optional_span(
                    compiled, output, stack, state, call_span, message,
                ),
                Err(result) => *result,
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

        let class_owner = class_owner_in_state(compiled, state, &declaring_class.name);
        let has_by_ref_argument = class_owner
            .unit()
            .functions
            .get(method_entry.function.index())
            .is_some_and(|callee| callee.params.iter().any(|param| param.by_ref));
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
        let route =
            self.method_dispatch_route(&class_owner, method_entry.function, declaring_class);
        let method_target = Rc::new(MethodCallResolvedTarget {
            declaring_class: declaring_class.name.clone(),
            function: method_entry.function,
            guard: method_guard,
            route,
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
        if let Some(id) = cache_id {
            self.install_dense_method_call_inline_cache(
                id,
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
                target,
            );
        } else {
            self.install_method_call_inline_cache(
                IrInlineCacheSite::classic(compiled, function_id, block_id, instruction_id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
                target,
            );
        }
        self.record_counter_dense_method_call_hit();
        self.execute_function_with_dense_plan(
            ExecutionCursor::new(compiled, output, stack, state),
            &class_owner,
            plan,
            method_entry.function,
            {
                let declaring = self.class_name_handles(&declaring_class.name).normalized;
                FunctionCall::new(args, Vec::new())
                    .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                    .with_optional_call_span(call_span)
                    .with_this(object.clone())
                    .with_class_context_handles(
                        declaring.clone(),
                        object_called_class_handle(&object),
                        declaring,
                    )
            },
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
        cache_id: Option<InlineCacheId>,
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
                ExecutionCursor::new(compiled, output, stack, state),
                StaticMethodCallableRequest {
                    class_name,
                    method,
                    args,
                    call_span,
                    allow_by_ref_value_warnings: false,
                    by_ref_warning_callable_name: None,
                },
            );
        }

        // Match the rich-IR static-call arm: an unknown class must attempt
        // registered autoloaders before resolution fails.
        if let Err(result) = self.autoload_static_class_if_missing(
            ExecutionCursor::new(compiled, output, stack, state),
            class_name,
            call_span.unwrap_or_default(),
            Some((
                compiled_unit_cache_key(compiled),
                function_id,
                block_id,
                instruction_id,
            )),
        ) {
            return *result;
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

        if class_extends_php_token(compiled, state, &class) && lowered_method == "tokenize" {
            self.record_counter_dense_call_fallback("php_token_subclass_static_method");
            let trace_values = args.iter().map(|arg| arg.value.clone()).collect::<Vec<_>>();
            let result = match self
                .php_token_static_method_value_for_class(compiled, state, &class, class_name, args)
            {
                Ok(result) => result,
                Err(error) => {
                    return self
                        .php_token_static_method_error_result(compiled, output, stack, error);
                }
            };
            let trace_context = call_span.map(|call_span| TokenizerStaticCallTraceContext {
                call: format!("{class_name}::{method}"),
                values: trace_values,
                call_span,
            });
            if let Err(result) = self.route_tokenizer_static_method_diagnostics(
                compiled,
                output,
                stack,
                state,
                result.diagnostics,
                trace_context.as_ref(),
            ) {
                return *result;
            }
            return VmResult::success_no_output(Some(result.value));
        }

        if class.flags.is_enum && matches!(lowered_method.as_str(), "cases" | "from" | "tryfrom") {
            self.record_counter_dense_call_fallback("enum_static_method");
            return match enum_static_method(compiled, state, &class, method, args, &|value| {
                self.constant_value(compiled.unit(), value)
            }) {
                Ok(value) => VmResult::success_no_output(Some(value)),
                Err(message) => self.runtime_error(output, compiled, stack, message),
            };
        }

        if lowered_method == "__construct" && is_supported_spl_runtime_class(&class.name) {
            self.record_counter_dense_call_fallback("spl_runtime_constructor");
            let Some(object) = current_this_object(compiled, stack) else {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_NON_STATIC_METHOD_CALL: Non-static method {}::__construct() cannot be called statically",
                        class.display_name
                    ),
                );
            };
            let init_args = if is_spl_iterator_runtime_class(&class.name) {
                match self.prepare_spl_iterator_constructor_args(
                    compiled,
                    &class.name,
                    args,
                    output,
                    stack,
                    state,
                ) {
                    Ok(args) => args,
                    Err(result) => return *result,
                }
            } else {
                args
            };
            if let Err(message) = initialize_spl_runtime_subclass_storage(
                &object,
                &class.name,
                init_args,
                &self.options.runtime_context,
                Some(&mut state.resources),
            ) {
                return self.runtime_error_at_optional_span(
                    compiled, output, stack, state, call_span, message,
                );
            }
            return VmResult::success_no_output(Some(Value::Null));
        }

        if is_supported_spl_runtime_class(&class.name) {
            let Some(object) = current_this_object(compiled, stack) else {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_NON_STATIC_METHOD_CALL: Non-static method {}::{}() cannot be called statically",
                        class.display_name, method
                    ),
                );
            };
            let spl_class = normalize_class_name(&class.name);
            if is_spl_iterator_runtime_class(&spl_class) && spl_iterator_method_is_supported(method)
            {
                self.record_counter_dense_call_fallback("spl_runtime_parent_method");
                if spl_class == "appenditerator"
                    && matches!(lowered_method.as_str(), "append" | "rewind" | "next")
                {
                    return match self.call_spl_append_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        args,
                        call_span,
                    ) {
                        Ok(value) => VmResult::success_no_output(Some(value)),
                        Err(result) => *result,
                    };
                }
                if spl_class == "norewinditerator"
                    && matches!(
                        lowered_method.as_str(),
                        "rewind" | "valid" | "current" | "key" | "next"
                    )
                {
                    return match self.call_spl_no_rewind_iterator_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        method,
                        args,
                    ) {
                        Ok(value) => VmResult::success_no_output(Some(value)),
                        Err(result) => *result,
                    };
                }
                if matches!(
                    spl_class.as_str(),
                    "recursiveiteratoriterator" | "recursivetreeiterator"
                ) && matches!(
                    lowered_method.as_str(),
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
                        ExecutionCursor::new(compiled, output, stack, state),
                        object,
                        method,
                        args,
                        call_span,
                    ) {
                        Ok(value) => VmResult::success_no_output(Some(value)),
                        Err(result) => *result,
                    };
                }
                return match call_spl_iterator_method(
                    object,
                    method,
                    args,
                    &self.options.runtime_context,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(message) => self.runtime_error_at_optional_span(
                        compiled, output, stack, state, call_span, message,
                    ),
                };
            }
            if is_spl_container_runtime_class(&spl_class)
                && spl_container_method_is_supported(method)
            {
                self.record_counter_dense_call_fallback("spl_runtime_parent_method");
                return match self.call_spl_container_method_with_magic(
                    ExecutionCursor::new(compiled, output, stack, state),
                    object,
                    method,
                    args,
                    call_span,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(result) => *result,
                };
            }
            if is_spl_heap_runtime_class(&spl_class) && spl_heap_method_is_supported(method) {
                self.record_counter_dense_call_fallback("spl_runtime_parent_method");
                return match self.call_spl_heap_method(
                    ExecutionCursor::new(compiled, output, stack, state),
                    object,
                    method,
                    args,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(SplHeapMethodError::Message(message)) => self
                        .runtime_error_at_optional_span(
                            compiled, output, stack, state, call_span, message,
                        ),
                    Err(SplHeapMethodError::Runtime(result)) => *result,
                };
            }
            if is_spl_file_runtime_class(&spl_class) && spl_file_method_is_supported(method) {
                self.record_counter_dense_call_fallback("spl_runtime_parent_method");
                return match call_spl_file_method_in_state(
                    compiled,
                    state,
                    &object,
                    method,
                    args,
                    &self.options.runtime_context,
                ) {
                    Ok(value) => VmResult::success_no_output(Some(value)),
                    Err(message) => self.runtime_error_at_optional_span(
                        compiled, output, stack, state, call_span, message,
                    ),
                };
            }
        }

        let (cached_target, observation) = if let Some(id) = cache_id {
            self.lookup_dense_method_call_inline_cache(
                DenseInlineCacheSite::new(id, function_id, instruction_id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
            )
        } else {
            self.observe_dense_call_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                InlineCacheKind::MethodCall,
            );
            self.lookup_method_call_inline_cache(
                IrInlineCacheSite::classic(compiled, function_id, block_id, instruction_id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
            )
        };
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
                if let Some(object) = current_this_object(compiled, stack) {
                    match self.call_magic_instance_method(
                        ExecutionCursor::new(compiled, output, stack, state),
                        object,
                        "__call",
                        method,
                        args.clone(),
                        call_span,
                    ) {
                        Ok(Some(result)) => return result,
                        Ok(None) => {}
                        Err(result) => return *result,
                    }
                }
                return match self.call_magic_static_method(
                    ExecutionCursor::new(compiled, output, stack, state),
                    MagicStaticCallRequest {
                        class: &class,
                        magic_method: "__callStatic",
                        called_method: method,
                        args,
                        called_class,
                        call_span,
                    },
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
                    Err(result) => *result,
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
                ExecutionCursor::new(compiled, output, stack, state),
                MagicStaticCallRequest {
                    class: &class,
                    magic_method: "__callStatic",
                    called_method: method,
                    args,
                    called_class,
                    call_span,
                },
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
                Err(result) => *result,
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
        let static_owner = class_owner_in_state(compiled, state, &declaring_class.name);
        let route =
            self.method_dispatch_route(&static_owner, method_entry.function, declaring_class);
        let method_target = Rc::new(MethodCallResolvedTarget {
            declaring_class: declaring_class.name.clone(),
            function: method_entry.function,
            guard: method_guard,
            route,
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
        if let Some(id) = cache_id {
            self.install_dense_method_call_inline_cache(
                id,
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
                target,
            );
        } else {
            self.install_method_call_inline_cache(
                IrInlineCacheSite::classic(compiled, function_id, block_id, instruction_id),
                &lowered_method,
                &receiver_class,
                scope.as_deref(),
                epoch,
                target,
            );
        }
        self.record_counter_dense_static_call_hit();
        let class_owner = class_owner_in_state(compiled, state, &declaring_class.name);
        self.execute_function_with_dense_plan(
            ExecutionCursor::new(compiled, output, stack, state),
            &class_owner,
            plan,
            method_entry.function,
            {
                let declaring = self.class_name_handles(&declaring_class.name).normalized;
                let mut call = FunctionCall::new(args, Vec::new())
                    .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                    .with_class_context_handles(
                        declaring.clone(),
                        self.class_name_handles(&called_class).display,
                        declaring,
                    )
                    .with_optional_call_span(call_span);
                if let Some(bound_this) = bound_this_for_scoped_call {
                    call = call.with_this(bound_this);
                }
                call
            },
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
            ExecutionCursor::new(compiled, output, stack, state),
            &owner,
            plan,
            method_entry.function,
            {
                let declaring = self.class_name_handles(&declaring_class.name).normalized;
                let mut call = FunctionCall::new(args, Vec::new())
                    .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                    .with_optional_call_span(call_span)
                    .with_class_context_handles(
                        declaring.clone(),
                        self.class_name_handles(&called_class).display,
                        declaring,
                    );
                if let Some(bound_this) = bound_this_for_scoped_call {
                    call = call.with_this(bound_this);
                }
                call
            },
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
        cache_id: Option<InlineCacheId>,
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
        if let Err(result) = self.autoload_static_class_if_missing_at_slot(
            compiled,
            display_class_name,
            call_span.unwrap_or_default(),
            Some((
                compiled_unit_cache_key(compiled),
                function_id,
                block_id,
                instruction_id,
            )),
            cache_id,
            output,
            stack,
            state,
        ) {
            return *result;
        }
        let Some(class) = self.cached_class_entry(compiled, state, class_name) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"),
            );
        };
        if let Err(result) =
            self.autoload_class_parents_if_missing(compiled, &class, output, stack, state)
        {
            return *result;
        }
        let class_owner = class_owner_in_state(compiled, state, &class.name);
        let runtime_class = match self.cached_runtime_class_entry(&class_owner, state, &class) {
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
        // Clone a memoized default-slot template (keyed by class + class-table
        // epoch) into the fresh instance, skipping the per-property iterate +
        // filter + `slot_by_name` hash-lookup loop the slow path runs. The
        // template is byte-identical to that loop's output and rebuilt on a
        // redefinition (epoch bump).
        let slot_template = self.cached_default_slot_template(
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
                ExecutionCursor::new(compiled, output, stack, state),
                &class_owner,
                plan,
                constructor.method.function,
                {
                    let declaring = self.class_name_handles(&constructor.class.name).normalized;
                    FunctionCall::new(args, Vec::new())
                        .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                        .with_optional_call_span(call_span)
                        .with_this(object.clone())
                        .with_class_context_handles(
                            declaring.clone(),
                            object_called_class_handle(&object),
                            declaring,
                        )
                },
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

fn dense_call_arg_has_reference_metadata(arg: &DenseCallArg) -> bool {
    arg.by_ref_local.is_some()
        || arg.by_ref_dim.is_some()
        || arg.by_ref_property.is_some()
        || arg.by_ref_property_dim.is_some()
}
