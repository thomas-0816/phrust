use super::*;

pub(super) fn execute_rich_exception_instruction(
    vm: &Vm,
    compiled: &CompiledUnit,
    unit: &IrUnit,
    function: &IrFunction,
    frame_index: usize,
    kind: &InstructionKind,
    span: IrSpan,
    shared_top_level_locals: &mut Option<&mut HashMap<String, Slot>>,
    diagnostics: &mut Vec<RuntimeDiagnostic>,
    output: &mut OutputBuffer,
    stack: &mut CallStack,
    state: &mut ExecutionState,
    exception_handlers: &mut Vec<ExceptionHandler>,
    pending_control: &mut Option<PendingControl>,
) -> RichDispatchOutcome {
    match kind {
        InstructionKind::EnterTry {
            catch,
            catch_types,
            finally,
            after,
            exception_local,
        } => {
            exception_handlers.push(ExceptionHandler {
                catch: *catch,
                catch_types: catch_types.clone(),
                finally: *finally,
                after: *after,
                exception_local: *exception_local,
            });
            if let Some(catch) = *catch
                && let Some(failure) = vm.validate_runtime_class_dependencies_in_try(
                    compiled, function, catch, output, stack, state, span,
                )
            {
                match failure {
                    ClassDependencyValidationFailure::Throwable(throwable) => {
                        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
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
                            vm.propagate_exception(output, stack, state, throwable),
                        ));
                    }
                    ClassDependencyValidationFailure::Result(result) => {
                        match vm.route_throwable_result(
                            compiled,
                            output,
                            stack,
                            state,
                            exception_handlers,
                            pending_control,
                            *result,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(Box::new(*result));
                            }
                        }
                    }
                }
            }
        }
        InstructionKind::LeaveTry => {
            let _ = exception_handlers.pop();
        }
        InstructionKind::EndFinally { after } => match pending_control.take() {
            Some(PendingControl::Return(value)) => {
                let mut resume_finally = None;
                while let Some(handler) = exception_handlers.pop() {
                    if let Some(finally) = handler.finally {
                        resume_finally = Some(finally);
                        break;
                    }
                }
                if let Some(finally) = resume_finally {
                    *pending_control = Some(PendingControl::Return(value));
                    return RichDispatchOutcome::Jump(finally);
                }
                let value = match coerce_return_value(
                    compiled,
                    state,
                    function,
                    value,
                    vm.typecheck_fast_path_context(),
                ) {
                    Ok(value) => value,
                    Err(message) => {
                        match vm.raise_runtime_error(
                            compiled,
                            output,
                            stack,
                            state,
                            exception_handlers,
                            pending_control,
                            span,
                            message,
                        ) {
                            RaiseOutcome::Caught(target) => {
                                return RichDispatchOutcome::Jump(target);
                            }
                            RaiseOutcome::Done(result) => {
                                return RichDispatchOutcome::Return(Box::new(*result));
                            }
                        }
                    }
                };
                if let Some(shared) = shared_top_level_locals.as_deref_mut() {
                    export_shared_locals_at_frame(function, stack, frame_index, shared);
                }
                stack.pop_frame_recycle(frame_index);
                return RichDispatchOutcome::Return(Box::new(
                    VmResult::success_with_diagnostics_no_output(
                        value,
                        std::mem::take(diagnostics),
                    ),
                ));
            }
            Some(PendingControl::Throw(value)) => {
                if let Some(target) = handle_throw(
                    compiled,
                    value.clone(),
                    stack,
                    state,
                    exception_handlers,
                    pending_control,
                ) {
                    return RichDispatchOutcome::Jump(target);
                }
                return RichDispatchOutcome::Return(Box::new(
                    vm.propagate_exception(output, stack, state, value),
                ));
            }
            None => {
                return RichDispatchOutcome::Jump(*after);
            }
        },
        InstructionKind::Throw { value } => {
            let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            state.pending_trace = Some(capture_backtrace_string(compiled, stack));
            if let Some(target) = handle_throw(
                compiled,
                value.clone(),
                stack,
                state,
                exception_handlers,
                pending_control,
            ) {
                return RichDispatchOutcome::Jump(target);
            }
            return RichDispatchOutcome::Return(Box::new(
                vm.propagate_exception(output, stack, state, value),
            ));
        }
        InstructionKind::MakeException {
            dst,
            class_name,
            message,
        } => {
            let message = match read_operand_at_frame(unit, stack, frame_index, *message) {
                Ok(value) => value,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let object = match make_exception_object(class_name, &message) {
                Ok(object) => object,
                Err(message) => {
                    return RichDispatchOutcome::Return(Box::new(
                        vm.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let runtime_span = runtime_source_span(compiled, span);
            if let Some(file) = runtime_span.file {
                set_throwable_property(&object, "file", Value::string(file.into_bytes()));
            }
            if let Some(line) = source_span_display_line(compiled, span, false) {
                set_throwable_property(&object, "line", Value::Int(line));
            }
            set_throwable_property(
                &object,
                "trace",
                Value::Array(debug_backtrace_array(compiled, stack, 1, 0)),
            );
            set_throwable_property(
                &object,
                "trace_string",
                Value::string(capture_backtrace_string(compiled, stack).into_bytes()),
            );
            if let Err(message) = stack
                .frame_mut(frame_index)
                .expect("frame was pushed")
                .registers
                .set(*dst, Value::Object(object))
            {
                return RichDispatchOutcome::Return(Box::new(
                    vm.runtime_error(output, compiled, stack, message),
                ));
            }
        }
        _ => unreachable!("non-exception instruction reached exception dispatch"),
    }
    RichDispatchOutcome::Continue
}
