//! Generator and fiber runtime method handling for the VM.

use super::prelude::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct GeneratorYield {
    pub(super) key: Option<Value>,
    pub(super) value: Value,
    /// True when the pair was forwarded from a delegated generator; forwarded
    /// keys must not advance the suspending generator's auto-key counter.
    pub(super) forwarded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct GeneratorContinuation {
    pub(super) frame: Frame,
    pub(super) block_id: BlockId,
    pub(super) instruction_index: usize,
    pub(super) yield_result: php_ir::ids::RegId,
    pub(super) foreach_iterators: HashMap<php_ir::ids::RegId, ForeachIterator>,
    pub(super) exception_handlers: Vec<ExceptionHandler>,
    pub(super) pending_control: Option<PendingControl>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FiberContinuation {
    pub(super) frame: Frame,
    pub(super) block_id: BlockId,
    pub(super) instruction_index: usize,
    pub(super) resume_result: php_ir::ids::RegId,
    pub(super) foreach_iterators: HashMap<php_ir::ids::RegId, ForeachIterator>,
    pub(super) exception_handlers: Vec<ExceptionHandler>,
    pub(super) pending_control: Option<PendingControl>,
}

pub(super) struct FiberContinuationState<'a> {
    resume_result: php_ir::ids::RegId,
    block_id: BlockId,
    instruction_index: usize,
    foreach_iterators: &'a HashMap<php_ir::ids::RegId, ForeachIterator>,
    exception_handlers: &'a [ExceptionHandler],
    pending_control: &'a Option<PendingControl>,
}

impl<'a> FiberContinuationState<'a> {
    pub(super) fn new(
        resume_result: php_ir::ids::RegId,
        block_id: BlockId,
        instruction_index: usize,
        foreach_iterators: &'a HashMap<php_ir::ids::RegId, ForeachIterator>,
        exception_handlers: &'a [ExceptionHandler],
        pending_control: &'a Option<PendingControl>,
    ) -> Self {
        Self {
            resume_result,
            block_id,
            instruction_index,
            foreach_iterators,
            exception_handlers,
            pending_control,
        }
    }

    fn capture(self, frame: Frame) -> FiberContinuation {
        FiberContinuation {
            frame,
            block_id: self.block_id,
            instruction_index: self.instruction_index,
            resume_result: self.resume_result,
            foreach_iterators: self.foreach_iterators.clone(),
            exception_handlers: self.exception_handlers.to_vec(),
            pending_control: self.pending_control.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum FiberResumeInput {
    Value(Value),
    Throw(Value),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FiberSuspension {
    pub(super) value: Value,
    pub(super) continuations: Vec<FiberContinuation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum GeneratorResumeInput {
    Value(Value),
    Throw(Value),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct YieldFromKey {
    pub(super) generator_id: u64,
    pub(super) block_id: BlockId,
    pub(super) instruction_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum YieldFromDelegation {
    Array {
        entries: Vec<(ArrayKey, Value)>,
        position: usize,
    },
    Generator {
        generator: GeneratorRef,
        started: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum YieldFromStep {
    Yield { key: Option<Value>, value: Value },
    Complete(Value),
}

impl Vm {
    pub(super) fn resume_fiber_continuations(
        &self,
        cursor: ExecutionCursor<'_>,
        fiber: FiberRef,
        mut continuations: Vec<FiberContinuation>,
        mut input: FiberResumeInput,
    ) -> VmResult {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        while let Some(continuation) = continuations.pop() {
            let function = continuation.frame.function;
            let result = self.execute_function(
                compiled,
                function,
                FunctionCall::new(Vec::new(), Vec::new()).resume_fiber(
                    fiber.clone(),
                    continuation,
                    input,
                ),
                output,
                stack,
                state,
            );
            if !result.status.is_success() || result.fiber_suspension.is_some() {
                return result;
            }
            if continuations.is_empty() {
                return result;
            }
            input = FiberResumeInput::Value(result.return_value.unwrap_or(Value::Null));
        }
        VmResult::success_no_output(Some(Value::Null))
    }

    pub(super) fn call_generator_method(
        &self,
        cursor: ExecutionCursor<'_>,
        generator: GeneratorRef,
        method: &str,
        args: Vec<CallArgument>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let method_name = normalize_method_name(method);
        if matches!(
            method_name.as_str(),
            "current" | "key" | "next" | "valid" | "rewind" | "getreturn"
        ) {
            validate_generator_arg_count(&method_name, &args, 0)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        }

        match method_name.as_str() {
            "current" => {
                self.advance_generator_to_first_yield(
                    compiled,
                    generator.clone(),
                    output,
                    stack,
                    state,
                )?;
                Ok(generator.current_value().unwrap_or(Value::Null))
            }
            "key" => {
                self.advance_generator_to_first_yield(
                    compiled,
                    generator.clone(),
                    output,
                    stack,
                    state,
                )?;
                Ok(generator.current_key().unwrap_or(Value::Null))
            }
            "valid" => {
                self.advance_generator_to_first_yield(
                    compiled,
                    generator.clone(),
                    output,
                    stack,
                    state,
                )?;
                Ok(Value::Bool(matches!(
                    generator.state(),
                    GeneratorState::Suspended
                )))
            }
            "rewind" => match generator.state() {
                GeneratorState::Created | GeneratorState::Suspended => {
                    self.advance_generator_to_first_yield(
                        compiled, generator, output, stack, state,
                    )?;
                    Ok(Value::Null)
                }
                GeneratorState::Closed => Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_REWIND_CLOSED: cannot rewind a closed generator",
                ))),
                GeneratorState::Running => Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_REENTRANCY: generator is already running",
                ))),
                GeneratorState::Errored => Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_ERRORED: generator already errored",
                ))),
            },
            "next" => {
                match generator.state() {
                    GeneratorState::Created => {
                        self.advance_generator_to_first_yield(
                            compiled,
                            generator.clone(),
                            output,
                            stack,
                            state,
                        )?;
                    }
                    GeneratorState::Suspended => {}
                    GeneratorState::Closed => return Ok(Value::Null),
                    GeneratorState::Running => {
                        return Err(Box::new(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_GENERATOR_REENTRANCY: generator is already running",
                        )));
                    }
                    GeneratorState::Errored => {
                        return Err(Box::new(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_GENERATOR_ERRORED: generator already errored",
                        )));
                    }
                }
                if matches!(generator.state(), GeneratorState::Suspended) {
                    self.resume_generator_to_next_yield(
                        compiled,
                        generator,
                        GeneratorResumeInput::Value(Value::Null),
                        output,
                        stack,
                        state,
                    )?;
                }
                Ok(Value::Null)
            }
            "getreturn" => match generator.state() {
                GeneratorState::Closed => Ok(generator.return_value().unwrap_or(Value::Null)),
                GeneratorState::Created | GeneratorState::Suspended | GeneratorState::Running => {
                    Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_GENERATOR_GET_RETURN_BEFORE_CLOSE: cannot get return value before generator completion",
                    )))
                }
                GeneratorState::Errored => Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_ERRORED: generator already errored",
                ))),
            },
            "send" => {
                validate_generator_arg_count(&method_name, &args, 1)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                if matches!(generator.state(), GeneratorState::Created) {
                    self.advance_generator_to_first_yield(
                        compiled,
                        generator.clone(),
                        output,
                        stack,
                        state,
                    )?;
                }
                if !matches!(generator.state(), GeneratorState::Suspended) {
                    return Ok(Value::Null);
                }
                let next = self.resume_generator_to_next_yield(
                    compiled,
                    generator,
                    GeneratorResumeInput::Value(args[0].value.clone()),
                    output,
                    stack,
                    state,
                )?;
                Ok(next.map(|(_, value)| value).unwrap_or(Value::Null))
            }
            "throw" => {
                validate_generator_arg_count(&method_name, &args, 1)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                if matches!(generator.state(), GeneratorState::Created) {
                    self.advance_generator_to_first_yield(
                        compiled,
                        generator.clone(),
                        output,
                        stack,
                        state,
                    )?;
                }
                let throwable = args[0].value.clone();
                let Value::Object(object) = &throwable else {
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_GENERATOR_THROW_NON_THROWABLE: Generator::throw expects Throwable, {} given",
                            value_type_name(&throwable)
                        ),
                    )));
                };
                if internal_throwable_instanceof(&object.class_name(), "throwable") != Some(true) {
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_GENERATOR_THROW_NON_THROWABLE: Generator::throw expects Throwable, {} given",
                            object.class_name()
                        ),
                    )));
                }
                if !matches!(generator.state(), GeneratorState::Suspended) {
                    return Err(Box::new(self.handle_uncaught_exception(
                        compiled, output, stack, state, throwable,
                    )));
                }
                let next = self.resume_generator_to_next_yield(
                    compiled,
                    generator,
                    GeneratorResumeInput::Throw(throwable),
                    output,
                    stack,
                    state,
                )?;
                Ok(next.map(|(_, value)| value).unwrap_or(Value::Null))
            }
            _ => Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_METHOD: method Generator::{method} is not defined"),
            ))),
        }
    }

    pub(super) fn call_fiber_method(
        &self,
        cursor: ExecutionCursor<'_>,
        fiber: FiberRef,
        method: &str,
        args: Vec<CallArgument>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let method_name = normalize_method_name(method);
        match method_name.as_str() {
            "start" => {
                let args = call_args_to_positional("Fiber::start", args)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?
                    .into_iter()
                    .map(CallArgument::positional)
                    .collect::<Vec<_>>();
                match fiber.state() {
                    FiberState::NotStarted => {}
                    FiberState::Running => {
                        return Err(Box::new(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_FIBER_ALREADY_RUNNING: Cannot start a fiber that has already been started",
                        )));
                    }
                    FiberState::Suspended => {
                        return Err(Box::new(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_FIBER_ALREADY_STARTED: Cannot start a fiber that has already been started",
                        )));
                    }
                    FiberState::Terminated | FiberState::Errored => {
                        return Err(Box::new(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_FIBER_ALREADY_TERMINATED: Cannot start a fiber that has already been started",
                        )));
                    }
                }
                fiber.set_state(FiberState::Running);
                self.record_runtime_trace_event(|| {
                    "fiber start transition=not-started->running".to_owned()
                });
                let result = self.call_fiber_callable(
                    ExecutionCursor::new(compiled, output, stack, state),
                    fiber.clone(),
                    fiber.callable(),
                    args,
                );
                if !result.status.is_success() {
                    fiber.set_state(FiberState::Errored);
                    self.record_runtime_trace_event(|| {
                        "fiber start transition=running->errored".to_owned()
                    });
                    return Err(Box::new(result));
                }
                if let Some(suspension) = result.fiber_suspension {
                    state
                        .fiber_continuations
                        .insert(fiber.id(), suspension.continuations);
                    fiber.set_state(FiberState::Suspended);
                    self.record_runtime_trace_event(|| {
                        format!(
                            "fiber start transition=running->suspended value={}",
                            trace_value(&suspension.value)
                        )
                    });
                    return Ok(suspension.value);
                }
                fiber.terminate(result.return_value);
                self.record_runtime_trace_event(|| {
                    "fiber start transition=running->terminated".to_owned()
                });
                Ok(Value::Null)
            }
            "isstarted" => {
                validate_fiber_arg_count(&method_name, &args, 0)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                Ok(Value::Bool(!matches!(
                    fiber.state(),
                    FiberState::NotStarted
                )))
            }
            "issuspended" => {
                validate_fiber_arg_count(&method_name, &args, 0)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                Ok(Value::Bool(matches!(fiber.state(), FiberState::Suspended)))
            }
            "isrunning" => {
                validate_fiber_arg_count(&method_name, &args, 0)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                Ok(Value::Bool(matches!(fiber.state(), FiberState::Running)))
            }
            "isterminated" => {
                validate_fiber_arg_count(&method_name, &args, 0)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                Ok(Value::Bool(matches!(
                    fiber.state(),
                    FiberState::Terminated | FiberState::Errored
                )))
            }
            "resume" => {
                validate_fiber_arg_count_range(&method_name, &args, 0, 1)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                if !matches!(fiber.state(), FiberState::Suspended) {
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_NOT_SUSPENDED: Cannot resume a fiber that is not suspended",
                    )));
                }
                let Some(continuations) = state.fiber_continuations.remove(&fiber.id()) else {
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_CONTINUATION_MISSING: suspended fiber has no VM continuation",
                    )));
                };
                let resume_value = args
                    .first()
                    .map(|arg| arg.value.clone())
                    .unwrap_or(Value::Null);
                fiber.set_state(FiberState::Running);
                self.record_runtime_trace_event(|| {
                    format!(
                        "fiber resume transition=suspended->running input={}",
                        trace_value(&resume_value)
                    )
                });
                let result = self.resume_fiber_continuations(
                    ExecutionCursor::new(compiled, output, stack, state),
                    fiber.clone(),
                    continuations,
                    FiberResumeInput::Value(resume_value),
                );
                if !result.status.is_success() {
                    fiber.set_state(FiberState::Errored);
                    self.record_runtime_trace_event(|| {
                        "fiber resume transition=running->errored".to_owned()
                    });
                    return Err(Box::new(result));
                }
                if let Some(suspension) = result.fiber_suspension {
                    state
                        .fiber_continuations
                        .insert(fiber.id(), suspension.continuations);
                    fiber.set_state(FiberState::Suspended);
                    self.record_runtime_trace_event(|| {
                        format!(
                            "fiber resume transition=running->suspended value={}",
                            trace_value(&suspension.value)
                        )
                    });
                    return Ok(suspension.value);
                }
                fiber.terminate(result.return_value);
                self.record_runtime_trace_event(|| {
                    "fiber resume transition=running->terminated".to_owned()
                });
                Ok(Value::Null)
            }
            "throw" => {
                validate_fiber_arg_count(&method_name, &args, 1)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                if !matches!(fiber.state(), FiberState::Suspended) {
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_NOT_SUSPENDED: Cannot resume a fiber that is not suspended",
                    )));
                }
                let throwable = args[0].value.clone();
                let Value::Object(object) = &throwable else {
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_FIBER_THROW_NON_THROWABLE: Fiber::throw expects Throwable, {} given",
                            value_type_name(&throwable)
                        ),
                    )));
                };
                if internal_throwable_instanceof(&object.class_name(), "throwable") != Some(true) {
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_FIBER_THROW_NON_THROWABLE: Fiber::throw expects Throwable, {} given",
                            object.class_name()
                        ),
                    )));
                }
                let Some(continuations) = state.fiber_continuations.remove(&fiber.id()) else {
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_CONTINUATION_MISSING: suspended fiber has no VM continuation",
                    )));
                };
                fiber.set_state(FiberState::Running);
                self.record_runtime_trace_event(|| {
                    format!(
                        "fiber throw transition=suspended->running input={}",
                        trace_value(&throwable)
                    )
                });
                let result = self.resume_fiber_continuations(
                    ExecutionCursor::new(compiled, output, stack, state),
                    fiber.clone(),
                    continuations,
                    FiberResumeInput::Throw(throwable),
                );
                if !result.status.is_success() {
                    fiber.set_state(FiberState::Errored);
                    self.record_runtime_trace_event(|| {
                        "fiber throw transition=running->errored".to_owned()
                    });
                    return Err(Box::new(result));
                }
                if let Some(suspension) = result.fiber_suspension {
                    state
                        .fiber_continuations
                        .insert(fiber.id(), suspension.continuations);
                    fiber.set_state(FiberState::Suspended);
                    self.record_runtime_trace_event(|| {
                        format!(
                            "fiber throw transition=running->suspended value={}",
                            trace_value(&suspension.value)
                        )
                    });
                    return Ok(suspension.value);
                }
                fiber.terminate(result.return_value);
                self.record_runtime_trace_event(|| {
                    "fiber throw transition=running->terminated".to_owned()
                });
                Ok(Value::Null)
            }
            "getreturn" => {
                validate_fiber_arg_count(&method_name, &args, 0)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                match fiber.state() {
                    FiberState::Terminated => Ok(fiber.return_value().unwrap_or(Value::Null)),
                    FiberState::Errored => Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_ERRORED: Cannot get fiber return value: The fiber threw an exception",
                    ))),
                    FiberState::NotStarted | FiberState::Running | FiberState::Suspended => {
                        Err(Box::new(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_FIBER_GET_RETURN_BEFORE_TERMINATION: Cannot get fiber return value: The fiber has not been started",
                        )))
                    }
                }
            }
            "suspend" => Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_FIBER_SUSPEND_INSTANCE_CALL: Fiber::suspend must be called statically",
            ))),
            _ => Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_METHOD: method Fiber::{method} is not defined"),
            ))),
        }
    }

    pub(super) fn advance_generator_to_first_yield(
        &self,
        compiled: &CompiledUnit,
        generator: GeneratorRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Option<(Option<Value>, Value)>, Box<VmResult>> {
        match generator.state() {
            GeneratorState::Created => {}
            GeneratorState::Suspended => return Ok(generator.current()),
            GeneratorState::Closed => return Ok(None),
            GeneratorState::Running => {
                return Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_REENTRANCY: generator is already running",
                )));
            }
            GeneratorState::Errored => {
                return Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_ERRORED: generator already errored",
                )));
            }
        }
        generator.set_state(GeneratorState::Running);
        self.record_runtime_trace_event(|| {
            format!(
                "generator state function={} transition=created->running",
                generator.function()
            )
        });
        let args = generator
            .args()
            .into_iter()
            .map(CallArgument::positional)
            .collect();
        let context = generator.call_context();
        let mut call = FunctionCall::new(args, Vec::new())
            .with_call_site_strict_types(
                context
                    .call_site_strict_types
                    .unwrap_or(compiled.unit().strict_types),
            )
            .running_generator(generator.clone());
        if let Some(this_value) = context.this_value {
            call = call.with_this(this_value);
        }
        if let (Some(scope_class), Some(called_class), Some(declaring_class)) = (
            context.scope_class,
            context.called_class,
            context.declaring_class,
        ) {
            // Captured from a call that already went through
            // `with_class_context`, so the handles keep their exact form.
            call = call.with_class_context_handles(scope_class, called_class, declaring_class);
        }
        let result = self.execute_function(
            compiled,
            FunctionId::new(generator.function()),
            call,
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            generator.set_state(GeneratorState::Errored);
            self.record_runtime_trace_event(|| {
                format!(
                    "generator state function={} transition=running->errored",
                    generator.function()
                )
            });
            return Err(Box::new(result));
        }
        if let Some(yielded) = result.yielded {
            if yielded.forwarded {
                generator.suspend_forwarded(yielded.key.clone(), yielded.value.clone());
            } else {
                generator.suspend(yielded.key.clone(), yielded.value.clone());
            }
            self.record_runtime_trace_event(|| {
                format!(
                    "generator suspend function={} key={} value={}",
                    generator.function(),
                    yielded
                        .key
                        .as_ref()
                        .map(trace_value)
                        .unwrap_or_else(|| "None".to_owned()),
                    trace_value(&yielded.value)
                )
            });
            Ok(Some((generator.current_key(), yielded.value)))
        } else {
            generator.close(result.return_value);
            self.record_runtime_trace_event(|| {
                format!(
                    "generator state function={} transition=running->closed",
                    generator.function()
                )
            });
            Ok(None)
        }
    }

    pub(super) fn resume_generator_to_next_yield(
        &self,
        compiled: &CompiledUnit,
        generator: GeneratorRef,
        input: GeneratorResumeInput,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Option<(Option<Value>, Value)>, Box<VmResult>> {
        match generator.state() {
            GeneratorState::Suspended => {}
            GeneratorState::Closed => return Ok(None),
            GeneratorState::Running => {
                return Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_REENTRANCY: generator is already running",
                )));
            }
            GeneratorState::Errored => {
                return Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_ERRORED: generator already errored",
                )));
            }
            GeneratorState::Created => {
                return Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_NOT_STARTED: generator has not reached a yield",
                )));
            }
        }

        let Some(continuation) = state.generator_continuations.remove(&generator.id()) else {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_GENERATOR_CONTINUATION_MISSING: suspended generator has no VM continuation",
            )));
        };
        generator.set_state(GeneratorState::Running);
        self.record_runtime_trace_event(|| {
            format!(
                "generator state function={} transition=suspended->running input={}",
                generator.function(),
                match &input {
                    GeneratorResumeInput::Value(value) => format!("value({})", trace_value(value)),
                    GeneratorResumeInput::Throw(value) => format!("throw({})", trace_value(value)),
                }
            )
        });
        let result = self.execute_function(
            compiled,
            FunctionId::new(generator.function()),
            FunctionCall::new(Vec::new(), Vec::new())
                .running_generator(generator.clone())
                .resume_generator(continuation, input),
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            generator.set_state(GeneratorState::Errored);
            state.generator_continuations.remove(&generator.id());
            self.record_runtime_trace_event(|| {
                format!(
                    "generator state function={} transition=running->errored",
                    generator.function()
                )
            });
            return Err(Box::new(result));
        }
        if let Some(yielded) = result.yielded {
            if yielded.forwarded {
                generator.suspend_forwarded(yielded.key.clone(), yielded.value.clone());
            } else {
                generator.suspend(yielded.key.clone(), yielded.value.clone());
            }
            self.record_runtime_trace_event(|| {
                format!(
                    "generator suspend function={} key={} value={}",
                    generator.function(),
                    yielded
                        .key
                        .as_ref()
                        .map(trace_value)
                        .unwrap_or_else(|| "None".to_owned()),
                    trace_value(&yielded.value)
                )
            });
            Ok(Some((generator.current_key(), yielded.value)))
        } else {
            state.generator_continuations.remove(&generator.id());
            generator.close(result.return_value);
            self.record_runtime_trace_event(|| {
                format!(
                    "generator state function={} transition=running->closed",
                    generator.function()
                )
            });
            Ok(None)
        }
    }

    pub(super) fn advance_yield_from_delegation(
        &self,
        compiled: &CompiledUnit,
        key: YieldFromKey,
        source: Operand,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<YieldFromStep, Box<VmResult>> {
        if !state.yield_from_delegations.contains_key(&key) {
            let source = match read_operand(compiled.unit(), stack, source) {
                Ok(source) => source,
                Err(message) => {
                    return Err(Box::new(
                        self.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
            let delegation = match source {
                Value::Array(array) => YieldFromDelegation::Array {
                    entries: array
                        .iter()
                        .map(|(key, value)| (key.clone(), effective_value(value)))
                        .collect(),
                    position: 0,
                },
                Value::Generator(generator) => YieldFromDelegation::Generator {
                    generator,
                    started: false,
                },
                other => {
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_UNSUPPORTED_YIELD_FROM_SOURCE: yield from over {} is not implemented; runtime-semantics supports arrays and generator MVP objects",
                            value_type_name(&other)
                        ),
                    )));
                }
            };
            state.yield_from_delegations.insert(key.clone(), delegation);
        }

        let Some(mut delegation) = state.yield_from_delegations.remove(&key) else {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_YIELD_FROM_DELEGATION_MISSING: yield from delegation state is missing",
            )));
        };
        let step = match &mut delegation {
            YieldFromDelegation::Array { entries, position } => {
                if let Some((entry_key, value)) = entries.get(*position).cloned() {
                    *position += 1;
                    YieldFromStep::Yield {
                        key: Some(array_key_to_value(entry_key)),
                        value,
                    }
                } else {
                    YieldFromStep::Complete(Value::Null)
                }
            }
            YieldFromDelegation::Generator { generator, started } => {
                let next = if *started {
                    self.resume_generator_to_next_yield(
                        compiled,
                        generator.clone(),
                        GeneratorResumeInput::Value(Value::Null),
                        output,
                        stack,
                        state,
                    )?
                } else {
                    *started = true;
                    self.advance_generator_to_first_yield(
                        compiled,
                        generator.clone(),
                        output,
                        stack,
                        state,
                    )?
                };
                if let Some((key, value)) = next {
                    YieldFromStep::Yield { key, value }
                } else {
                    YieldFromStep::Complete(generator.return_value().unwrap_or(Value::Null))
                }
            }
        };
        if matches!(step, YieldFromStep::Yield { .. }) {
            state.yield_from_delegations.insert(key, delegation);
        }
        Ok(step)
    }

    pub(super) fn suspend_current_fiber(
        &self,
        compiled: &CompiledUnit,
        running_fiber: &Option<FiberRef>,
        args: Vec<CallArgument>,
        continuation: FiberContinuationState<'_>,
        output: &OutputBuffer,
        stack: &mut CallStack,
    ) -> Result<VmResult, Box<VmResult>> {
        let Some(fiber) = running_fiber.as_ref() else {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_FIBER_SUSPEND_OUTSIDE_FIBER: Cannot suspend outside of a fiber",
            )));
        };
        if let Some(name) = args.iter().find_map(|arg| arg.name.as_deref()) {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_UNKNOWN_NAMED_ARG: Fiber::suspend has no builtin parameter ${name}"
                ),
            )));
        }
        if args.len() > 1 {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_TOO_MANY_ARGS: Fiber::suspend expects at most 1 argument(s), {} given",
                    args.len()
                ),
            )));
        }
        let value = args
            .into_iter()
            .next()
            .map(|arg| arg.value)
            .unwrap_or(Value::Null);
        self.record_runtime_trace_event(|| {
            format!(
                "fiber suspend transition=running->suspended state={:?} value={}",
                fiber.state(),
                trace_value(&value)
            )
        });
        let Some(frame) = stack.pop() else {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_FIBER_FRAME_MISSING: fiber frame missing at suspend",
            )));
        };
        let mut result = VmResult::success_no_output(None);
        result.fiber_suspension = Some(Box::new(FiberSuspension {
            value,
            continuations: vec![continuation.capture(frame)],
        }));
        Ok(result)
    }

    pub(super) fn propagate_fiber_suspension(
        &self,
        mut result: VmResult,
        compiled: &CompiledUnit,
        continuation: FiberContinuationState<'_>,
        output: &OutputBuffer,
        stack: &mut CallStack,
    ) -> VmResult {
        if let Some(suspension) = result.fiber_suspension.as_mut() {
            let Some(frame) = stack.pop() else {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_FIBER_FRAME_MISSING: caller frame missing while propagating fiber suspension",
                );
            };
            suspension
                .continuations
                .insert(0, continuation.capture(frame));
        }
        result
    }
}

