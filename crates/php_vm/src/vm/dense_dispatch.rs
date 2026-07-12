use super::dispatch_contract::DenseBinaryRequest;
use super::prelude::*;

impl Vm {
    pub(super) fn execute_bytecode_function(
        &self,
        request: DenseExecutionRequest<'_, '_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let DenseExecutionRequest {
            compiled,
            dense,
            plan,
            dense_function,
            ir_function,
            function_id,
            mut call,
        } = request;
        self.record_counter_dense_function_executed();
        if call.resume_continuation.is_some()
            || call.resume_fiber_continuation.is_some()
            || call.running_generator.is_some()
            || call.running_fiber.is_some()
        {
            return VmResult::unsupported(
                output.clone(),
                "E_PHP_VM_DENSE_BYTECODE_CALL_SHAPE_UNSUPPORTED: dense bytecode function calls do not support generator or fiber continuations yet",
            );
        }
        let mut diagnostics = Vec::new();
        // Function-invariant call-shape facts come from the execution plan
        // when it carries them (one Vec index) and fall back to the hashed
        // per-(unit, function) memo caches for plan-less calls.
        let call_shape_meta = plan
            .and_then(|plan| plan.call_shape_meta.get(function_id.index()))
            .copied();
        let frame_shape = match call_shape_meta {
            Some(meta) => FrameShapeFlags {
                has_try_or_finally: meta.has_try_or_finally,
                may_hold_destructor_sensitive_value: meta.may_hold_destructor_sensitive_value,
                has_inline_blocker: meta.has_inline_blocker,
            },
            None => self.frame_shape_flags(compiled, function_id, ir_function),
        };
        let frame_reuse_call_shape_reason = frame_reuse_call_shape_blocked_reason(
            ir_function,
            &call,
            frame_shape,
            self.options.reuse_class_context_frames,
        );
        let frame_layout = call_frame_layout_class(ir_function, &call, frame_shape);
        let argument_policy = call.argument_binding_policy(compiled);
        let elide_frame_args = match call_shape_meta {
            Some(meta) => meta.elide_frame_args,
            None => self.frame_args_elidable(compiled, function_id, ir_function),
        };
        self.record_counter_direct_frame(frame_layout, ir_function, elide_frame_args);
        // R1 fast path: an exact-arity plain-positional by-value call binds its
        // arguments straight into the callee frame's locals, reusing the
        // incoming `Vec<CallArgument>` as the hand-off buffer and skipping the
        // per-call `Vec<PreparedArg>` allocation. Guarded to the identical shape
        // as `bind_arguments`'s fast path, so coercion, strict-types, TypeError
        // reporting, and the backtrace snapshot stay byte-identical (see the
        // shared `is_direct_bind_fast_shape`, `coerce_or_check_param_type`, and
        // `trace_value_for_bound_param`).
        // R1.2 fast lane: the dense call arm may hand bare positional values
        // (pre-validated shape, references already dereferenced at operand
        // read) — no `CallArgument` vector exists at all for those calls.
        let prebound_values = call.positional_values.take();
        // The dense fast lane may only pre-bind values for the exact shape the
        // classic predicate accepts; anything else must arrive as
        // `CallArgument`s so the general binder sees it.
        debug_assert!(
            prebound_values.is_none()
                || (elide_frame_args
                    && call.args.is_empty()
                    && prebound_values
                        .as_ref()
                        .is_some_and(|values| { values.len() == ir_function.params.len() })
                    && arguments::params_bind_direct(ir_function)),
            "pre-bound positional values outside the direct-bind fast shape"
        );
        let direct_bind = prebound_values.is_some()
            || (elide_frame_args && arguments::is_direct_bind_fast_shape(ir_function, &call.args));
        let mut direct_values = CallValuesSmall::new();
        let mut prepared_args: Option<Vec<PreparedArg>> = None;
        let mut frame_args: Vec<Value> = Vec::new();
        let mut binding_diagnostics: Vec<RuntimeDiagnostic> = Vec::new();
        let has_by_ref_arg;
        if direct_bind {
            // Resolve reference arguments in place, exactly as the fast path in
            // `bind_arguments` does, so the trace snapshot and the locals both
            // observe the dereferenced value. The shape guarantees no by-ref
            // params, no defaults, no variadic, and no named arguments, so
            // `frame_args`/`binding_diagnostics` stay empty as they would on the
            // general path for this shape.
            direct_values = if let Some(prebound_values) = prebound_values {
                prebound_values
            } else {
                std::mem::take(&mut call.args)
                    .into_iter()
                    .map(|arg| match arg.value {
                        Value::Reference(cell) => cell.get(),
                        value => value,
                    })
                    .collect()
            };
            self.record_counter_prepared_arg_vector_allocation_avoided();
            has_by_ref_arg = false;
        } else {
            let prepared = match arguments::prepare_arguments(
                compiled,
                ir_function,
                std::mem::take(&mut call.args),
                stack,
                state,
                self.typecheck_fast_path_context(),
                argument_policy,
                call.allow_by_ref_value_warnings,
                call.call_span,
                call.by_ref_warning_callable_name.as_deref(),
                elide_frame_args,
            ) {
                Ok(args) => args,
                Err(message) => {
                    let error_compiled = call.error_context_compiled.as_ref().unwrap_or(compiled);
                    let error_span = call.call_span.unwrap_or(ir_function.span);
                    let caller_only_trace =
                        message.code() == "E_PHP_VM_BY_REF_ARG_NOT_REFERENCEABLE";
                    let result = self.runtime_error(output, error_compiled, stack, message);
                    if let Some(throwable) = runtime_error_throwable(&result) {
                        tag_throwable_location(&throwable, error_compiled, error_span);
                        state.pending_trace = Some(if caller_only_trace {
                            capture_backtrace_string(error_compiled, stack)
                        } else {
                            capture_backtrace_string_with_failed_call(
                                error_compiled,
                                stack,
                                ir_function,
                                error_span,
                            )
                        });
                        state.pending_throw = Some(throwable);
                        return VmResult::propagating_exception(output.clone());
                    }
                    return result;
                }
            };
            has_by_ref_arg = frame_reuse_prepared_args_blocked_reason(&prepared.args).is_some();
            frame_args = prepared.frame_args;
            binding_diagnostics = prepared.diagnostics;
            prepared_args = Some(prepared.args);
        }
        let frame_reuse_blocked_reason =
            frame_reuse_call_shape_reason.or_else(|| has_by_ref_arg.then_some("by_ref_argument"));
        self.record_counter_call_frame_layout(frame_layout);
        let specialized_frame_fallback = specialized_call_frame_fallback_reason(
            frame_layout,
            frame_reuse_blocked_reason,
            has_by_ref_arg,
        );
        let specialized_tiny_frame = specialized_frame_fallback.is_none();
        if frame_layout == "tiny_leaf_frame" {
            self.record_counter_tiny_frame_candidate();
        }
        if let Some(reason) = specialized_frame_fallback {
            self.record_counter_generic_frame_fallback(reason);
        }
        let activation_context = FrameActivationContext {
            scope_class: call.scope_class.take(),
            called_class: call.called_class.take(),
            declaring_class: call.declaring_class.take(),
            call_span: call.call_span,
        };
        let reused_frame = if let Some(reason) = frame_reuse_blocked_reason {
            self.record_counter_frame_reuse_blocked(reason);
            stack.push_fresh_frame(
                function_id,
                dense_function.register_count,
                dense_function.local_count,
                activation_context,
            );
            false
        } else {
            stack.push_reusable_frame(
                function_id,
                dense_function.register_count,
                dense_function.local_count,
                activation_context,
            )
        };
        let frame_index = stack.len().saturating_sub(1);
        self.record_counter_frame_activation(
            reused_frame,
            dense_function.register_count,
            dense_function.local_count,
        );
        for diagnostic in binding_diagnostics {
            let handled = match self.dispatch_error_handler(
                compiled,
                output,
                stack,
                state,
                PHP_E_WARNING,
                &diagnostic,
            ) {
                Ok(handled) => handled,
                Err(result) => {
                    stack.pop_recycle();
                    return *result;
                }
            };
            if !handled && error_reporting_allows(state, PHP_E_WARNING) {
                emit_vm_diagnostic(
                    output,
                    state,
                    &diagnostic,
                    PhpDiagnosticChannel::Warning,
                    PHP_E_WARNING,
                );
                diagnostics.push(diagnostic);
            }
        }
        {
            let frame = stack.current_mut().expect("bytecode frame was pushed");
            let args_is_empty = if direct_bind {
                direct_values.is_empty()
            } else {
                prepared_args.as_ref().is_none_or(Vec::is_empty)
            };
            if specialized_tiny_frame {
                self.record_counter_specialized_frame_hit();
                if !args_is_empty {
                    self.record_counter_arg_array_avoided();
                }
                if reused_frame {
                    self.record_counter_heap_frame_avoided();
                }
            } else {
                frame.arguments = frame_args;
                // Backtrace arguments reconstruct lazily from the live locals
                // (reference-engine semantics: traces show current slot
                // values), so no per-call snapshot is built here.
                frame.trace_arguments = TraceArguments::Lazy {
                    arg_count: if direct_bind {
                        direct_values.len() as u32
                    } else {
                        prepared_args.as_ref().map_or(0, Vec::len) as u32
                    },
                };
            }
        }
        if let Err(message) = initialize_captures(ir_function, call.captures, stack) {
            let result = self.runtime_error(output, compiled, stack, message);
            stack.pop_recycle();
            return result;
        }
        if let Some(this_value) = call.this_value
            && let Err(message) =
                initialize_this(compiled, state, function_id, ir_function, this_value, stack)
        {
            let result = self.runtime_error(output, compiled, stack, message);
            stack.pop_recycle();
            return result;
        }
        if let Some(shared) = call.shared_top_level_locals.as_deref_mut() {
            import_shared_locals(
                ir_function,
                stack,
                state,
                shared,
                call.shared_top_level_bind_missing_globals,
            );
        } else if ir_function.flags.is_top_level {
            bind_top_level_global_locals(ir_function, stack, state);
        }
        if direct_bind {
            // Direct-to-locals: coerce each raw by-value argument with the same
            // `coerce_or_check_param_type` at the same program point as the
            // general path, then write it straight into the frame's locals. The
            // fast shape has no by-ref params, so there is no reference cell to
            // bind (`coerce_or_check_param_type` ignores the by-ref flag anyway).
            let mut direct_values = direct_values;
            let bound_count = direct_values.len().min(ir_function.params.len());
            for arg_index in 0..bound_count {
                let param = &ir_function.params[arg_index];
                let mut value =
                    std::mem::replace(&mut direct_values[arg_index], Value::Uninitialized);
                if let Err(message) = coerce_or_check_param_type(
                    ParamTypecheckRequest {
                        compiled,
                        state,
                        function: ir_function,
                        param,
                        arg_index,
                        fast_path: self.typecheck_fast_path_context(),
                        strict_types: argument_policy.call_site_strict_types,
                        call_span: call.call_span,
                    },
                    &mut value,
                ) {
                    // Cold: the frame's arguments are elided on this fast
                    // path, so the lazy trace has no source for the failing
                    // (and any later) argument — materialize the snapshot from
                    // the already-bound locals plus the remaining raw args so
                    // the TypeError's own trace shows the real values.
                    direct_values[arg_index] = value;
                    let entries: Vec<FrameTraceArgument> = (0..direct_values.len())
                        .map(|index| {
                            let param = ir_function.params.get(index);
                            let value = match param {
                                Some(param) if index < arg_index => stack
                                    .current()
                                    .and_then(|frame| frame.locals.get(param.local))
                                    .unwrap_or(Value::Null),
                                _ => direct_values[index].clone(),
                            };
                            let sensitive = param.is_some_and(param_is_sensitive);
                            FrameTraceArgument {
                                name: None,
                                value: trace_value_for_param(&value, sensitive),
                            }
                        })
                        .collect();
                    if let Some(frame) = stack.current_mut() {
                        frame.trace_arguments = TraceArguments::Materialized(entries);
                    }
                    let result = self.runtime_error(output, compiled, stack, message);
                    if let Some(throwable) = runtime_error_throwable(&result) {
                        tag_throwable_location(&throwable, compiled, ir_function.span);
                        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                        return self.propagate_exception(output, stack, state, throwable);
                    }
                    stack.pop_recycle();
                    return result;
                }
                let locals = &mut stack
                    .current_mut()
                    .expect("bytecode frame was pushed")
                    .locals;
                if let Err(message) = locals.set(param.local, value) {
                    let result = self.runtime_error(output, compiled, stack, message);
                    stack.pop_recycle();
                    return result;
                }
            }
        } else {
            for (arg_index, (param, mut arg)) in ir_function
                .params
                .iter()
                .zip(prepared_args.unwrap_or_default())
                .enumerate()
            {
                if let Err(message) = coerce_or_check_param_type(
                    ParamTypecheckRequest {
                        compiled,
                        state,
                        function: ir_function,
                        param,
                        arg_index,
                        fast_path: self.typecheck_fast_path_context(),
                        strict_types: argument_policy.call_site_strict_types,
                        call_span: call.call_span,
                    },
                    &mut arg.value,
                ) {
                    let result = self.runtime_error(output, compiled, stack, message);
                    if let Some(throwable) = runtime_error_throwable(&result) {
                        tag_throwable_location(&throwable, compiled, ir_function.span);
                        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                        return self.propagate_exception(output, stack, state, throwable);
                    }
                    stack.pop_recycle();
                    return result;
                }
                let locals = &mut stack
                    .current_mut()
                    .expect("bytecode frame was pushed")
                    .locals;
                let result = if param.by_ref {
                    if let Some(reference) = arg.reference {
                        reference.set(arg.value);
                        locals.bind_reference_cell(param.local, reference)
                    } else {
                        locals.set(param.local, arg.value)
                    }
                } else {
                    locals.set(param.local, arg.value)
                };
                if let Err(message) = result {
                    let result = self.runtime_error(output, compiled, stack, message);
                    stack.pop_recycle();
                    return result;
                }
            }
        }
        let unit_id = compiled.unit().id;
        let dense_inline_cache_ids = if self.options.inline_caches.enabled() {
            let (ids, observations) = self
                .inline_caches
                .borrow_mut()
                .bind_dense_slots(compiled_unit_cache_key(compiled), &dense.cache_slots);
            for (descriptor, observation) in observations {
                self.record_inline_cache_site_event(
                    FunctionId::new(descriptor.function),
                    InstrId::new(descriptor.instruction),
                    observation,
                );
            }
            Some(ids)
        } else {
            None
        };
        // Runtime lever R3: `None` unless the flag is on, so the hot read path is
        // unchanged by default. Built once per (unit, function) and reused.
        let move_plan = self.last_use_move_plan(compiled, plan, function_id, dense_function);
        let move_plan = move_plan.as_deref();
        let mut foreach_iterators: HashMap<RegId, ForeachIterator> = HashMap::new();
        let mut block_index = 0_u32;
        let mut steps = 0_usize;
        'dispatch: loop {
            if let Some(code) = state.process_exit_code {
                stack.pop_frame_recycle(frame_index);
                return script_exit_result(output, state, code);
            }
            steps += 1;
            match execution_limit_exceeded(state, steps, self.options.max_steps) {
                Some(ExecutionLimitExceeded::Timeout) => {
                    let result = self.execution_timeout(output, compiled, stack);
                    stack.pop_recycle();
                    return result;
                }
                Some(ExecutionLimitExceeded::StepLimit) => {
                    let result =
                        self.runtime_error(output, compiled, stack, "VM step limit exceeded");
                    stack.pop_recycle();
                    return result;
                }
                None => {}
            }
            let Some(block) = dense_function.blocks.get(block_index as usize) else {
                let result = self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!("invalid dense bytecode block block:{block_index}"),
                );
                stack.pop_recycle();
                return result;
            };
            self.record_counter_dense_block_entry(function_id.raw(), block_index);
            let start = block.first_instruction as usize;
            let end = start + block.instruction_len as usize;
            let Some(instructions) = dense_function.instructions.get(start..end) else {
                let result = self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!("invalid dense bytecode instruction range for block:{block_index}"),
                );
                stack.pop_recycle();
                return result;
            };
            match try_execute_dense_pcre_ascii_offset_block_fast_path(
                compiled,
                dense,
                instructions,
                stack,
                state,
            ) {
                Ok(Some((next_block, truthy))) => {
                    self.record_counter_dense_branch(
                        function_id.raw(),
                        block_index,
                        next_block,
                        truthy,
                        false,
                    );
                    block_index = next_block;
                    continue 'dispatch;
                }
                Ok(None) => {}
                Err(message) => {
                    let result = self.runtime_error(output, compiled, stack, message);
                    stack.pop_recycle();
                    return result;
                }
            }
            let mut instruction_offset = 0_usize;
            while instruction_offset < instructions.len() {
                let instruction = &instructions[instruction_offset];
                let inline_cache_id = instruction.cache_slot.and_then(|slot| {
                    dense_inline_cache_ids
                        .as_deref()
                        .and_then(|ids| ids.get(slot.index()))
                        .copied()
                });
                let dense_instruction_index = start + instruction_offset;
                let dense_instruction_index = match u32::try_from(dense_instruction_index) {
                    Ok(index) => index,
                    Err(_) => {
                        let result = self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_DENSE_BYTECODE_SITE_INDEX_OVERFLOW: dense instruction index exceeds u32",
                        );
                        stack.pop_recycle();
                        return result;
                    }
                };
                let next_instruction_offset = if instruction.opcode.is_superinstruction()
                    && instruction_offset + 1 < instructions.len()
                {
                    instruction_offset + 2
                } else {
                    instruction_offset + 1
                };
                self.record_counter_bytecode_instruction(instruction.opcode);
                self.record_counter_superinstruction_executed(instruction.opcode);
                self.observe_dense_quickening(
                    unit_id,
                    function_id,
                    dense_instruction_index,
                    instruction.opcode,
                );
                match instruction.opcode {
                    DenseOpcode::Nop => {}
                    DenseOpcode::DeclareFunction => {
                        let DenseOperands::DeclareFunction { name, function } =
                            instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(name) = dense.names.get(name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode function declaration name n{name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let name = name.clone();
                        if let Err(message) = declare_runtime_function(
                            compiled,
                            state,
                            &name,
                            FunctionId::new(function),
                        ) {
                            let result = function_redeclaration_fatal_result(
                                output,
                                compiled,
                                stack,
                                dense_instruction_span(dense, instruction),
                                message,
                            );
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::DeclareClass => {
                        let DenseOperands::DeclareClass { name } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(name) = dense.names.get(name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode class declaration name n{name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let name = name.clone();
                        if let Err(message) = declare_runtime_class(compiled, state, &name) {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::FetchClassConstant => {
                        let DenseOperands::FetchClassConstant {
                            dst,
                            class_name,
                            constant,
                        } = &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dst = *dst;
                        let Some(class_name) = dense.names.get(*class_name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode class name n{class_name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(constant) = dense.names.get(*constant as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode class constant name n{constant}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let class_name = class_name.clone();
                        let constant = constant.clone();
                        let span = dense_instruction_span(dense, instruction);
                        let cache_site = (
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                        );
                        if inline_cache_id.is_none() {
                            self.observe_dense_property_inline_cache(
                                compiled,
                                cache_site.0,
                                cache_site.1,
                                cache_site.2,
                                InlineCacheKind::ClassConstantStaticProperty,
                            );
                        }
                        // Dense functions carry no local exception handlers
                        // (try/catch keeps a function on the rich plan), so a
                        // raised \Error / autoload throwable propagates to outer
                        // frames rather than routing in-frame.
                        let value = match self.fetch_class_constant_value(
                            ExecutionCursor::new(compiled, output, stack, state),
                            &class_name,
                            &constant,
                            Some(cache_site),
                            inline_cache_id,
                            span,
                        ) {
                            Ok(value) => value,
                            Err(ClassConstantFetch::Throwable(result)) => {
                                let result = *result;
                                if let Some(throwable) = state
                                    .pending_throw
                                    .take()
                                    .or_else(|| runtime_error_throwable(&result))
                                {
                                    // propagate_exception pops this frame.
                                    return self
                                        .propagate_exception(output, stack, state, throwable);
                                }
                                stack.pop_recycle();
                                return result;
                            }
                            Err(ClassConstantFetch::Raise(span, message)) => {
                                let mut exception_handlers = Vec::new();
                                let mut pending_control = None;
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_DENSE_FETCH_CLASS_CONSTANT_HANDLER: dense functions have no local exception handlers".to_string(),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                }
                            }
                            Err(ClassConstantFetch::Fatal(message)) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::FetchStaticProperty => {
                        let DenseOperands::FetchClassConstant {
                            dst,
                            class_name,
                            constant,
                        } = &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dst = *dst;
                        let Some(class_name) = dense.names.get(*class_name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode class name n{class_name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(property) = dense.names.get(*constant as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode static property name n{constant}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let class_name = class_name.clone();
                        let property = property.clone();
                        let span = dense_instruction_span(dense, instruction);
                        let cache_site = (
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                        );
                        if inline_cache_id.is_none() {
                            self.observe_dense_property_inline_cache(
                                compiled,
                                cache_site.0,
                                cache_site.1,
                                cache_site.2,
                                InlineCacheKind::ClassConstantStaticProperty,
                            );
                        }
                        let value = match self.fetch_static_property_value(
                            ExecutionCursor::new(compiled, output, stack, state),
                            &class_name,
                            &property,
                            Some(cache_site),
                            inline_cache_id,
                            span,
                        ) {
                            Ok(value) => value,
                            Err(ClassConstantFetch::Throwable(result)) => {
                                let result = *result;
                                if let Some(throwable) = state
                                    .pending_throw
                                    .take()
                                    .or_else(|| runtime_error_throwable(&result))
                                {
                                    // propagate_exception pops this frame.
                                    return self
                                        .propagate_exception(output, stack, state, throwable);
                                }
                                stack.pop_recycle();
                                return result;
                            }
                            Err(ClassConstantFetch::Raise(span, message)) => {
                                let mut exception_handlers = Vec::new();
                                let mut pending_control = None;
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_DENSE_FETCH_STATIC_PROPERTY_HANDLER: dense functions have no local exception handlers".to_string(),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                }
                            }
                            Err(ClassConstantFetch::Fatal(message)) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::IssetProperty | DenseOpcode::EmptyProperty => {
                        let DenseOperands::FetchProperty {
                            dst,
                            object,
                            property,
                        } = &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dst = *dst;
                        let object = *object;
                        let Some(property) = dense.names.get(*property as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode property name n{property}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let property = property.clone();
                        let object = match self.read_dense_operand(compiled, stack, object) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let probe = if instruction.opcode == DenseOpcode::IssetProperty {
                            self.isset_property_value(
                                compiled, &object, &property, output, stack, state,
                            )
                        } else {
                            self.empty_property_value(
                                compiled, &object, &property, output, stack, state,
                            )
                        };
                        let value = match probe {
                            Ok(value) => value,
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::CloneObject => {
                        let DenseOperands::RegOperand { dst, src } = &instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dst = *dst;
                        let src = *src;
                        let object = match self.read_dense_operand(compiled, stack, src) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let value = match self.clone_object_value(
                            compiled,
                            &object,
                            dense_instruction_span(dense, instruction),
                            output,
                            stack,
                            state,
                        ) {
                            Ok(value) => value,
                            Err(ClassConstantFetch::Throwable(result)) => {
                                let result = *result;
                                if let Some(throwable) = state
                                    .pending_throw
                                    .take()
                                    .or_else(|| runtime_error_throwable(&result))
                                {
                                    // propagate_exception pops this frame.
                                    return self
                                        .propagate_exception(output, stack, state, throwable);
                                }
                                stack.pop_recycle();
                                return result;
                            }
                            Err(ClassConstantFetch::Raise(span, message)) => {
                                let mut exception_handlers = Vec::new();
                                let mut pending_control = None;
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_DENSE_CLONE_HANDLER: dense functions have no local exception handlers".to_string(),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                }
                            }
                            Err(ClassConstantFetch::Fatal(message)) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::AssignStaticProperty => {
                        let DenseOperands::AssignStaticProperty {
                            dst,
                            class_name,
                            property,
                            value,
                        } = &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dst = *dst;
                        let value_operand = *value;
                        let (Some(class_name), Some(property)) = (
                            dense.names.get(*class_name as usize),
                            dense.names.get(*property as usize),
                        ) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                "invalid dense bytecode static property names".to_owned(),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let class_name = class_name.clone();
                        let property = property.clone();
                        let value = match self.read_dense_operand(compiled, stack, value_operand) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let span = dense_instruction_span(dense, instruction);
                        let (value, previous_effective) = match self.assign_static_property_value(
                            compiled,
                            &class_name,
                            &property,
                            value,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                BlockId::new(block_index),
                                InstrId::new(dense_instruction_index),
                            )),
                            span,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(outcome) => outcome,
                            Err(StaticPropertyAssignError::Vm(result)) => {
                                let result = *result;
                                if let Some(throwable) = state
                                    .pending_throw
                                    .take()
                                    .or_else(|| runtime_error_throwable(&result))
                                {
                                    // propagate_exception pops this frame.
                                    return self
                                        .propagate_exception(output, stack, state, throwable);
                                }
                                stack.pop_recycle();
                                return result;
                            }
                            Err(StaticPropertyAssignError::Raise(span, message)) => {
                                let mut exception_handlers = Vec::new();
                                let mut pending_control = None;
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_DENSE_ASSIGN_STATIC_PROPERTY_HANDLER: dense functions have no local exception handlers".to_string(),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                }
                            }
                            Err(StaticPropertyAssignError::Fatal(message)) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        {
                            let mut exception_handlers = Vec::new();
                            let mut pending_control = None;
                            if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                &previous_effective,
                            ) {
                                match outcome {
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_DENSE_ASSIGN_STATIC_PROPERTY_HANDLER: dense functions have no local exception handlers".to_string(),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                }
                            }
                        }
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::IssetStaticProperty | DenseOpcode::EmptyStaticProperty => {
                        let empty_probe =
                            matches!(instruction.opcode, DenseOpcode::EmptyStaticProperty);
                        let DenseOperands::FetchClassConstant {
                            dst,
                            class_name,
                            constant,
                        } = &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dst = *dst;
                        let (Some(class_name), Some(property)) = (
                            dense.names.get(*class_name as usize),
                            dense.names.get(*constant as usize),
                        ) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                "invalid dense bytecode static property names".to_owned(),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let class_name = class_name.clone();
                        let property = property.clone();
                        let span = dense_instruction_span(dense, instruction);
                        let probe = match static_property_isset_empty_result(
                            self,
                            ExecutionCursor::new(compiled, output, stack, state),
                            &class_name,
                            &property,
                            empty_probe,
                            span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                BlockId::new(block_index),
                                InstrId::new(dense_instruction_index),
                            )),
                        ) {
                            Ok(result) => result,
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                let mut exception_handlers = Vec::new();
                                let mut pending_control = None;
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_DENSE_STATIC_PROPERTY_PROBE_HANDLER: dense functions have no local exception handlers".to_string(),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                }
                            }
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                let result = *result;
                                if let Some(throwable) = state
                                    .pending_throw
                                    .take()
                                    .or_else(|| runtime_error_throwable(&result))
                                {
                                    // propagate_exception pops this frame.
                                    return self
                                        .propagate_exception(output, stack, state, throwable);
                                }
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), Value::Bool(probe))
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::AssignDynamicProperty => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Object,
                            "property_assign",
                        );
                        let DenseOperands::AssignDynamicProperty {
                            dst,
                            object,
                            property,
                            value,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let property_value = match self
                            .read_dense_operand(compiled, stack, property)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        // Mirrors `dynamic_property_name` on the rich path:
                        // string conversion with PHP-visible warnings/raises.
                        let property = match self.value_to_string(
                            compiled,
                            &property_value,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(name) => name.to_string_lossy(),
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        let object = match self.read_dense_operand(compiled, stack, object) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let value = match self.read_dense_operand(compiled, stack, value) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        let value = match self.dense_assign_property_value(
                            compiled,
                            output,
                            stack,
                            state,
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            inline_cache_id,
                            &property,
                            object,
                            value,
                            span,
                        ) {
                            Ok(value) => value,
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::UnsetProperty => {
                        let DenseOperands::UnsetProperty { object, property } =
                            &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let object_operand = *object;
                        let Some(property) = dense.names.get(*property as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode property name n{property}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let property = property.clone();
                        let object = match self.read_dense_operand(compiled, stack, object_operand)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let result = self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot unset property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                                stack.pop_recycle();
                                return result;
                            }
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let span = dense_instruction_span(dense, instruction);
                        match self.unset_property_value(
                            compiled, &object, &property, span, output, stack, state,
                        ) {
                            Ok(()) => {}
                            Err(StaticPropertyAssignError::Vm(result)) => {
                                let result = *result;
                                if let Some(throwable) = state
                                    .pending_throw
                                    .take()
                                    .or_else(|| runtime_error_throwable(&result))
                                {
                                    return self
                                        .propagate_exception(output, stack, state, throwable);
                                }
                                stack.pop_recycle();
                                return result;
                            }
                            Err(StaticPropertyAssignError::Raise(span, message)) => {
                                let mut exception_handlers = Vec::new();
                                let mut pending_control = None;
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_DENSE_UNSET_PROPERTY_HANDLER: dense functions have no local exception handlers".to_string(),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                }
                            }
                            Err(StaticPropertyAssignError::Fatal(message)) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        }
                    }
                    DenseOpcode::UnsetPropertyDim => {
                        let DenseOperands::UnsetPropertyDim {
                            object,
                            property,
                            dims,
                        } = &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let object_operand = *object;
                        let Some(property) = dense.names.get(*property as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode property name n{property}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let property = property.clone();
                        let object = match self.read_dense_operand(compiled, stack, object_operand)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let result = self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot unset property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                                stack.pop_recycle();
                                return result;
                            }
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let dims = match self.read_dense_dim_operands(compiled, stack, dims) {
                            Ok(dims) => dims,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Err(message) =
                            unset_property_dim(compiled, state, stack, &object, &property, &dims)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::AssignPropertyDim => {
                        let DenseOperands::AssignPropertyDim {
                            dst,
                            object,
                            property,
                            dims,
                            append,
                            value,
                        } = &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dst = *dst;
                        let append = *append;
                        let object_operand = *object;
                        let value_operand = *value;
                        let Some(property) = dense.names.get(*property as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode property name n{property}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let property = property.clone();
                        let dims = match self.read_dense_dim_operands(compiled, stack, dims) {
                            Ok(dims) => dims,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let object = match self.read_dense_operand(compiled, stack, object_operand)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let result = self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_DIM_ASSIGN_NON_OBJECT: cannot assign property dimension {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                                stack.pop_recycle();
                                return result;
                            }
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let assigned = match self.read_dense_operand(compiled, stack, value_operand)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let span = dense_instruction_span(dense, instruction);
                        let value = match self.assign_property_dim_value(
                            compiled,
                            object,
                            &property,
                            &dims,
                            append,
                            assigned,
                            span,
                            &mut diagnostics,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(value) => value,
                            Err(PropertyDimAssign::Raise(span, message)) => {
                                let mut exception_handlers = Vec::new();
                                let mut pending_control = None;
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_DENSE_PROPERTY_DIM_ASSIGN_HANDLER: dense functions have no local exception handlers".to_string(),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                }
                            }
                            Err(PropertyDimAssign::Fatal(message)) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                            Err(PropertyDimAssign::Return(result)) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::LoadConst | DenseOpcode::LoadConstLoadConst => {
                        // The fused form appends a second constant load into
                        // another register after the unchanged first load.
                        let (dst, constant, fused_const) = match instruction.operands {
                            DenseOperands::RegConst { dst, constant } => (dst, constant, None),
                            DenseOperands::LoadConstPair {
                                first_dst,
                                first_constant,
                                second_dst,
                                second_constant,
                            } => (
                                first_dst,
                                first_constant,
                                Some((second_dst, second_constant)),
                            ),
                            _ => {
                                let result = self.invalid_bytecode_operand_shape(
                                    output,
                                    compiled,
                                    stack,
                                    instruction,
                                );
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let value = match self
                            .cached_constant_value(compiled, ConstId::new(constant))
                        {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if let Some((second_dst, second_constant)) = fused_const {
                            let value = match self
                                .cached_constant_value(compiled, ConstId::new(second_constant))
                            {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            };
                            if let Err(message) = stack
                                .current_mut()
                                .expect("bytecode frame was pushed")
                                .registers
                                .set(RegId::new(second_dst), value)
                            {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        }
                    }
                    DenseOpcode::LoadConstEcho => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Output,
                            "echo",
                        );
                        let DenseOperands::RegConst { dst, constant } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let value = match self
                            .cached_constant_value(compiled, ConstId::new(constant))
                        {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        // Echo borrows the value, so echo first and move the
                        // value into the register afterwards instead of
                        // cloning it for the register write.
                        if let Err(result) = self.write_echo(compiled, output, stack, state, &value)
                        {
                            stack.pop_recycle();
                            return *result;
                        }
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::FetchConst => {
                        let DenseOperands::RegName { dst, name } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(name) = dense.names.get(name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense name index {name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let value = match name.as_str() {
                            "STDIN" => {
                                let resource = state
                                    .stdin
                                    .get_or_insert_with(|| {
                                        state.resources.register_stdin(Vec::new())
                                    })
                                    .clone();
                                Value::Resource(resource)
                            }
                            "STDOUT" => {
                                let resource = state
                                    .stdout
                                    .get_or_insert_with(|| state.resources.register_stdout())
                                    .clone();
                                Value::Resource(resource)
                            }
                            "STDERR" => {
                                let resource = state
                                    .stderr
                                    .get_or_insert_with(|| state.resources.register_stderr())
                                    .clone();
                                Value::Resource(resource)
                            }
                            _ => match lexical_constant_lookup(compiled, state, stack, name) {
                                Ok(Some(resolved)) => {
                                    if let Some(constant) = resolved.predefined
                                        && let Err(result) = self
                                            .emit_predefined_constant_deprecation(
                                                ExecutionCursor::new(
                                                    compiled, output, stack, state,
                                                ),
                                                &mut diagnostics,
                                                dense_instruction_span(dense, instruction),
                                                constant,
                                            )
                                    {
                                        stack.pop_recycle();
                                        return *result;
                                    }
                                    resolved.value
                                }
                                Ok(None) => {
                                    let result = self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        format!(
                                            "E_PHP_RUNTIME_UNDEFINED_CONSTANT: undefined constant {name}"
                                        ),
                                    );
                                    stack.pop_recycle();
                                    return result;
                                }
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            },
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::Move => {
                        let DenseOperands::RegOperand { dst, src } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let value = match self.read_dense_operand_last_use(
                            compiled,
                            stack,
                            src,
                            move_plan,
                            dense_instruction_index,
                        ) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::Cast => {
                        let DenseOperands::Cast { dst, kind, src } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let src = match self.read_dense_operand_last_use(
                            compiled,
                            stack,
                            src,
                            move_plan,
                            dense_instruction_index,
                        ) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let source_span = runtime_source_span(
                            compiled,
                            dense_instruction_span(dense, instruction),
                        );
                        let value = match self.execute_cast(
                            kind,
                            &src,
                            source_span,
                            ExecutionCursor::new(compiled, output, stack, state),
                        ) {
                            Ok(value) => value,
                            Err(result) => {
                                if let Some(throwable) = runtime_error_throwable(&result) {
                                    let span = dense_instruction_span(dense, instruction);
                                    tag_throwable_location(&throwable, compiled, span);
                                    let is_caching_iterator_string_cast = matches!(
                                        effective_value(&src),
                                        Value::Object(object)
                                            if spl_runtime_marker(&object)
                                                .is_some_and(|class| is_spl_caching_iterator_class(&class))
                                    );
                                    state.pending_trace = Some(
                                        if matches!(kind, CastKind::String)
                                            && is_caching_iterator_string_cast
                                        {
                                            capture_backtrace_string_with_builtin_failed_call(
                                                compiled,
                                                stack,
                                                "CachingIterator->__toString",
                                                &[],
                                                span,
                                            )
                                        } else {
                                            capture_backtrace_string(compiled, stack)
                                        },
                                    );
                                    state.pending_throw = Some(throwable);
                                    stack.pop_recycle();
                                    return VmResult::propagating_exception(output.clone());
                                }
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::LoadLocal | DenseOpcode::LoadLocalLoadConst => {
                        // The fused form appends a constant load into a second
                        // register after the unchanged local-load sequence.
                        let (dst, src, fused_const) = match instruction.operands {
                            DenseOperands::RegOperand { dst, src } => (dst, src, None),
                            DenseOperands::LoadLocalLoadConst {
                                first_dst,
                                local,
                                second_dst,
                                constant,
                            } => (first_dst, local, Some((second_dst, constant))),
                            _ => {
                                let result = self.invalid_bytecode_operand_shape(
                                    output,
                                    compiled,
                                    stack,
                                    instruction,
                                );
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if src.kind != DenseOperandKind::Local {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        }
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        // The lookahead scan only matters when the local is
                        // actually undefined (it decides whether to suppress
                        // the undefined-variable warning for a by-ref out
                        // parameter); defined locals — the overwhelmingly
                        // common case — skip it entirely.
                        let local_is_defined = stack
                            .current()
                            .and_then(|frame| frame.locals.get_slot(LocalId::new(src.index)))
                            .is_some_and(|slot| match slot {
                                Slot::Value(value) => !value.is_uninitialized(),
                                Slot::Reference(_) => true,
                            });
                        let suppress_undefined_warning = !local_is_defined
                            && dense_load_local_is_pre_call_by_ref_out_param(
                                compiled,
                                state,
                                dense,
                                instructions,
                                instruction_offset,
                                dst,
                                src.index,
                            );
                        let value = match self.load_local_value(
                            compiled,
                            ir_function,
                            output,
                            stack,
                            state,
                            &mut diagnostics,
                            LocalId::new(src.index),
                            span,
                            suppress_undefined_warning,
                        ) {
                            Ok(value) => value,
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if let Some((second_dst, constant)) = fused_const {
                            let value = match self
                                .cached_constant_value(compiled, ConstId::new(constant))
                            {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            };
                            if let Err(message) = stack
                                .current_mut()
                                .expect("bytecode frame was pushed")
                                .registers
                                .set(RegId::new(second_dst), value)
                            {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        }
                    }
                    DenseOpcode::LoadLocalEcho => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Output,
                            "echo",
                        );
                        let DenseOperands::RegOperand { dst, src } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        if src.kind != DenseOperandKind::Local {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        }
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        let value = match self.load_local_value(
                            compiled,
                            ir_function,
                            output,
                            stack,
                            state,
                            &mut diagnostics,
                            LocalId::new(src.index),
                            span,
                            false,
                        ) {
                            Ok(value) => value,
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        // Echo borrows the value, so echo first and move the
                        // value into the register afterwards instead of
                        // cloning it for the register write.
                        if let Err(result) = self.write_echo(compiled, output, stack, state, &value)
                        {
                            stack.pop_recycle();
                            return *result;
                        }
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::LoadLocalQuiet => {
                        let DenseOperands::RegOperand { dst, src } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        if src.kind != DenseOperandKind::Local {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        }
                        let local = LocalId::new(src.index);
                        let value = if is_globals_local(ir_function, local) {
                            self.record_counter_local_slot_fast_path(false);
                            Value::Array(state.globals.globals_array())
                        } else {
                            self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(
                                stack, local,
                            ));
                            match stack
                                .current()
                                .expect("bytecode frame was pushed")
                                .locals
                                .get(local)
                            {
                                Some(Value::Uninitialized) => Value::Null,
                                Some(value) => value.clone(),
                                None => {
                                    let result = self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        format!("invalid local local:{}", local.raw()),
                                    );
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::IssetLocal | DenseOpcode::EmptyLocal => {
                        let DenseOperands::RegOperand { dst, src } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        if src.kind != DenseOperandKind::Local {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        }
                        let local = LocalId::new(src.index);
                        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(
                            stack, local,
                        ));
                        let value = read_local_value(stack, local).unwrap_or(Value::Uninitialized);
                        let result = if instruction.opcode == DenseOpcode::IssetLocal {
                            !matches!(value, Value::Uninitialized | Value::Null)
                        } else {
                            match php_empty(&value) {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), Value::Bool(result))
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::StoreLocal | DenseOpcode::StoreLocalDiscard => {
                        let DenseOperands::LocalOperand { local, src } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        // The discard variant unsets the source register right
                        // after the store, so a register read here is a move by
                        // definition — take it instead of clone-then-unset. The
                        // plain store keeps the register live and only moves
                        // when the last-use plan proves this read is dead.
                        let value = if instruction.opcode == DenseOpcode::StoreLocalDiscard {
                            self.take_consumed_dense_operand(compiled, stack, src)
                        } else {
                            self.read_dense_operand_last_use(
                                compiled,
                                stack,
                                src,
                                move_plan,
                                dense_instruction_index,
                            )
                        };
                        let value = match value {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let previous = if instruction.opcode == DenseOpcode::StoreLocalDiscard {
                            stack
                                .current()
                                .expect("bytecode frame was pushed")
                                .locals
                                .get(LocalId::new(local))
                                .unwrap_or(Value::Uninitialized)
                        } else {
                            match stack
                                .current()
                                .expect("bytecode frame was pushed")
                                .locals
                                .get(LocalId::new(local))
                            {
                                Some(Value::Object(object)) => Value::Object(object),
                                _ => Value::Uninitialized,
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .locals
                            .set(LocalId::new(local), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if instruction.opcode == DenseOpcode::StoreLocalDiscard {
                            if let Err(message) = unset_dense_register_operand(stack, src) {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                            let mut exception_handlers = Vec::new();
                            let mut pending_control = None;
                            if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                &previous,
                            ) {
                                match outcome {
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_BYTECODE_DESTRUCTOR_CATCH_UNSUPPORTED: bytecode store/discard cannot route a caught destructor exception",
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                }
                            }
                        }
                        release_unrooted_object_handles(&previous);
                    }
                    DenseOpcode::UnsetLocal => {
                        let DenseOperands::Local { local } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let local = LocalId::new(local);
                        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(
                            stack, local,
                        ));
                        let previous = stack
                            .current()
                            .expect("bytecode frame was pushed")
                            .locals
                            .get(local)
                            .unwrap_or(Value::Uninitialized);
                        if ir_function.flags.is_top_level
                            && !is_globals_local(ir_function, local)
                            && let Some(Slot::Reference(cell)) = stack
                                .current()
                                .expect("bytecode frame was pushed")
                                .locals
                                .get_slot(local)
                                .cloned()
                            && let Some(name) = ir_function.locals.get(local.index())
                            && state
                                .globals
                                .get_slot(name)
                                .is_some_and(|global| global.ptr_eq(&cell))
                        {
                            cell.set(Value::Uninitialized);
                        }
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .locals
                            .unset(local)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        let mut exception_handlers = Vec::new();
                        let mut pending_control = None;
                        if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                            ExecutionCursor::new(compiled, output, stack, state),
                            &mut exception_handlers,
                            &mut pending_control,
                            &previous,
                        ) {
                            match outcome {
                                RaiseOutcome::Done(result) => {
                                    stack.pop_recycle();
                                    return *result;
                                }
                                RaiseOutcome::Caught(_) => {
                                    let result = self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        "E_PHP_VM_BYTECODE_DESTRUCTOR_CATCH_UNSUPPORTED: bytecode unset cannot route a caught destructor exception",
                                    );
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        }
                        release_unrooted_object_handles(&previous);
                    }
                    DenseOpcode::BindGlobal => {
                        let DenseOperands::LocalName { local, name } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(name) = dense.names.get(name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense name index {name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::GlobalOrSuperglobalReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::GlobalOrSuperglobalReference,
                        );
                        let cell = state.globals.ensure_slot(name.clone(), Value::Null);
                        if cell.get().is_uninitialized() {
                            cell.set(Value::Null);
                        }
                        self.record_counter_local_slot_fast_path(false);
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .locals
                            .bind_reference_cell(LocalId::new(local), cell)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::BindReferenceDim => {
                        let DenseOperands::BindReferenceDim {
                            local,
                            ref dims,
                            append,
                            source,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        let dims = match self.read_dense_dim_operands(compiled, stack, dims) {
                            Ok(dims) => dims,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let local = LocalId::new(local);
                        let source = LocalId::new(source);
                        self.record_counter_local_slot_fast_path(
                            local_slot_is_in_bounds(stack, local)
                                && local_slot_is_in_bounds(stack, source),
                        );
                        let _source = layout_source::enter(layout_source::BY_REF_ARGUMENT_BINDING);
                        let cell = match stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .locals
                            .ensure_reference_cell(source)
                        {
                            Ok(cell) => cell,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Some(object) =
                            spl_array_access_local_object_at_frame(stack, frame_index, local)
                        {
                            let span = dense
                                .spans
                                .get(instruction.span.index())
                                .copied()
                                .unwrap_or_default();
                            emit_spl_array_access_bind_reference_notice(
                                compiled, output, stack, state, &object, span,
                            );
                            let message =
                                "Cannot assign by reference to an array dimension of an object"
                                    .to_owned();
                            let diagnostic = RuntimeDiagnostic::new(
                                "E_PHP_VM_ARRAY_ACCESS_BIND_REFERENCE",
                                RuntimeSeverity::FatalError,
                                message.clone(),
                                runtime_source_span(compiled, span),
                                stack_trace(compiled, stack),
                                Some(PhpReferenceClassification::Error),
                            );
                            stack.pop_recycle();
                            return VmResult::runtime_error_with_diagnostic(
                                output.clone(),
                                message,
                                diagnostic,
                            );
                        }
                        if let Err(message) =
                            bind_dim_local_to_reference_cell(stack, local, &dims, append, cell)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        self.record_counter_alias_state(local_alias_state(stack, local));
                        self.record_lvalue_trace_event(
                            if append {
                                "bind-reference-dim-append"
                            } else {
                                "bind-reference-dim"
                            },
                            local,
                            &dims,
                        );
                    }
                    DenseOpcode::InitStaticLocal => {
                        let DenseOperands::StaticLocal {
                            local,
                            name,
                            default,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(name) = dense.names.get(name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode static local name n{name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::EscapedReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::EscapedReference,
                        );
                        let key = (function_id.raw(), name.clone());
                        let cell = if let Some(cell) = state.static_locals.get(&key) {
                            cell.clone()
                        } else {
                            let value = match self.read_dense_operand(compiled, stack, default) {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            };
                            let cell = ReferenceCell::new(value);
                            state.static_locals.insert(key, cell.clone());
                            cell
                        };
                        let local = LocalId::new(local);
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .locals
                            .bind_reference_cell(local, cell)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        self.record_counter_alias_state(local_alias_state(stack, local));
                    }
                    DenseOpcode::BinaryConcatEcho => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Output,
                            "echo",
                        );
                        let DenseOperands::Binary { dst, lhs, rhs } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        // Borrow both operands for the quickened concat fast
                        // path; only the generic fallback materializes owned
                        // values.
                        let lhs = match self.read_dense_operand_ref(compiled, stack, lhs) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let rhs = match self.read_dense_operand_ref(compiled, stack, rhs) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        let value = self.try_quickened_dense_concat_string_string(
                            unit_id,
                            function_id,
                            dense_instruction_index,
                            lhs.as_value(),
                            rhs.as_value(),
                        );
                        let value = match value {
                            Some(value) => value,
                            None => {
                                let lhs = lhs.into_owned();
                                let rhs = rhs.into_owned();
                                match self.execute_binary(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    BinaryOp::Concat,
                                    &lhs,
                                    &rhs,
                                    runtime_source_span(compiled, span),
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        stack.pop_recycle();
                                        return *result;
                                    }
                                }
                            }
                        };
                        // Echo borrows the value, so echo first and move the
                        // value into the register afterwards instead of
                        // cloning it for the register write.
                        if let Err(result) = self.write_echo(compiled, output, stack, state, &value)
                        {
                            stack.pop_recycle();
                            return *result;
                        }
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::BinaryAdd
                    | DenseOpcode::BinarySub
                    | DenseOpcode::BinaryMul
                    | DenseOpcode::BinaryDiv
                    | DenseOpcode::BinaryMod
                    | DenseOpcode::BinaryConcat
                    | DenseOpcode::BinaryPow
                    | DenseOpcode::BinaryBitAnd
                    | DenseOpcode::BinaryBitOr
                    | DenseOpcode::BinaryBitXor
                    | DenseOpcode::BinaryShiftLeft
                    | DenseOpcode::BinaryShiftRight => {
                        let DenseOperands::Binary { dst, lhs, rhs } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        if let Err(result) = self.execute_dense_binary_op(
                            DenseBinaryRequest {
                                compiled,
                                unit_id,
                                function_id,
                                instruction_index: dense_instruction_index,
                                opcode: instruction.opcode,
                                dst,
                                lhs,
                                rhs,
                                span,
                            },
                            output,
                            stack,
                            state,
                        ) {
                            stack.pop_recycle();
                            return *result;
                        }
                    }
                    DenseOpcode::CompareEqual
                    | DenseOpcode::CompareNotEqual
                    | DenseOpcode::CompareIdentical
                    | DenseOpcode::CompareNotIdentical
                    | DenseOpcode::CompareLess
                    | DenseOpcode::CompareLessEqual
                    | DenseOpcode::CompareGreater
                    | DenseOpcode::CompareGreaterEqual
                    | DenseOpcode::CompareSpaceship => {
                        let DenseOperands::Binary { dst, lhs, rhs } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        if let Err(message) = self.execute_dense_compare_op(
                            compiled,
                            stack,
                            instruction.opcode,
                            dst,
                            lhs,
                            rhs,
                        ) {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::UnaryPlus
                    | DenseOpcode::UnaryMinus
                    | DenseOpcode::UnaryNot
                    | DenseOpcode::UnaryBitNot => {
                        let DenseOperands::RegOperand { dst, src } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        match self.execute_dense_unary_op(
                            compiled,
                            stack,
                            instruction.opcode,
                            dst,
                            src,
                        ) {
                            Ok(None) => {}
                            Ok(Some(deprecation)) => {
                                let span = dense_instruction_span(dense, instruction);
                                if let Err(result) = self.emit_implicit_int_deprecation(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    deprecation,
                                    runtime_source_span(compiled, span),
                                ) {
                                    stack.pop_recycle();
                                    return *result;
                                }
                            }
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        }
                    }
                    DenseOpcode::Include => {
                        let DenseOperands::Include { dst, kind, path } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let path = match self.read_dense_operand(compiled, stack, path) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        // The include-path inline cache only installs into an
                        // observed slot; the rich interpreter observes every
                        // IC-capable instruction generically, so mirror that
                        // here before executing the include.
                        if inline_cache_id.is_none() {
                            self.observe_dense_call_inline_cache(
                                compiled,
                                function_id,
                                BlockId::new(block_index),
                                InstrId::new(dense_instruction_index),
                                InlineCacheKind::IncludePath,
                            );
                        }
                        let result = self.execute_include(
                            ExecutionCursor::new(compiled, output, stack, state),
                            IncludeExecutionRequest {
                                site: UnitInlineCacheSite::new(
                                    inline_cache_id,
                                    compiled_unit_cache_key(compiled),
                                    function_id,
                                    BlockId::new(block_index),
                                    InstrId::new(dense_instruction_index),
                                ),
                                instruction_span: dense_instruction_span(dense, instruction),
                                kind,
                                path: &path,
                            },
                        );
                        if !result.status.is_success() || state.pending_throw.is_some() {
                            if include_failure_allows_continuation(kind, &result) {
                                diagnostics.extend(result.diagnostics);
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("bytecode frame is active")
                                    .registers
                                    .set(RegId::new(dst), Value::Bool(false))
                                {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                                instruction_offset = next_instruction_offset;
                                continue;
                            }
                            stack.pop_recycle();
                            return result;
                        }
                        diagnostics.extend(result.diagnostics);
                        let return_value = result.return_value.unwrap_or(Value::Int(1));
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("bytecode frame is active")
                            .registers
                            .set(RegId::new(dst), return_value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::CallFunction | DenseOpcode::CallFunctionDiscard => {
                        let DenseOperands::Call {
                            dst,
                            name,
                            ref args,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let name_index = name;
                        let Some(name) = dense.names.get(name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode name n{name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dense_pcre_fast = self
                            .try_execute_dense_preg_match_start_offset_ascii_fast(
                                name, args, compiled, stack, state,
                            );
                        match dense_pcre_fast {
                            Ok(Some(return_value)) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("bytecode frame is active")
                                    .registers
                                    .set(RegId::new(dst), return_value)
                                {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                                if let Err(message) =
                                    unset_consumed_dense_call_arg_registers_at_frame(
                                        stack,
                                        frame_index,
                                        args,
                                        Some(RegId::new(dst)),
                                    )
                                {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                                if instruction.opcode == DenseOpcode::CallFunctionDiscard {
                                    let discard_src = DenseOperand {
                                        kind: DenseOperandKind::Register,
                                        index: dst,
                                    };
                                    let value = match self.take_consumed_dense_operand(
                                        compiled,
                                        stack,
                                        discard_src,
                                    ) {
                                        Ok(value) => value,
                                        Err(message) => {
                                            let result = self
                                                .runtime_error(output, compiled, stack, message);
                                            stack.pop_recycle();
                                            return result;
                                        }
                                    };
                                    let mut exception_handlers = Vec::new();
                                    let mut pending_control = None;
                                    if let Some(outcome) = self
                                        .run_destructors_for_unreferenced_value(
                                            ExecutionCursor::new(compiled, output, stack, state),
                                            &mut exception_handlers,
                                            &mut pending_control,
                                            &value,
                                        )
                                    {
                                        match outcome {
                                            RaiseOutcome::Done(result) => {
                                                stack.pop_recycle();
                                                return *result;
                                            }
                                            RaiseOutcome::Caught(_) => {
                                                let result = self.runtime_error(
                                                    output,
                                                    compiled,
                                                    stack,
                                                    "E_PHP_VM_BYTECODE_DESTRUCTOR_CATCH_UNSUPPORTED: bytecode discard cannot route a caught destructor exception",
                                                );
                                                stack.pop_recycle();
                                                return result;
                                            }
                                        }
                                    }
                                }
                                instruction_offset = next_instruction_offset;
                                continue;
                            }
                            Ok(None) => {}
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        }
                        // R1.2: a plain positional argument list (no names,
                        // direct values only) has a call shape fully determined
                        // by the dense metadata — byte-identical to what
                        // `function_call_shape` derives after materializing
                        // `CallArgument`s. Deriving it up front lets the IC
                        // resolve the callee before any operand is read, so a
                        // qualifying dense→dense direct-bind call reads bare
                        // values straight into the callee binder and never
                        // allocates the `Vec<CallArgument>`. `by_ref_local` is
                        // set for *any* variable passed positionally (write-back
                        // tracking) and is admitted: the callee gate below
                        // excludes by-ref params, the classic direct-bind shape
                        // ignores it too, and the shared post-call register
                        // unset works from the dense metadata either way. The
                        // dim/property by-ref targets stay on the materialized
                        // path — building them reads object operands and can
                        // fault before the call. Plain operand reads are
                        // effect-free (no warnings), so deferring them past the
                        // IC lookup is not observable.
                        let bare_positional_shape = args.iter().all(|arg| {
                            arg.name.is_none()
                                && matches!(arg.value_kind, IrCallArgValueKind::Direct)
                                && arg.by_ref_dim.is_none()
                                && arg.by_ref_property.is_none()
                                && arg.by_ref_property_dim.is_none()
                        });
                        let mut deferred_values: Option<Vec<CallArgument>> = None;
                        if !bare_positional_shape {
                            deferred_values = Some(
                                match self.read_dense_call_args_for_function(
                                    dense, compiled, stack, name, args,
                                ) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        stack.pop_recycle();
                                        return result;
                                    }
                                },
                            );
                        }
                        // Prefer the per-unit interned normalized name: no
                        // per-call lowering allocation, and the IC guard
                        // compares by symbol identity.
                        let lowered_name = match dense.normalized_interned_name(name_index) {
                            Some(interned) => {
                                self.record_counter_symbolized_call_name_hit();
                                interned.clone()
                            }
                            None => {
                                self.record_counter_symbolized_name_fallback(
                                    "uninterned_call_name",
                                );
                                PhpString::intern(normalize_function_name(name).as_bytes())
                            }
                        };
                        let epoch = state.lookup_epoch();
                        let call_shape = match &deferred_values {
                            Some(values) => function_call_shape(values),
                            // Keep in lockstep with `call_argument_has_by_ref_metadata`:
                            // materialization preserves each target's presence
                            // (`Option` → `Option`), so metadata presence here is
                            // exactly the flag the materialized shape would carry.
                            None => FunctionCallShape {
                                arity: args.len().try_into().unwrap_or(u32::MAX),
                                named_arguments: Vec::new(),
                                by_ref_arguments: CallReferenceMask::from_flags(args.iter().map(
                                    |arg| {
                                        arg.by_ref_local.is_some()
                                            || arg.by_ref_dim.is_some()
                                            || arg.by_ref_property.is_some()
                                            || arg.by_ref_property_dim.is_some()
                                    },
                                )),
                            },
                        };
                        let dense_instruction_id = InstrId::new(dense_instruction_index);
                        let cached_target = if let Some(id) = inline_cache_id {
                            self.lookup_dense_function_call_inline_cache(
                                id,
                                function_id,
                                dense_instruction_id,
                                &lowered_name,
                                epoch,
                                &call_shape,
                            )
                        } else {
                            self.observe_dense_call_inline_cache(
                                compiled,
                                function_id,
                                BlockId::new(block_index),
                                dense_instruction_id,
                                InlineCacheKind::FunctionCall,
                            );
                            self.lookup_function_call_inline_cache(
                                IrInlineCacheSite::classic(
                                    compiled,
                                    function_id,
                                    BlockId::new(block_index),
                                    dense_instruction_id,
                                ),
                                &lowered_name,
                                epoch,
                                &call_shape,
                            )
                        };
                        let target = if let Some(target) = cached_target {
                            self.record_counter_dense_call_ic_hit();
                            target
                        } else {
                            self.record_counter_dense_call_ic_miss();
                            let lowered_str =
                                std::str::from_utf8(lowered_name.as_bytes()).unwrap_or_default();
                            let Some(resolved) =
                                self.resolve_function_call_target(compiled, state, lowered_str)
                            else {
                                self.record_counter_dense_call_fallback("unknown_function");
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
                                    let span = dense_instruction_span(dense, instruction);
                                    tag_throwable_location(&throwable, compiled, span);
                                    state.pending_trace =
                                        Some(capture_backtrace_string(compiled, stack));
                                    state.pending_throw = Some(throwable);
                                    stack.pop_recycle();
                                    return VmResult::propagating_exception(output.clone());
                                }
                                stack.pop_recycle();
                                return result;
                            };
                            if let Some(id) = inline_cache_id {
                                self.install_dense_function_call_inline_cache(
                                    id,
                                    &lowered_name,
                                    epoch,
                                    call_shape,
                                    resolved.clone(),
                                );
                            } else {
                                self.install_function_call_inline_cache(
                                    IrInlineCacheSite::classic(
                                        compiled,
                                        function_id,
                                        BlockId::new(block_index),
                                        dense_instruction_id,
                                    ),
                                    &lowered_name,
                                    epoch,
                                    call_shape,
                                    resolved.clone(),
                                );
                            }
                            resolved
                        };
                        self.record_counter_dense_direct_call_hit();
                        // Resolve first so userland shadowing and namespace
                        // fallback remain authoritative, then run registry
                        // intrinsics from borrowed dense operands before any
                        // call-argument/value vector is materialized.
                        let pre_args_intrinsic = if bare_positional_shape
                            && deferred_values.is_none()
                            && args.len() <= 8
                            && args.iter().all(|arg| {
                                arg.by_ref_dim.is_none()
                                    && arg.by_ref_property.is_none()
                                    && arg.by_ref_property_dim.is_none()
                            })
                            && self.options.inline_caches.enabled()
                            && let FunctionCallCacheTarget::Builtin {
                                kind: FunctionCallBuiltinKind::InternalRegistry,
                                name: builtin_name,
                            } = &target
                        {
                            let mut borrowed_values =
                                smallvec::SmallVec::<[DenseOperandRead<'_>; 8]>::new();
                            for arg in args {
                                match self.read_dense_operand_ref(compiled, stack, arg.value) {
                                    Ok(value) => borrowed_values.push(value),
                                    Err(message) => {
                                        drop(borrowed_values);
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        stack.pop_recycle();
                                        return result;
                                    }
                                }
                            }
                            self.profile_builtin_call(builtin_name, || {
                                self.try_execute_fast_builtin_stub_borrowed(
                                    builtin_name,
                                    &borrowed_values,
                                )
                            })
                        } else {
                            None
                        };
                        // R1.2 fast lane: the IC resolved a current-unit dense
                        // callee whose bind shape sends arguments straight into
                        // frame locals (`is_direct_bind_fast_shape` split: the
                        // dense metadata proved the argument side above, the
                        // callee's params prove the rest here). Read the
                        // operands as bare `Value`s; the direct-bind loop in
                        // `execute_bytecode_function` coerces and writes them
                        // to locals with no `Vec<CallArgument>` in between.
                        let mut fast_lane_values: Option<CallValuesSmall> = None;
                        if bare_positional_shape
                            && let Some(plan) = plan
                            && let FunctionCallCacheTarget::CurrentUnit {
                                unit_identity,
                                function,
                            } = &target
                            && *unit_identity == compiled.cache_identity()
                            && matches!(
                                plan.function_plan(function.index()),
                                Some(DenseFunctionPlan::Dense)
                            )
                            && let Some(callee) = compiled.unit().functions.get(function.index())
                            && args.len() == callee.params.len()
                            && let Some(callee_meta) =
                                plan.call_shape_meta.get(function.index()).copied()
                            && callee_meta.params_bind_direct
                            && callee_meta.elide_frame_args
                            && args.len() <= 8
                        {
                            let mut positional = CallValuesSmall::new();
                            for arg in args {
                                match self.read_dense_operand_with_source(
                                    compiled,
                                    stack,
                                    arg.value,
                                    layout_source::CALL_ARGUMENT_SNAPSHOT,
                                ) {
                                    Ok(value) => {
                                        self.record_counter_value_clone_reason(
                                            layout_source::CALL_ARGUMENT_SNAPSHOT.name(),
                                        );
                                        positional.push(value);
                                    }
                                    Err(message) => {
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        stack.pop_recycle();
                                        return result;
                                    }
                                }
                            }
                            self.record_counter_dense_call_bare_args_hit();
                            fast_lane_values = Some(positional);
                        }
                        let values = if pre_args_intrinsic.is_some() || fast_lane_values.is_some() {
                            Vec::new()
                        } else if let Some(values) = deferred_values {
                            values
                        } else {
                            match self.read_dense_call_args_for_function(
                                dense, compiled, stack, name, args,
                            ) {
                                Ok(values) => values,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        };
                        let result = if let Some(result) = pre_args_intrinsic {
                            result
                        } else if let Some(plan) = plan
                            && let FunctionCallCacheTarget::CurrentUnit {
                                unit_identity,
                                function,
                            } = target.clone()
                            && unit_identity == compiled.cache_identity()
                        {
                            match plan.function_plan(function.index()) {
                                Some(DenseFunctionPlan::Dense) => {
                                    let Some(dense_function) =
                                        plan.unit.functions.get(function.index())
                                    else {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            format!(
                                                "E_PHP_VM_DENSE_BYTECODE_CALL_MISSING: dense function {} is missing",
                                                function.raw()
                                            ),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    };
                                    let Some(ir_function) =
                                        compiled.unit().functions.get(function.index())
                                    else {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            format!(
                                                "E_PHP_VM_DENSE_BYTECODE_CALL_MISSING: IR function {} is missing",
                                                function.raw()
                                            ),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    };
                                    let mut call = FunctionCall::new(values, Vec::new())
                                        .with_call_site_strict_types(
                                            compiled.unit().strict_types,
                                        )
                                        .with_optional_call_span(
                                            plan.unit
                                                .spans
                                                .get(instruction.span.index())
                                                .copied(),
                                        );
                                    if let Some(positional) = fast_lane_values.take() {
                                        call = call.with_positional_values(positional);
                                    }
                                    if !ir_function.attributes.is_empty()
                                        && let Some(message) =
                                            Self::deprecated_attribute_call_message(
                                                compiled,
                                                ir_function,
                                            )
                                        && let Err(result) = self.emit_deprecated_call(
                                            ExecutionCursor::new(compiled, output, stack, state),
                                            message,
                                            call.call_span,
                                        )
                                    {
                                        stack.pop_recycle();
                                        return *result;
                                    }
                                    // Copy-and-patch native leaf tier, fired from
                                    // the hot dense call path. The dense fast path
                                    // dispatches straight to `execute_bytecode_function`,
                                    // bypassing `execute_function_inner` where the
                                    // rich path's leaf hook lives — so without this
                                    // the tier never engages on real (dense) code.
                                    #[cfg(feature = "jit-copy-patch")]
                                    let native = self.try_execute_profiled_copy_patch_leaf(
                                        ExecutionCursor::new(compiled, output, stack, state),
                                        function,
                                        ir_function,
                                        &call,
                                    );
                                    #[cfg(not(feature = "jit-copy-patch"))]
                                    let native: Option<VmResult> = None;
                                    if let Some(result) = native {
                                        result
                                    } else if let Some(value) = {
                                        #[cfg(feature = "jit-cranelift")]
                                        {
                                            self.try_execute_dense_jit_leaf(
                                                compiled,
                                                state,
                                                function,
                                                ir_function,
                                                &call,
                                            )
                                        }
                                        #[cfg(not(feature = "jit-cranelift"))]
                                        {
                                            None
                                        }
                                    } {
                                        VmResult::success_no_output(Some(value))
                                    } else {
                                        self.execute_bytecode_function(
                                            DenseExecutionRequest {
                                                compiled,
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
                                        )
                                    }
                                }
                                Some(DenseFunctionPlan::RichFallback { reason }) => {
                                    self.record_counter_dense_call_fallback(reason);
                                    let function_name = compiled
                                        .unit()
                                        .functions
                                        .get(function.index())
                                        .map(|function| function.name.as_str())
                                        .unwrap_or("<missing>");
                                    self.record_counter_rich_fallback_function_executed(
                                        reason,
                                        function_name,
                                    );
                                    self.execute_function(
                                        compiled,
                                        function,
                                        FunctionCall::new(values, Vec::new())
                                            .with_call_site_strict_types(compiled.unit().strict_types)
                                            .with_optional_call_span(
                                                plan.unit
                                                    .spans
                                                    .get(instruction.span.index())
                                                    .copied(),
                                            ),
                                        output,
                                        stack,
                                        state,
                                    )
                                }
                                None => self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_DENSE_BYTECODE_PLAN_MISSING: function {} is missing from dense execution plan",
                                        function.raw()
                                    ),
                                ),
                            }
                        } else {
                            self.execute_function_call_target(
                                ExecutionCursor::new(compiled, output, stack, state),
                                target,
                                values,
                                Some((
                                    compiled_unit_cache_key(compiled),
                                    function_id,
                                    BlockId::new(block_index),
                                    InstrId::new(dense_instruction_index),
                                )),
                                dense.spans.get(instruction.span.index()).copied(),
                                &None,
                            )
                        };
                        if !result.status.is_success() {
                            self.record_counter_dense_call_fallback("dispatch_error");
                            stack.pop_recycle();
                            return result;
                        }
                        if result.fiber_suspension.is_some() {
                            self.record_counter_dense_call_fallback("fiber_suspension");
                            let result = VmResult::unsupported(
                                output.clone(),
                                "E_PHP_VM_DENSE_BYTECODE_CALL_FIBER_UNSUPPORTED: dense bytecode direct calls do not support fiber suspension yet",
                            );
                            stack.pop_recycle();
                            return result;
                        }
                        let return_value = result.return_value.unwrap_or(Value::Null);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("bytecode frame is active")
                            .registers
                            .set(RegId::new(dst), return_value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if let Err(message) = unset_consumed_dense_call_arg_registers_at_frame(
                            stack,
                            frame_index,
                            args,
                            Some(RegId::new(dst)),
                        ) {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if instruction.opcode == DenseOpcode::CallFunctionDiscard {
                            // Same sequence as the standalone Discard arm:
                            // take the value out of the register and run
                            // destructors for the unreferenced result.
                            let discard_src = DenseOperand {
                                kind: DenseOperandKind::Register,
                                index: dst,
                            };
                            let value = match self.take_consumed_dense_operand(
                                compiled,
                                stack,
                                discard_src,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            };
                            let mut exception_handlers = Vec::new();
                            let mut pending_control = None;
                            if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                &value,
                            ) {
                                match outcome {
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_BYTECODE_DESTRUCTOR_CATCH_UNSUPPORTED: bytecode discard cannot route a caught destructor exception",
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                }
                            }
                        }
                    }
                    DenseOpcode::NewObject => {
                        let DenseOperands::NewObject {
                            dst,
                            class_name,
                            display_class_name,
                            ref args,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(class_name) = dense.names.get(class_name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode class name n{class_name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(display_class_name) = dense.names.get(display_class_name as usize)
                        else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode class name n{display_class_name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let values = match self.read_dense_call_args(dense, compiled, stack, args) {
                            Ok(values) => values,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let result = self.execute_dense_new_object(
                            compiled,
                            plan,
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            inline_cache_id,
                            class_name,
                            display_class_name,
                            values,
                            dense.spans.get(instruction.span.index()).copied(),
                            output,
                            stack,
                            state,
                        );
                        if !result.status.is_success() {
                            stack.pop_recycle();
                            return result;
                        }
                        let value = result.return_value.unwrap_or(Value::Null);
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode caller frame is active")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::MakeClosure => {
                        let DenseOperands::MakeClosure {
                            dst,
                            function,
                            ref captures,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let captured = match self
                            .evaluate_dense_closure_captures(dense, compiled, stack, captures)
                        {
                            Ok(captures) => captures,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let value = make_closure_value(
                            compiled,
                            state,
                            stack,
                            FunctionId::new(function),
                            captured,
                        );
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::AcquireCallable => {
                        let DenseOperands::RegOperand { dst, src } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let value = match self.read_dense_operand(compiled, stack, src) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let value = match acquire_callable_value(compiled, state, stack, value) {
                            Ok(value) => value,
                            Err(message) => {
                                // Dense functions carry no local exception
                                // handlers (try/catch keeps a function on the
                                // rich plan), so the raise propagates to
                                // outer frames.
                                let mut exception_handlers = Vec::new();
                                let mut pending_control = None;
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    dense
                                        .spans
                                        .get(instruction.span.index())
                                        .copied()
                                        .unwrap_or_default(),
                                    message,
                                ) {
                                    RaiseOutcome::Caught(_) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            "E_PHP_VM_DENSE_ACQUIRE_CALLABLE_HANDLER: dense functions have no local exception handlers",
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                }
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::CallCallable => {
                        let DenseOperands::CallableCall {
                            dst,
                            callee,
                            ref args,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let callee = match self.read_dense_operand(compiled, stack, callee) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let values = match self.read_dense_call_args(dense, compiled, stack, args) {
                            Ok(values) => values,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        // Register the dense call site so IC installs and
                        // megamorphic transitions stick (mirrors CallFunction).
                        self.observe_dense_call_inline_cache(
                            compiled,
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            InlineCacheKind::FunctionCall,
                        );
                        let result = self.execute_callable_value_call(
                            compiled,
                            callee,
                            values,
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            dense.spans.get(instruction.span.index()).copied(),
                            output,
                            stack,
                            state,
                            &None,
                        );
                        if !result.status.is_success() {
                            self.record_counter_dense_call_fallback("dispatch_error");
                            // Mirror the rich CallCallable arm: dispatch
                            // errors carrying a throwable present as an
                            // uncaught exception, not a raw diagnostic.
                            if let Some(throwable) = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result))
                            {
                                let result =
                                    self.propagate_exception(output, stack, state, throwable);
                                stack.pop_recycle();
                                return result;
                            }
                            stack.pop_recycle();
                            return result;
                        }
                        if result.fiber_suspension.is_some() {
                            self.record_counter_dense_call_fallback("fiber_suspension");
                            let result = VmResult::unsupported(
                                output.clone(),
                                "E_PHP_VM_DENSE_BYTECODE_CALL_FIBER_UNSUPPORTED: dense bytecode callable calls do not support fiber suspension yet",
                            );
                            stack.pop_recycle();
                            return result;
                        }
                        self.record_counter_dense_callable_call_hit();
                        let value = result.return_value.unwrap_or(Value::Null);
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode caller frame is active")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if let Err(message) = unset_consumed_dense_call_arg_registers_at_frame(
                            stack,
                            frame_index,
                            args,
                            Some(RegId::new(dst)),
                        ) {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::ResolveCallable => {
                        let DenseOperands::ResolveCallable { dst, kind, target } =
                            instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(target_name) = dense.names.get(target as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode callable name n{target}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let callable = match kind {
                            DenseCallableKind::FunctionName => {
                                php_ir::instruction::CallableKind::FunctionName {
                                    name: target_name.clone(),
                                }
                            }
                            DenseCallableKind::MethodPlaceholder => {
                                php_ir::instruction::CallableKind::MethodPlaceholder {
                                    target: target_name.clone(),
                                }
                            }
                            DenseCallableKind::UnresolvedDynamic => {
                                php_ir::instruction::CallableKind::UnresolvedDynamic {
                                    target: target_name.clone(),
                                }
                            }
                        };
                        let value = match resolve_callable(compiled, state, &callable) {
                            Ok(value) => value,
                            Err(message) => {
                                // Same raise contract as the rich arm: the
                                // error is a catchable throwable that
                                // propagates out of the dense body.
                                let result = self.runtime_error_at_optional_span(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    dense.spans.get(instruction.span.index()).copied(),
                                    message,
                                );
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::Pipe => {
                        let DenseOperands::Pipe {
                            dst,
                            input,
                            callable,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let input = match self.read_dense_operand(compiled, stack, input) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let callee = match self.read_dense_operand(compiled, stack, callable) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        // Pipe calls share the callable-call site machinery,
                        // including its inline caches (mirrors CallCallable).
                        self.observe_dense_call_inline_cache(
                            compiled,
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            InlineCacheKind::FunctionCall,
                        );
                        let result = self.execute_callable_value_call(
                            compiled,
                            callee,
                            vec![CallArgument::positional(input)],
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            dense.spans.get(instruction.span.index()).copied(),
                            output,
                            stack,
                            state,
                            &None,
                        );
                        if !result.status.is_success() {
                            self.record_counter_dense_call_fallback("dispatch_error");
                            if let Some(throwable) = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result))
                            {
                                let result =
                                    self.propagate_exception(output, stack, state, throwable);
                                stack.pop_recycle();
                                return result;
                            }
                            stack.pop_recycle();
                            return result;
                        }
                        if result.fiber_suspension.is_some() {
                            self.record_counter_dense_call_fallback("fiber_suspension");
                            let result = VmResult::unsupported(
                                output.clone(),
                                "E_PHP_VM_DENSE_BYTECODE_CALL_FIBER_UNSUPPORTED: dense bytecode pipe calls do not support fiber suspension yet",
                            );
                            stack.pop_recycle();
                            return result;
                        }
                        self.record_counter_dense_callable_call_hit();
                        let value = result.return_value.unwrap_or(Value::Null);
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode caller frame is active")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::CallMethod => {
                        let DenseOperands::MethodCall {
                            dst,
                            object,
                            method,
                            ref args,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let object = match self.read_dense_operand(compiled, stack, object) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if dense.interned_name(method).is_some() {
                            self.record_counter_symbolized_method_name_hit();
                        } else {
                            self.record_counter_symbolized_name_fallback("uninterned_method_name");
                        }
                        let Some(method) = dense.names.get(method as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode method name n{method}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let values = match self.read_dense_call_args(dense, compiled, stack, args) {
                            Ok(values) => values,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let result = self.execute_dense_method_call(
                            compiled,
                            plan,
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            inline_cache_id,
                            object,
                            method,
                            values,
                            dense.spans.get(instruction.span.index()).copied(),
                            output,
                            stack,
                            state,
                        );
                        if !result.status.is_success() {
                            self.record_counter_dense_call_fallback("dispatch_error");
                            stack.pop_recycle();
                            return result;
                        }
                        if result.fiber_suspension.is_some() {
                            self.record_counter_dense_call_fallback("fiber_suspension");
                            let result = VmResult::unsupported(
                                output.clone(),
                                "E_PHP_VM_DENSE_BYTECODE_CALL_FIBER_UNSUPPORTED: dense bytecode method calls do not support fiber suspension yet",
                            );
                            stack.pop_recycle();
                            return result;
                        }
                        let return_value = result.return_value.unwrap_or(Value::Null);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("bytecode frame is active")
                            .registers
                            .set(RegId::new(dst), return_value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::CallStaticMethod => {
                        let DenseOperands::StaticCall {
                            dst,
                            class_name,
                            method,
                            ref args,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(class_name) = dense.names.get(class_name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode class name n{class_name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        if dense.interned_name(method).is_some() {
                            self.record_counter_symbolized_method_name_hit();
                        } else {
                            self.record_counter_symbolized_name_fallback("uninterned_method_name");
                        }
                        let Some(method) = dense.names.get(method as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode method name n{method}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let values = match self.read_dense_call_args(dense, compiled, stack, args) {
                            Ok(values) => values,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let result = self.execute_dense_static_method_call(
                            compiled,
                            plan,
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            inline_cache_id,
                            class_name,
                            method,
                            values,
                            dense.spans.get(instruction.span.index()).copied(),
                            output,
                            stack,
                            state,
                        );
                        if !result.status.is_success() {
                            self.record_counter_dense_call_fallback("dispatch_error");
                            stack.pop_recycle();
                            return result;
                        }
                        if result.fiber_suspension.is_some() {
                            self.record_counter_dense_call_fallback("fiber_suspension");
                            let result = VmResult::unsupported(
                                output.clone(),
                                "E_PHP_VM_DENSE_BYTECODE_CALL_FIBER_UNSUPPORTED: dense bytecode static calls do not support fiber suspension yet",
                            );
                            stack.pop_recycle();
                            return result;
                        }
                        let return_value = result.return_value.unwrap_or(Value::Null);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("bytecode frame is active")
                            .registers
                            .set(RegId::new(dst), return_value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if let Err(message) = unset_consumed_dense_call_arg_registers_at_frame(
                            stack,
                            frame_index,
                            args,
                            Some(RegId::new(dst)),
                        ) {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::NewArray => {
                        let DenseOperands::Dst { dst } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), Value::Array(PhpArray::new()))
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::ArrayInsert | DenseOpcode::LoadConstArrayInsert => {
                        // The fused form writes the constant value register
                        // first and then runs the unchanged insert sequence.
                        let (array, key, value, by_ref_local) = match instruction.operands {
                            DenseOperands::ArrayInsert {
                                array,
                                key,
                                value,
                                by_ref_local,
                            } => (array, key, value, by_ref_local),
                            DenseOperands::LoadConstArrayInsert {
                                value_dst,
                                value_constant,
                                array,
                                key,
                            } => {
                                let constant = match self
                                    .cached_constant_value(compiled, ConstId::new(value_constant))
                                {
                                    Ok(value) => value,
                                    Err(message) => {
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        stack.pop_recycle();
                                        return result;
                                    }
                                };
                                if let Err(message) = stack
                                    .current_mut()
                                    .expect("bytecode frame was pushed")
                                    .registers
                                    .set(RegId::new(value_dst), constant)
                                {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                                (
                                    array,
                                    key,
                                    DenseOperand {
                                        kind: DenseOperandKind::Register,
                                        index: value_dst,
                                    },
                                    None,
                                )
                            }
                            _ => {
                                let result = self.invalid_bytecode_operand_shape(
                                    output,
                                    compiled,
                                    stack,
                                    instruction,
                                );
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let key = match key {
                            Some(key) => match self
                                .read_dense_operand(compiled, stack, key)
                                .and_then(|value| array_key_from_value(&value))
                            {
                                Ok(key) => Some(key),
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            },
                            None => None,
                        };
                        let value = if let Some(local) = by_ref_local {
                            let _source =
                                layout_source::enter(layout_source::BY_REF_ARGUMENT_BINDING);
                            match stack
                                .current_mut()
                                .expect("bytecode frame was pushed")
                                .locals
                                .ensure_reference_cell(LocalId::new(local))
                            {
                                Ok(cell) => Value::Reference(cell),
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        } else {
                            match self.read_dense_operand_last_use(
                                compiled,
                                stack,
                                value,
                                move_plan,
                                dense_instruction_index,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        };
                        let Some(Value::Array(array_value)) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .get_mut(RegId::new(array))
                        else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                "E_PHP_VM_ARRAY_INSERT_TARGET: target is not an array register",
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let was_packed = array_value.is_packed_fast();
                        if let Some(key) = key {
                            array_value.insert(key, value);
                        } else {
                            array_value.append(value);
                            if was_packed && array_value.is_packed_fast() {
                                self.record_counter_array_packed_append_fast_path_hit();
                            }
                        }
                        if was_packed && !array_value.is_packed_fast() {
                            self.record_counter_array_packed_to_mixed_transition();
                        }
                    }
                    DenseOpcode::FetchDim | DenseOpcode::LoadConstFetchDim => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Array,
                            "dim_fetch",
                        );
                        let _clone_source = layout_source::enter(layout_source::ARRAY_ELEMENT_READ);
                        // The fused form performs the constant key load (same
                        // effect as the LoadConst arm) and then runs this
                        // unchanged dimension-fetch sequence on the key
                        // register, so fused and unfused semantics match.
                        let (dst, array, key, quiet) = match instruction.operands {
                            DenseOperands::FetchDim {
                                dst,
                                array,
                                key,
                                quiet,
                            } => (dst, array, key, quiet),
                            DenseOperands::LoadConstFetchDim {
                                key_dst,
                                key_constant,
                                dst,
                                array,
                                quiet,
                            } => {
                                let value = match self
                                    .cached_constant_value(compiled, ConstId::new(key_constant))
                                {
                                    Ok(value) => value,
                                    Err(message) => {
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        stack.pop_recycle();
                                        return result;
                                    }
                                };
                                if let Err(message) = stack
                                    .current_mut()
                                    .expect("bytecode frame was pushed")
                                    .registers
                                    .set(RegId::new(key_dst), value)
                                {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                                (
                                    dst,
                                    array,
                                    DenseOperand {
                                        kind: DenseOperandKind::Register,
                                        index: key_dst,
                                    },
                                    quiet,
                                )
                            }
                            _ => {
                                let result = self.invalid_bytecode_operand_shape(
                                    output,
                                    compiled,
                                    stack,
                                    instruction,
                                );
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        // Runtime lever R3 (array-read release): decide up front,
                        // before any borrow of the source register, whether this
                        // fetch is that register's provably-dead block-local last
                        // use (flag-off leaves `move_plan` `None`, so this is
                        // always `false` and the read path is unchanged).
                        let release_array_register = array.kind == DenseOperandKind::Register
                            && move_plan.is_some_and(|plan| {
                                plan.is_array_release_eligible(dense_instruction_index, array.index)
                            });
                        let array_ref = if array.kind == DenseOperandKind::Local
                            && is_globals_local(ir_function, LocalId::new(array.index))
                        {
                            DenseOperandRead::Owned(Value::Array(state.globals.globals_array()))
                        } else {
                            match self.read_dense_operand_ref(compiled, stack, array) {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        };
                        let key_ref = match self.read_dense_operand_ref(compiled, stack, key) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Value::String(key_string) = key_ref.as_value()
                            && key_string.symbol_id().is_some()
                        {
                            self.record_counter_symbolized_array_key_hit();
                        }
                        let value = self.try_quickened_packed_array_int_key(
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            array_ref.as_value(),
                            key_ref.as_value(),
                        );
                        let value = match value {
                            Some(value) => value,
                            None => match self.try_dense_array_fetch_dim_borrowed(
                                array_ref.as_value(),
                                key_ref.as_value(),
                                quiet,
                            ) {
                                Some(value) => value,
                                None => {
                                    let array = array_ref.into_owned();
                                    let key_value = key_ref.into_owned();
                                    match self.fetch_dim_value(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut diagnostics,
                                        &array,
                                        &key_value,
                                        quiet,
                                        span,
                                    ) {
                                        Ok(value) => value,
                                        Err(result) => {
                                            stack.pop_recycle();
                                            return *result;
                                        }
                                    }
                                }
                            },
                        };
                        // The element value above is an owned copy (references
                        // are dereferenced and cloned), never an alias into the
                        // array, so the dead source-array register clone may now
                        // be dropped. `release_dead_shared_array_register` only
                        // drops shared handles, so the array's owning local
                        // reclaims sole ownership without changing any
                        // PHP-visible value or destructor timing.
                        if release_array_register {
                            self.release_dead_shared_array_register(stack, array.index);
                        }
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::IssetDim => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Array,
                            "dim_isset",
                        );
                        let _clone_source = layout_source::enter(layout_source::ARRAY_ELEMENT_READ);
                        let DenseOperands::IssetDim {
                            dst,
                            local,
                            ref dims,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let local = LocalId::new(local);
                        if let Err(result) = self.emit_dense_dim_float_key_diagnostics(
                            ExecutionCursor::new(compiled, output, stack, state),
                            local,
                            dims,
                            dense_instruction_span(dense, instruction),
                        ) {
                            stack.pop_recycle();
                            return *result;
                        }
                        let dims = match self.read_dense_dim_operands(compiled, stack, dims) {
                            Ok(dims) => dims,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(
                            stack, local,
                        ));
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        match self.try_userland_arrayaccess_offset_exists_local(
                            ExecutionCursor::new(compiled, output, stack, state),
                            local,
                            &dims,
                            span,
                        ) {
                            Ok(Some(result)) => {
                                if let Err(message) = stack
                                    .current_mut()
                                    .expect("bytecode frame was pushed")
                                    .registers
                                    .set(RegId::new(dst), Value::Bool(result))
                                {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                                instruction_offset = next_instruction_offset;
                                continue;
                            }
                            Ok(None) => {}
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        }
                        let local_value = if is_globals_local(ir_function, local) {
                            Some(Value::Array(state.globals.globals_array()))
                        } else {
                            read_local_value(stack, local)
                        };
                        let value = if let Some((object, key_value)) = local_value
                            .as_ref()
                            .and_then(|value| spl_array_access_dim_target(value, &dims))
                        {
                            let exists = match self.call_array_access_dim_method(
                                ExecutionCursor::new(compiled, output, stack, state),
                                object,
                                "offsetExists",
                                key_value,
                                Some(span),
                            ) {
                                Ok(value) => value,
                                Err(result) => {
                                    stack.pop_recycle();
                                    return *result;
                                }
                            };
                            match to_bool(&exists) {
                                Ok(true) => Some(Value::Bool(true)),
                                Ok(false) => None,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        } else if let (Some(base_value), [key]) =
                            (local_value.as_ref(), dims.as_slice())
                        {
                            let base = effective_value(base_value);
                            if let Value::Array(array) = &base {
                                match self.try_array_shape_lookup(array, key) {
                                    Some(value) => value,
                                    None => fetch_dim_path_value(base_value, &dims).ok().flatten(),
                                }
                            } else {
                                fetch_dim_path_value(base_value, &dims).ok().flatten()
                            }
                        } else {
                            local_value.and_then(|value| {
                                fetch_dim_path_value(&value, &dims).ok().flatten()
                            })
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(
                                RegId::new(dst),
                                Value::Bool(!matches!(value, None | Some(Value::Null))),
                            )
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::EmptyDim => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Array,
                            "dim_empty",
                        );
                        let _clone_source = layout_source::enter(layout_source::ARRAY_ELEMENT_READ);
                        let DenseOperands::EmptyDim {
                            dst,
                            local,
                            ref dims,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let local = LocalId::new(local);
                        let dims = match self.read_dense_dim_operands(compiled, stack, dims) {
                            Ok(dims) => dims,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(
                            stack, local,
                        ));
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        match self.try_userland_arrayaccess_offset_empty_local(
                            ExecutionCursor::new(compiled, output, stack, state),
                            local,
                            &dims,
                            span,
                        ) {
                            Ok(Some(result)) => {
                                if let Err(message) = stack
                                    .current_mut()
                                    .expect("bytecode frame was pushed")
                                    .registers
                                    .set(RegId::new(dst), Value::Bool(result))
                                {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                                instruction_offset = next_instruction_offset;
                                continue;
                            }
                            Ok(None) => {}
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        }
                        let local_value = if is_globals_local(ir_function, local) {
                            Some(Value::Array(state.globals.globals_array()))
                        } else {
                            read_local_value(stack, local)
                        };
                        let value = if let Some((object, key_value)) = local_value
                            .as_ref()
                            .and_then(|value| spl_array_access_dim_target(value, &dims))
                        {
                            let exists = match self.call_array_access_dim_method(
                                ExecutionCursor::new(compiled, output, stack, state),
                                object.clone(),
                                "offsetExists",
                                key_value.clone(),
                                Some(span),
                            ) {
                                Ok(value) => value,
                                Err(result) => {
                                    stack.pop_recycle();
                                    return *result;
                                }
                            };
                            let exists = match to_bool(&exists) {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            };
                            if exists {
                                match self.call_array_access_dim_method(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    object,
                                    "offsetGet",
                                    key_value,
                                    Some(span),
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        stack.pop_recycle();
                                        return *result;
                                    }
                                }
                            } else {
                                Value::Uninitialized
                            }
                        } else {
                            // Fail-closed record-like / small-map fast path for
                            // `empty($arr[$key])`. try_array_shape_lookup returns
                            // Some(Some(v)) for a hit, Some(None) for an absent
                            // key, and None for any ambiguous / COW / reference
                            // shape, so this produces exactly the same value the
                            // generic path would (present element or
                            // Uninitialized) and only accelerates proven shapes.
                            // Shape hit/miss/fallback counters are recorded by
                            // the helper.
                            let shape_fast_path = if let (Some(base_value), [key]) =
                                (local_value.as_ref(), dims.as_slice())
                            {
                                let base = effective_value(base_value);
                                if let Value::Array(array) = &base {
                                    self.try_array_shape_lookup(array, key)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            match shape_fast_path {
                                Some(shape_value) => shape_value.unwrap_or(Value::Uninitialized),
                                None => local_value
                                    .and_then(|value| {
                                        fetch_dim_path_value(&value, &dims).ok().flatten()
                                    })
                                    .unwrap_or(Value::Uninitialized),
                            }
                        };
                        let result = match php_empty_access_value(&value) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), Value::Bool(result))
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::UnsetDim => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Array,
                            "dim_unset",
                        );
                        let _clone_source =
                            layout_source::enter(layout_source::ARRAY_ELEMENT_WRITE);
                        let DenseOperands::UnsetDim { local, ref dims } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let local = LocalId::new(local);
                        if let Err(result) = self.emit_dense_dim_float_key_diagnostics(
                            ExecutionCursor::new(compiled, output, stack, state),
                            local,
                            dims,
                            dense_instruction_span(dense, instruction),
                        ) {
                            stack.pop_recycle();
                            return *result;
                        }
                        let dims = match self.read_dense_dim_operands(compiled, stack, dims) {
                            Ok(dims) => dims,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(
                            stack, local,
                        ));
                        let was_packed = if is_globals_local(ir_function, local) {
                            false
                        } else {
                            local_array_is_packed_fast(stack, local)
                        };
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        match self.try_userland_arrayaccess_offset_unset_local(
                            ExecutionCursor::new(compiled, output, stack, state),
                            local,
                            &dims,
                            span,
                        ) {
                            Ok(true) => {
                                self.record_lvalue_trace_event("array-unset-dim", local, &dims);
                                instruction_offset = next_instruction_offset;
                                continue;
                            }
                            Ok(false) => {}
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        }
                        let local_value = if is_globals_local(ir_function, local) {
                            Some(Value::Array(state.globals.globals_array()))
                        } else {
                            read_local_value(stack, local)
                        };
                        if let Some((object, key_value)) = local_value
                            .as_ref()
                            .and_then(|value| spl_array_access_dim_target(value, &dims))
                        {
                            match self.call_array_access_dim_method(
                                ExecutionCursor::new(compiled, output, stack, state),
                                object,
                                "offsetUnset",
                                key_value,
                                Some(span),
                            ) {
                                Ok(_) => {
                                    self.record_lvalue_trace_event("array-unset-dim", local, &dims);
                                    instruction_offset = next_instruction_offset;
                                    continue;
                                }
                                Err(result) => {
                                    stack.pop_recycle();
                                    return *result;
                                }
                            }
                        }
                        let result = if is_globals_local(ir_function, local) {
                            unset_globals_dim(&mut state.globals, &dims)
                        } else {
                            unset_dim_local(stack, local, &dims)
                        };
                        if let Err(message) = result {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if !is_globals_local(ir_function, local)
                            && was_packed
                            && !local_array_is_packed_fast(stack, local)
                        {
                            self.record_counter_array_packed_to_mixed_transition();
                        }
                        self.record_lvalue_trace_event("array-unset-dim", local, &dims);
                    }
                    DenseOpcode::AssignDim | DenseOpcode::AppendDim => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Array,
                            if instruction.opcode == DenseOpcode::AppendDim {
                                "dim_append"
                            } else {
                                "dim_assign"
                            },
                        );
                        let _clone_source =
                            layout_source::enter(layout_source::ARRAY_ELEMENT_WRITE);
                        let DenseOperands::AssignDim {
                            dst,
                            local,
                            ref dims,
                            value,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dim_values = match self.read_dense_dim_values(compiled, stack, dims) {
                            Ok(dims) => dims,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let value = match self.read_dense_operand_last_use(
                            compiled,
                            stack,
                            value,
                            move_plan,
                            dense_instruction_index,
                        ) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let local = LocalId::new(local);
                        let append = instruction.opcode == DenseOpcode::AppendDim;
                        match self.try_userland_arrayaccess_offset_set_local(
                            ExecutionCursor::new(compiled, output, stack, state),
                            local,
                            &dim_values,
                            append,
                            &value,
                            dense
                                .spans
                                .get(instruction.span.index())
                                .copied()
                                .unwrap_or_default(),
                        ) {
                            Ok(true) => {
                                if let Err(message) = stack
                                    .current_mut()
                                    .expect("bytecode frame was pushed")
                                    .registers
                                    .set(RegId::new(dst), value)
                                {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                                instruction_offset = next_instruction_offset;
                                continue;
                            }
                            Ok(false) => {}
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        }
                        if dim_values.iter().any(is_float_dim_key) {
                            let container = stack
                                .current()
                                .expect("bytecode frame was pushed")
                                .locals
                                .get(local)
                                .unwrap_or(Value::Null);
                            let span = dense_instruction_span(dense, instruction);
                            for dim in &dim_values {
                                if is_float_dim_key(dim)
                                    && let Err(result) = self.emit_dim_float_key_diagnostics(
                                        ExecutionCursor::new(compiled, output, stack, state),
                                        &container,
                                        dim,
                                        span,
                                    )
                                {
                                    stack.pop_recycle();
                                    return *result;
                                }
                            }
                        }
                        let dims = match dim_values_to_array_keys(&dim_values) {
                            Ok(dims) => dims,
                            Err(message) => {
                                if !append
                                    && let Some(object) =
                                        spl_multiple_iterator_local_object(stack, local)
                                    && dim_values.len() == 1
                                    && matches!(effective_value(&dim_values[0]), Value::Object(_))
                                {
                                    match self.spl_multiple_iterator_offset_set(
                                        compiled,
                                        &object,
                                        dim_values[0].clone(),
                                        value.clone(),
                                        output,
                                        stack,
                                    ) {
                                        Ok(()) => {
                                            if let Err(message) = stack
                                                .current_mut()
                                                .expect("bytecode frame was pushed")
                                                .registers
                                                .set(RegId::new(dst), value)
                                            {
                                                let result = self.runtime_error(
                                                    output, compiled, stack, message,
                                                );
                                                stack.pop_recycle();
                                                return result;
                                            }
                                            instruction_offset = next_instruction_offset;
                                            continue;
                                        }
                                        Err(result) => {
                                            stack.pop_recycle();
                                            return *result;
                                        }
                                    }
                                }
                                if !append
                                    && let Some(object) =
                                        spl_object_storage_local_object(stack, local)
                                    && dim_values.len() == 1
                                {
                                    let result = spl_container_offset_set(
                                        &object,
                                        dim_values[0].clone(),
                                        value.clone(),
                                    );
                                    if let Err(message) = result {
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    if let Err(message) = stack
                                        .current_mut()
                                        .expect("bytecode frame was pushed")
                                        .registers
                                        .set(RegId::new(dst), value)
                                    {
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        stack.pop_recycle();
                                        return result;
                                    }
                                    instruction_offset = next_instruction_offset;
                                    continue;
                                }
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        // The assignment-expression result register is dead
                        // for plain `$a[$k] = $v;` statements (nothing reads
                        // it), so the value moves into the array slot instead
                        // of being cloned for a store nobody observes.
                        let mut register_value = (!move_plan
                            .is_some_and(|plan| plan.is_dead_write(dst)))
                        .then(|| value.clone());
                        let result = if is_globals_local(ir_function, local) {
                            assign_globals_dim(&mut state.globals, &dims, value, append)
                        } else {
                            let was_packed = local_array_is_packed_fast(stack, local);
                            let cow_or_reference =
                                local_array_has_cow_or_reference_fallback(stack, local);
                            let result = assign_dim_local(stack, local, &dims, value, append);
                            if let Ok(path) = &result {
                                self.record_counter_map_update_slot_path(*path, &dims, append);
                            }
                            let result = result.map(|_| ());
                            if result.is_ok() && cow_or_reference {
                                self.record_counter_cow_or_reference_fallback();
                            }
                            if result.is_ok()
                                && append
                                && dims.is_empty()
                                && was_packed
                                && !cow_or_reference
                                && local_array_is_packed_fast(stack, local)
                            {
                                self.record_counter_array_packed_append_fast_path_hit();
                            }
                            if result.is_ok()
                                && was_packed
                                && !local_array_is_packed_fast(stack, local)
                            {
                                self.record_counter_array_packed_to_mixed_transition();
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
                                    dense
                                        .spans
                                        .get(instruction.span.index())
                                        .copied()
                                        .unwrap_or_default(),
                                    index,
                                );
                                if let Some(register_value) = register_value.take()
                                    && let Err(message) = stack
                                        .current_mut()
                                        .expect("bytecode frame was pushed")
                                        .registers
                                        .set(RegId::new(dst), register_value)
                                {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                                instruction_offset = next_instruction_offset;
                                continue;
                            }
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        self.record_lvalue_trace_event(
                            if append {
                                "array-append-dim"
                            } else {
                                "array-write-dim"
                            },
                            local,
                            &dims,
                        );
                        if let Some(register_value) = register_value.take()
                            && let Err(message) = stack
                                .current_mut()
                                .expect("bytecode frame was pushed")
                                .registers
                                .set(RegId::new(dst), register_value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::ForeachInit => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Array,
                            "foreach_init",
                        );
                        let DenseOperands::ForeachInit { iterator, source } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let source = match self.read_dense_operand(compiled, stack, source) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let span = dense.spans.get(instruction.span.index());
                        let foreach_iterator = match self.foreach_iterator_from_value(
                            compiled,
                            source,
                            output,
                            stack,
                            state,
                            ForeachInvalidSourceBehavior::WarnAndEmpty {
                                span: span.copied(),
                            },
                        ) {
                            Ok(iterator) => iterator,
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        self.record_runtime_trace_event(|| {
                            format!(
                                "foreach init iterator=r{} kind={}",
                                iterator,
                                format_foreach_iterator_kind(&foreach_iterator)
                            )
                        });
                        foreach_iterators.insert(RegId::new(iterator), foreach_iterator);
                    }
                    DenseOpcode::ForeachNext => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Array,
                            "foreach_next",
                        );
                        let DenseOperands::ForeachNext {
                            has_value,
                            iterator,
                            key,
                            value,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let next_value = match self.next_foreach_value(
                            ExecutionCursor::new(compiled, output, stack, state),
                            &mut foreach_iterators,
                            RegId::new(iterator),
                            key.is_some(),
                        ) {
                            Ok(next) => next,
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        let frame = stack
                            .frame_mut(frame_index)
                            .expect("bytecode frame is active");
                        let Some((entry_key, entry_value)) = next_value else {
                            if let Err(message) = frame
                                .registers
                                .set(RegId::new(has_value), Value::Bool(false))
                            {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                            instruction_offset = next_instruction_offset;
                            continue;
                        };
                        if let Err(message) = frame
                            .registers
                            .set(RegId::new(has_value), Value::Bool(true))
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if let Some(key) = key
                            && let Err(message) = frame
                                .registers
                                .set(RegId::new(key), entry_key.unwrap_or(Value::Int(0)))
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                        if let Err(message) = frame.registers.set(RegId::new(value), entry_value) {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::ForeachCleanup => {
                        let DenseOperands::ForeachCleanup { iterator } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        if let Some(value) = foreach_iterators
                            .remove(&RegId::new(iterator))
                            .and_then(foreach_iterator_candidate_value)
                        {
                            let mut exception_handlers = Vec::new();
                            let mut pending_control = None;
                            if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                &value,
                            ) {
                                match outcome {
                                    RaiseOutcome::Done(result) => {
                                        if state.pending_throw.is_none() {
                                            stack.pop_recycle();
                                        }
                                        return *result;
                                    }
                                    RaiseOutcome::Caught(target) => {
                                        let result = self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            format!(
                                                "E_PHP_VM_DENSE_DESTRUCTOR_CATCH_UNSUPPORTED: destructor caught at block {} during foreach cleanup",
                                                target.raw()
                                            ),
                                        );
                                        stack.pop_recycle();
                                        return result;
                                    }
                                }
                            }
                        }
                    }
                    DenseOpcode::IssetPropertyDim | DenseOpcode::EmptyPropertyDim => {
                        let DenseOperands::PropertyDimProbe {
                            dst,
                            object,
                            property,
                            dims,
                        } = &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let is_empty = instruction.opcode == DenseOpcode::EmptyPropertyDim;
                        let Some(property) = dense.names.get(*property as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode property name n{property}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let dims = match self.read_dense_dim_operands(compiled, stack, dims) {
                            Ok(dims) => dims,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let result = {
                            let object_read =
                                match self.read_dense_operand_ref(compiled, stack, *object) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        stack.pop_recycle();
                                        return result;
                                    }
                                };
                            match object_read.as_value() {
                                Value::Object(object) => {
                                    match self.property_dim_probe(
                                        compiled,
                                        state,
                                        stack,
                                        PropertyDimProbe {
                                            object,
                                            property,
                                            dims: &dims,
                                            is_empty,
                                        },
                                    ) {
                                        Ok(value) => Value::Bool(value),
                                        Err(message) => {
                                            let result = self
                                                .runtime_error(output, compiled, stack, message);
                                            stack.pop_recycle();
                                            return result;
                                        }
                                    }
                                }
                                // Non-object receivers mirror the rich arms.
                                _ => Value::Bool(is_empty),
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(*dst), result)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::InstanceOf => {
                        let DenseOperands::InstanceOf {
                            dst,
                            object,
                            class_name,
                        } = &instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let Some(class_name) = dense.names.get(*class_name as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode class name n{class_name}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        // instanceof only inspects the operand; a borrowed
                        // read avoids cloning object/array handles.
                        let value = {
                            let object = match self.read_dense_operand_ref(compiled, stack, *object)
                            {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            };
                            match self.object_instanceof_cached(
                                compiled,
                                state,
                                object.as_value(),
                                class_name,
                            ) {
                                Ok(value) => Value::Bool(value),
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(*dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::FetchProperty => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Object,
                            "property_fetch",
                        );
                        let _clone_source =
                            layout_source::enter(layout_source::OBJECT_PROPERTY_READ);
                        let DenseOperands::FetchProperty {
                            dst,
                            object,
                            property,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        if dense.interned_name(property).is_some() {
                            self.record_counter_symbolized_property_name_hit();
                        } else {
                            self.record_counter_symbolized_name_fallback(
                                "uninterned_property_name",
                            );
                        }
                        let Some(property) = dense.names.get(property as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode property name n{property}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let object = match self.read_dense_operand(compiled, stack, object) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        let value = match self.dense_fetch_property_value(
                            compiled,
                            output,
                            stack,
                            state,
                            &mut diagnostics,
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            inline_cache_id,
                            property,
                            object,
                            span,
                        ) {
                            Ok(value) => value,
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::AssignProperty => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Object,
                            "property_assign",
                        );
                        let DenseOperands::AssignProperty {
                            dst,
                            object,
                            property,
                            value,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        if dense.interned_name(property).is_some() {
                            self.record_counter_symbolized_property_name_hit();
                        } else {
                            self.record_counter_symbolized_name_fallback(
                                "uninterned_property_name",
                            );
                        }
                        let Some(property) = dense.names.get(property as usize) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid dense bytecode property name n{property}"),
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let object = match self.read_dense_operand(compiled, stack, object) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let value = match self.read_dense_operand(compiled, stack, value) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let span = dense
                            .spans
                            .get(instruction.span.index())
                            .copied()
                            .unwrap_or_default();
                        let value = match self.dense_assign_property_value(
                            compiled,
                            output,
                            stack,
                            state,
                            function_id,
                            BlockId::new(block_index),
                            InstrId::new(dense_instruction_index),
                            inline_cache_id,
                            property,
                            object,
                            value,
                            span,
                        ) {
                            Ok(value) => value,
                            Err(result) => {
                                stack.pop_recycle();
                                return *result;
                            }
                        };
                        if let Err(message) = stack
                            .current_mut()
                            .expect("bytecode frame was pushed")
                            .registers
                            .set(RegId::new(dst), value)
                        {
                            let result = self.runtime_error(output, compiled, stack, message);
                            stack.pop_recycle();
                            return result;
                        }
                    }
                    DenseOpcode::Echo => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Output,
                            "echo",
                        );
                        let DenseOperands::Operand { src } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        // Borrow the operand for the shared echo fast path;
                        // only conversion/`__toString` fallbacks need an
                        // owned value (and `&mut` access to the stack).
                        let value = match self.read_dense_operand_ref(compiled, stack, src) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if !Self::try_write_echo_fast(output, value.as_value()) {
                            let value = value.into_owned();
                            if let Err(result) =
                                self.write_echo(compiled, output, stack, state, &value)
                            {
                                stack.pop_recycle();
                                return *result;
                            }
                        }
                    }
                    DenseOpcode::Discard => {
                        let DenseOperands::Operand { src } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let value = match self.take_consumed_dense_operand(compiled, stack, src) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let mut exception_handlers = Vec::new();
                        let mut pending_control = None;
                        if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                            ExecutionCursor::new(compiled, output, stack, state),
                            &mut exception_handlers,
                            &mut pending_control,
                            &value,
                        ) {
                            match outcome {
                                RaiseOutcome::Done(result) => {
                                    stack.pop_recycle();
                                    return *result;
                                }
                                RaiseOutcome::Caught(_) => {
                                    let result = self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        "E_PHP_VM_BYTECODE_DESTRUCTOR_CATCH_UNSUPPORTED: bytecode discard cannot route a caught destructor exception",
                                    );
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        }
                    }
                    DenseOpcode::Jump => {
                        let DenseOperands::Jump { target } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        block_index = target;
                        continue 'dispatch;
                    }
                    DenseOpcode::JumpIfFalse | DenseOpcode::JumpIfTrue => {
                        let DenseOperands::JumpIf { condition, target } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let truthy = match self.read_dense_operand_branch_truthy(
                            compiled,
                            stack,
                            condition,
                            unit_id,
                            function_id,
                            dense_instruction_index,
                        ) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let jump = if instruction.opcode == DenseOpcode::JumpIfFalse {
                            !truthy
                        } else {
                            truthy
                        };
                        let from_block = block_index;
                        let next_block = if jump {
                            target
                        } else {
                            match next_dense_block_index(dense_function, block_index) {
                                Ok(next) => next,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            }
                        };
                        self.record_counter_dense_branch(
                            function_id.raw(),
                            from_block,
                            next_block,
                            truthy,
                            !jump,
                        );
                        block_index = next_block;
                        continue 'dispatch;
                    }
                    DenseOpcode::JumpIf => {
                        let DenseOperands::JumpIfElse {
                            condition,
                            if_true,
                            if_false,
                        } = instruction.operands
                        else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let truthy = match self.read_dense_operand_branch_truthy(
                            compiled,
                            stack,
                            condition,
                            unit_id,
                            function_id,
                            dense_instruction_index,
                        ) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        let next_block = if truthy { if_true } else { if_false };
                        self.record_counter_dense_branch(
                            function_id.raw(),
                            block_index,
                            next_block,
                            truthy,
                            false,
                        );
                        block_index = next_block;
                        continue 'dispatch;
                    }
                    DenseOpcode::Return => {
                        let DenseOperands::Return { value } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let value = match value {
                            // The frame dies with this return; register
                            // operands move out instead of cloning. Dense
                            // functions never carry local finally handlers,
                            // so nothing can observe the register afterwards.
                            Some(value) => {
                                match self.take_consumed_dense_operand(compiled, stack, value) {
                                    Ok(value) => {
                                        self.record_counter_value_clone_reason(
                                            layout_source::RETURN_VALUE.name(),
                                        );
                                        Some(value)
                                    }
                                    Err(message) => {
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        stack.pop_recycle();
                                        return result;
                                    }
                                }
                            }
                            None => None,
                        };
                        let value = match coerce_return_value(
                            compiled,
                            state,
                            ir_function,
                            value,
                            self.typecheck_fast_path_context(),
                        ) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return result;
                            }
                        };
                        if let Some(shared) = call.shared_top_level_locals.as_deref_mut() {
                            export_shared_locals(ir_function, stack, shared);
                        }
                        stack.pop_recycle();
                        let mut result =
                            VmResult::success_with_diagnostics_no_output(value, diagnostics);
                        result.returned_explicitly = !is_synthetic_eof_return(
                            ir_function,
                            dense_instruction_span(dense, instruction),
                            result.return_value.as_ref(),
                        );
                        return result;
                    }
                    DenseOpcode::Exit => {
                        let DenseOperands::Exit { value } = instruction.operands else {
                            let result = self.invalid_bytecode_operand_shape(
                                output,
                                compiled,
                                stack,
                                instruction,
                            );
                            stack.pop_recycle();
                            return result;
                        };
                        let value = match value {
                            Some(value) => match self.read_dense_operand(compiled, stack, value) {
                                Ok(value) => Some(value),
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    stack.pop_recycle();
                                    return result;
                                }
                            },
                            None => None,
                        };
                        let code =
                            match self.resolve_exit_value(compiled, output, stack, state, value) {
                                Ok(code) => code,
                                Err(result) => {
                                    stack.pop_recycle();
                                    return *result;
                                }
                            };
                        state.process_exit_code = Some(code);
                        stack.pop_recycle();
                        return script_exit_result(output, state, code);
                    }
                }
                if let Some(code) = state.process_exit_code {
                    stack.pop_recycle();
                    return script_exit_result(output, state, code);
                }
                instruction_offset = next_instruction_offset;
            }
            let result = self.runtime_error(
                output,
                compiled,
                stack,
                format!("dense bytecode block block:{block_index} has no terminator"),
            );
            stack.pop_recycle();
            return result;
        }
    }
}
