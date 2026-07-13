//! Direct caller-slot to callee-local transfer.

use super::prelude::*;

impl Vm {
    #[cfg(feature = "jit-cranelift")]
    pub(super) fn try_execute_direct_jit(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        function_id: FunctionId,
        function: &IrFunction,
        direct: &DirectCall<'_>,
        frame: &Frame,
    ) -> Option<Value> {
        let tier = self.tiering.borrow_mut().record_function_entry(
            compiled_unit_cache_key(compiled),
            function_id,
            self.options.quickening,
            self.options.jit,
        );
        self.record_counter_jit_tiering_decision(tier);
        let call_shape_supported = direct.receiver.is_none()
            && direct.class_context.scope.is_none()
            && direct.class_context.called.is_none()
            && direct.class_context.declaring.is_none();
        self.try_execute_jit_leaf(JitLeafRequest {
            compiled,
            state,
            function_id,
            function,
            tier,
            call_shape_supported,
            args: JitArgumentSlots::DirectFrame(frame),
        })
    }

    pub(super) fn bind_owned_direct_values(
        &self,
        mut direct_values: Vec<Value>,
        compiled: &CompiledUnit,
        ir_function: &IrFunction,
        strict_types: bool,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let bound_count = direct_values.len().min(ir_function.params.len());
        for arg_index in 0..bound_count {
            let param = &ir_function.params[arg_index];
            let mut value = std::mem::replace(&mut direct_values[arg_index], Value::Uninitialized);
            if let Err(message) = coerce_or_check_param_type(
                ParamTypecheckRequest {
                    compiled,
                    state,
                    function: ir_function,
                    param,
                    arg_index,
                    fast_path: self.typecheck_fast_path_context(),
                    strict_types,
                    call_span,
                },
                &mut value,
            ) {
                direct_values[arg_index] = value;
                let entries = (0..direct_values.len())
                    .map(|index| {
                        let param = ir_function.params.get(index);
                        let value = match param {
                            Some(param) if index < arg_index => stack
                                .current()
                                .and_then(|frame| frame.locals.get(param.local))
                                .unwrap_or(Value::Null),
                            _ => direct_values[index].clone(),
                        };
                        FrameTraceArgument {
                            name: None,
                            value: trace_value_for_param(
                                &value,
                                param.is_some_and(param_is_sensitive),
                            ),
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
                    return Err(self.propagate_exception(output, stack, state, throwable));
                }
                stack.pop_recycle();
                return Err(result);
            }
            let locals = &mut stack
                .current_mut()
                .expect("bytecode frame was pushed")
                .locals;
            if let Err(message) = locals.set(param.local, value) {
                let result = self.runtime_error(output, compiled, stack, message);
                stack.pop_recycle();
                return Err(result);
            }
        }
        Ok(())
    }

    pub(super) fn bind_dense_direct_call(
        &self,
        direct_call: &DirectCall<'_>,
        compiled: &CompiledUnit,
        ir_function: &IrFunction,
        frame_index: usize,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let DirectArgumentSources::Dense(args) = direct_call.argument_sources;
        debug_assert_eq!(args.len(), ir_function.params.len());
        for (arg_index, (arg, param)) in args.iter().zip(&ir_function.params).enumerate() {
            let move_register = direct_call.move_source.filter(|source| {
                arg.value.kind == DenseOperandKind::Register && source.raw() == arg.value.index
            });
            let mut value = match arg.value.kind {
                DenseOperandKind::Constant => {
                    let id = ConstId::new(arg.value.index);
                    if let Some(value) = self.resolved_constant_value(compiled, id) {
                        value
                    } else {
                        match constant_value(compiled.unit(), id) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return Err(result);
                            }
                        }
                    }
                }
                DenseOperandKind::Register => {
                    let Some((caller, _callee)) =
                        stack.split_frames_mut(direct_call.caller_frame, frame_index)
                    else {
                        let result = self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_DIRECT_CALL_FRAME_SPLIT: caller/callee frames are invalid",
                        );
                        stack.pop_recycle();
                        return Err(result);
                    };
                    let register = RegId::new(arg.value.index);
                    if move_register.is_some() {
                        match caller.registers.take(register) {
                            Ok(value) => value,
                            Err(message) => {
                                let result = self.runtime_error(output, compiled, stack, message);
                                stack.pop_recycle();
                                return Err(result);
                            }
                        }
                    } else {
                        let Some(value) = caller.registers.get(register) else {
                            let result = self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!("invalid direct-call register r{}", register.raw()),
                            );
                            stack.pop_recycle();
                            return Err(result);
                        };
                        value.clone()
                    }
                }
                DenseOperandKind::Local => {
                    let Some((caller, _callee)) =
                        stack.split_frames_mut(direct_call.caller_frame, frame_index)
                    else {
                        let result = self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_DIRECT_CALL_FRAME_SPLIT: caller/callee frames are invalid",
                        );
                        stack.pop_recycle();
                        return Err(result);
                    };
                    caller
                        .locals
                        .get(LocalId::new(arg.value.index))
                        .unwrap_or(Value::Null)
                }
            };
            if let Value::Reference(reference) = value {
                value = reference.get();
            }
            self.record_counter_direct_call_transfer(move_register.is_some());
            if let Err(message) = coerce_or_check_param_type(
                ParamTypecheckRequest {
                    compiled,
                    state,
                    function: ir_function,
                    param,
                    arg_index,
                    fast_path: self.typecheck_fast_path_context(),
                    strict_types: direct_call.strict_types,
                    call_span: direct_call.span,
                },
                &mut value,
            ) {
                let mut entries = Vec::with_capacity(args.len());
                for (index, trace_arg) in args.iter().enumerate() {
                    let trace_param = ir_function.params.get(index);
                    let trace_value = if index < arg_index {
                        trace_param
                            .and_then(|param| {
                                stack
                                    .frame_mut(frame_index)
                                    .and_then(|frame| frame.locals.get(param.local))
                            })
                            .unwrap_or(Value::Null)
                    } else if index == arg_index {
                        value.clone()
                    } else {
                        self.read_dense_operand_at_frame(
                            compiled,
                            stack,
                            direct_call.caller_frame,
                            trace_arg.value,
                        )
                        .unwrap_or(Value::Null)
                    };
                    entries.push(FrameTraceArgument {
                        name: None,
                        value: trace_value_for_param(
                            &trace_value,
                            trace_param.is_some_and(param_is_sensitive),
                        ),
                    });
                }
                if let Some(frame) = stack.frame_mut(frame_index) {
                    frame.trace_arguments = TraceArguments::Materialized(entries);
                }
                let result = self.runtime_error(output, compiled, stack, message);
                if let Some(throwable) = runtime_error_throwable(&result) {
                    tag_throwable_location(&throwable, compiled, ir_function.span);
                    state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                    return Err(self.propagate_exception(output, stack, state, throwable));
                }
                stack.pop_recycle();
                return Err(result);
            }
            let Some(frame) = stack.frame_mut(frame_index) else {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_DIRECT_CALL_FRAME: direct callee frame is missing",
                ));
            };
            let locals = &mut frame.locals;
            if let Err(message) = locals.set(param.local, value) {
                let result = self.runtime_error(output, compiled, stack, message);
                stack.pop_recycle();
                return Err(result);
            }
        }
        Ok(())
    }
}