pub(super) fn new_fiber_object(args: Vec<CallArgument>) -> Result<FiberRef, String> {
    if let Some(name) = args.iter().find_map(|arg| arg.name.as_deref()) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_NAMED_ARG: Fiber::__construct has no builtin parameter ${name}"
        ));
    }
    if args.len() != 1 {
        let id = if args.is_empty() {
            "E_PHP_VM_TOO_FEW_ARGS"
        } else {
            "E_PHP_VM_TOO_MANY_ARGS"
        };
        return Err(format!(
            "{id}: Fiber::__construct expects exactly 1 argument(s), {} given",
            args.len()
        ));
    }
    let callable = args
        .into_iter()
        .next()
        .expect("checked exactly one argument")
        .value;
    if !fiber_constructor_accepts_callable(&callable) {
        return Err(format!(
            "E_PHP_VM_FIBER_CONSTRUCTOR_NOT_CALLABLE: Fiber::__construct expects callable, {} given",
            value_type_name(&callable)
        ));
    }
    Ok(FiberRef::new(callable))
}

pub(super) fn fiber_constructor_accepts_callable(value: &Value) -> bool {
    matches!(
        value,
        Value::Callable(_) | Value::String(_) | Value::Array(_) | Value::Object(_)
    )
}

pub(super) fn validate_fiber_arg_count(
    method: &str,
    args: &[CallArgument],
    expected: usize,
) -> Result<(), String> {
    validate_fiber_arg_count_range(method, args, expected, expected)
}

