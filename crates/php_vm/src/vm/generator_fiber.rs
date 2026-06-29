//! Generator and fiber runtime method handling for the VM.

use super::*;

impl Vm {
    pub(super) fn resume_fiber_continuations(
        &self,
        compiled: &CompiledUnit,
        fiber: FiberRef,
        mut continuations: Vec<FiberContinuation>,
        mut input: FiberResumeInput,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
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
        VmResult::success(output.clone(), Some(Value::Null))
    }

    pub(super) fn call_generator_method(
        &self,
        compiled: &CompiledUnit,
        generator: GeneratorRef,
        method: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
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
                GeneratorState::Closed => Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_REWIND_CLOSED: cannot rewind a closed generator",
                )),
                GeneratorState::Running => Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_REENTRANCY: generator is already running",
                )),
                GeneratorState::Errored => Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_ERRORED: generator already errored",
                )),
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
                        return Err(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_GENERATOR_REENTRANCY: generator is already running",
                        ));
                    }
                    GeneratorState::Errored => {
                        return Err(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_GENERATOR_ERRORED: generator already errored",
                        ));
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
                    Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_GENERATOR_GET_RETURN_BEFORE_CLOSE: cannot get return value before generator completion",
                    ))
                }
                GeneratorState::Errored => Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_GENERATOR_ERRORED: generator already errored",
                )),
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
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_GENERATOR_THROW_NON_THROWABLE: Generator::throw expects Throwable, {} given",
                            value_type_name(&throwable)
                        ),
                    ));
                };
                if internal_throwable_instanceof(&object.class_name(), "throwable") != Some(true) {
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_GENERATOR_THROW_NON_THROWABLE: Generator::throw expects Throwable, {} given",
                            object.class_name()
                        ),
                    ));
                }
                if !matches!(generator.state(), GeneratorState::Suspended) {
                    return Err(self.handle_uncaught_exception(
                        compiled, output, stack, state, throwable,
                    ));
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
            _ => Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_METHOD: method Generator::{method} is not defined"),
            )),
        }
    }

    pub(super) fn call_fiber_method(
        &self,
        compiled: &CompiledUnit,
        fiber: FiberRef,
        method: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
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
                        return Err(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_FIBER_ALREADY_RUNNING: FiberError: fiber is already running",
                        ));
                    }
                    FiberState::Suspended => {
                        return Err(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_FIBER_ALREADY_STARTED: FiberError: fiber has already started",
                        ));
                    }
                    FiberState::Terminated | FiberState::Errored => {
                        return Err(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_FIBER_ALREADY_TERMINATED: FiberError: fiber has already terminated",
                        ));
                    }
                }
                fiber.set_state(FiberState::Running);
                self.record_runtime_trace_event("fiber start transition=not-started->running");
                let result = self.call_fiber_callable(
                    compiled,
                    fiber.clone(),
                    fiber.callable(),
                    args,
                    output,
                    stack,
                    state,
                );
                if !result.status.is_success() {
                    fiber.set_state(FiberState::Errored);
                    self.record_runtime_trace_event("fiber start transition=running->errored");
                    return Err(result);
                }
                if let Some(suspension) = result.fiber_suspension {
                    state
                        .fiber_continuations
                        .insert(fiber.id(), suspension.continuations);
                    fiber.set_state(FiberState::Suspended);
                    self.record_runtime_trace_event(format!(
                        "fiber start transition=running->suspended value={}",
                        trace_value(&suspension.value)
                    ));
                    return Ok(suspension.value);
                }
                fiber.terminate(result.return_value);
                self.record_runtime_trace_event("fiber start transition=running->terminated");
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
                validate_fiber_arg_count(&method_name, &args, 1)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                if !matches!(fiber.state(), FiberState::Suspended) {
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_NOT_SUSPENDED: FiberError: fiber is not suspended",
                    ));
                }
                let Some(continuations) = state.fiber_continuations.remove(&fiber.id()) else {
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_CONTINUATION_MISSING: suspended fiber has no VM continuation",
                    ));
                };
                fiber.set_state(FiberState::Running);
                self.record_runtime_trace_event(format!(
                    "fiber resume transition=suspended->running input={}",
                    trace_value(&args[0].value)
                ));
                let result = self.resume_fiber_continuations(
                    compiled,
                    fiber.clone(),
                    continuations,
                    FiberResumeInput::Value(args[0].value.clone()),
                    output,
                    stack,
                    state,
                );
                if !result.status.is_success() {
                    fiber.set_state(FiberState::Errored);
                    self.record_runtime_trace_event("fiber resume transition=running->errored");
                    return Err(result);
                }
                if let Some(suspension) = result.fiber_suspension {
                    state
                        .fiber_continuations
                        .insert(fiber.id(), suspension.continuations);
                    fiber.set_state(FiberState::Suspended);
                    self.record_runtime_trace_event(format!(
                        "fiber resume transition=running->suspended value={}",
                        trace_value(&suspension.value)
                    ));
                    return Ok(suspension.value);
                }
                fiber.terminate(result.return_value);
                self.record_runtime_trace_event("fiber resume transition=running->terminated");
                Ok(Value::Null)
            }
            "throw" => {
                validate_fiber_arg_count(&method_name, &args, 1)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                if !matches!(fiber.state(), FiberState::Suspended) {
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_NOT_SUSPENDED: FiberError: fiber is not suspended",
                    ));
                }
                let throwable = args[0].value.clone();
                let Value::Object(object) = &throwable else {
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_FIBER_THROW_NON_THROWABLE: Fiber::throw expects Throwable, {} given",
                            value_type_name(&throwable)
                        ),
                    ));
                };
                if internal_throwable_instanceof(&object.class_name(), "throwable") != Some(true) {
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_FIBER_THROW_NON_THROWABLE: Fiber::throw expects Throwable, {} given",
                            object.class_name()
                        ),
                    ));
                }
                let Some(continuations) = state.fiber_continuations.remove(&fiber.id()) else {
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_CONTINUATION_MISSING: suspended fiber has no VM continuation",
                    ));
                };
                fiber.set_state(FiberState::Running);
                self.record_runtime_trace_event(format!(
                    "fiber throw transition=suspended->running input={}",
                    trace_value(&throwable)
                ));
                let result = self.resume_fiber_continuations(
                    compiled,
                    fiber.clone(),
                    continuations,
                    FiberResumeInput::Throw(throwable),
                    output,
                    stack,
                    state,
                );
                if !result.status.is_success() {
                    fiber.set_state(FiberState::Errored);
                    self.record_runtime_trace_event("fiber throw transition=running->errored");
                    return Err(result);
                }
                if let Some(suspension) = result.fiber_suspension {
                    state
                        .fiber_continuations
                        .insert(fiber.id(), suspension.continuations);
                    fiber.set_state(FiberState::Suspended);
                    self.record_runtime_trace_event(format!(
                        "fiber throw transition=running->suspended value={}",
                        trace_value(&suspension.value)
                    ));
                    return Ok(suspension.value);
                }
                fiber.terminate(result.return_value);
                self.record_runtime_trace_event("fiber throw transition=running->terminated");
                Ok(Value::Null)
            }
            "getreturn" => {
                validate_fiber_arg_count(&method_name, &args, 0)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                match fiber.state() {
                    FiberState::Terminated => Ok(fiber.return_value().unwrap_or(Value::Null)),
                    FiberState::Errored => Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_FIBER_ERRORED: FiberError: fiber terminated with an exception",
                    )),
                    FiberState::NotStarted | FiberState::Running | FiberState::Suspended => {
                        Err(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_FIBER_GET_RETURN_BEFORE_TERMINATION: FiberError: cannot get fiber return value before termination",
                        ))
                    }
                }
            }
            "suspend" => Err(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_FIBER_SUSPEND_INSTANCE_CALL: Fiber::suspend must be called statically",
            )),
            _ => Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_METHOD: method Fiber::{method} is not defined"),
            )),
        }
    }
}