pub(super) fn validate_fiber_arg_count_range(
    method: &str,
    args: &[CallArgument],
    min: usize,
    max: usize,
) -> Result<(), String> {
    if let Some(name) = args.iter().find_map(|arg| arg.name.as_deref()) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_NAMED_ARG: Fiber::{method} has no builtin parameter ${name}"
        ));
    }
    if args.len() < min || args.len() > max {
        let id = if args.len() < min {
            "E_PHP_VM_TOO_FEW_ARGS"
        } else {
            "E_PHP_VM_TOO_MANY_ARGS"
        };
        let expected = if min == max {
            format!("exactly {min}")
        } else if min == 0 {
            format!("at most {max}")
        } else {
            format!("between {min} and {max}")
        };
        return Err(format!(
            "{id}: Fiber::{method} expects {expected} argument(s), {} given",
            args.len()
        ));
    }
    Ok(())
}

pub(super) fn validate_generator_arg_count(
    method: &str,
    args: &[CallArgument],
    expected: usize,
) -> Result<(), String> {
    if let Some(name) = args.iter().find_map(|arg| arg.name.as_deref()) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_NAMED_ARG: Generator::{method} has no builtin parameter ${name}"
        ));
    }
    if args.len() != expected {
        let id = if args.len() < expected {
            "E_PHP_VM_TOO_FEW_ARGS"
        } else {
            "E_PHP_VM_TOO_MANY_ARGS"
        };
        return Err(format!(
            "{id}: Generator::{method} expects exactly {expected} argument(s), {} given",
            args.len()
        ));
    }
    Ok(())
}
