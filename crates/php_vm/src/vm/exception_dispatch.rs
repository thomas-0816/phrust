use super::prelude::*;

impl Vm {
    pub(super) fn throw_exception_result(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        span: php_ir::IrSpan,
        message: String,
    ) -> VmResult {
        let message_value = Value::string(message.into_bytes());
        let throwable = match make_exception_object("Exception", &message_value) {
            Ok(object) => Value::Object(object),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        tag_throwable_location(&throwable, compiled, span);
        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
        state.pending_throw = Some(throwable);
        VmResult::propagating_exception(output.clone())
    }

    pub(super) fn throw_catchable_exception(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        message: String,
    ) -> VmResult {
        let message_value = Value::string(message.into_bytes());
        let throwable = match make_exception_object("Exception", &message_value) {
            Ok(object) => Value::Object(object),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
        state.pending_throw = Some(throwable);
        VmResult::propagating_exception(output.clone())
    }

    #[cold]
    #[inline(never)]
    pub(super) fn runtime_error(
        &self,
        output: &OutputBuffer,
        compiled: &CompiledUnit,
        stack: &CallStack,
        message: impl Into<String>,
    ) -> VmResult {
        let mut message = message.into();
        let diagnostic_message = message.clone();
        if stack.len() > 1 {
            message.push_str("\ncall_stack:");
            for frame in stack.frames().iter().rev() {
                let name = compiled
                    .unit()
                    .functions
                    .get(frame.function.index())
                    .map(|function| function.name.as_str())
                    .unwrap_or("<missing>");
                message.push_str("\n  at ");
                message.push_str(name);
            }
        }
        let diagnostic = runtime_diagnostic_for_message(&diagnostic_message, compiled, stack);
        VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic)
    }

    pub(super) fn runtime_error_with_source_span(
        &self,
        output: &OutputBuffer,
        compiled: &CompiledUnit,
        stack: &CallStack,
        source_span: RuntimeSourceSpan,
        message: impl Into<String>,
    ) -> VmResult {
        let mut message = message.into();
        let diagnostic_message = message.clone();
        if stack.len() > 1 {
            message.push_str("\ncall_stack:");
            for frame in stack.frames().iter().rev() {
                let name = compiled
                    .unit()
                    .functions
                    .get(frame.function.index())
                    .map(|function| function.name.as_str())
                    .unwrap_or("<missing>");
                message.push_str("\n  at ");
                message.push_str(name);
            }
        }
        let diagnostic = runtime_diagnostic_for_message_with_source_span(
            &diagnostic_message,
            compiled,
            stack,
            source_span,
        );
        VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic)
    }

    pub(super) fn runtime_error_with_bringup_context(
        &self,
        output: &OutputBuffer,
        compiled: &CompiledUnit,
        stack: &CallStack,
        state: &ExecutionState,
        source_span: RuntimeSourceSpan,
        message: impl Into<String>,
        context: BringupDiagnosticInput,
    ) -> VmResult {
        let mut message = message.into();
        let diagnostic_message = message.clone();
        if stack.len() > 1 {
            message.push_str("\ncall_stack:");
            for frame in stack.frames().iter().rev() {
                let name = compiled
                    .unit()
                    .functions
                    .get(frame.function.index())
                    .map(|function| function.name.as_str())
                    .unwrap_or("<missing>");
                message.push_str("\n  at ");
                message.push_str(name);
            }
        }
        let mut diagnostic = runtime_diagnostic_for_message_with_source_span(
            &diagnostic_message,
            compiled,
            stack,
            source_span,
        );
        if let Some(payload) = runtime_bringup_payload(
            &diagnostic_message,
            diagnostic.id(),
            compiled,
            state,
            stack,
            context,
        ) {
            diagnostic = diagnostic.with_diagnostic_payload(payload);
        }
        VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic)
    }

    /// Records `throwable` as unwinding past the current frame, pops that frame,
    /// and returns a non-success result so the caller re-throws it through its
    /// own handlers (or, at the entry point, renders it as uncaught).
    pub(super) fn propagate_exception(
        &self,
        output: &OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        throwable: Value,
    ) -> VmResult {
        state.pending_throw = Some(throwable);
        stack.pop_recycle();
        VmResult::propagating_exception(output.clone())
    }

    /// Builds a runtime error for `message` and routes PHP throwables through
    /// the current frame's handlers.
    pub(super) fn raise_runtime_error(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        handlers: &mut Vec<ExceptionHandler>,
        pending_control: &mut Option<PendingControl>,
        span: php_ir::IrSpan,
        message: String,
    ) -> RaiseOutcome {
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
            if let Some(target) = handle_throw(
                compiled,
                throwable.clone(),
                stack,
                state,
                handlers,
                pending_control,
            ) {
                return RaiseOutcome::Caught(target);
            }
            return RaiseOutcome::Done(Box::new(
                self.propagate_exception(output, stack, state, throwable),
            ));
        }
        RaiseOutcome::Done(Box::new(result))
    }

    pub(super) fn raise_runtime_class_entry_error(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        handlers: &mut Vec<ExceptionHandler>,
        pending_control: &mut Option<PendingControl>,
        operation_span: IrSpan,
        error: RuntimeClassEntryError,
    ) -> RaiseOutcome {
        let location_span = error.constant_initializer_span.unwrap_or(operation_span);
        let trace = error
            .constant_initializer_span
            .map(|_| {
                capture_backtrace_string_with_constant_expression(compiled, stack, operation_span)
            })
            .unwrap_or_else(|| capture_backtrace_string(compiled, stack));
        let result = self.runtime_error(output, compiled, stack, error.message);
        if let Some(throwable) = runtime_error_throwable(&result) {
            tag_throwable_location(&throwable, compiled, location_span);
            state.pending_trace = Some(trace);
            if let Some(target) = handle_throw(
                compiled,
                throwable.clone(),
                stack,
                state,
                handlers,
                pending_control,
            ) {
                return RaiseOutcome::Caught(target);
            }
            return RaiseOutcome::Done(Box::new(
                self.propagate_exception(output, stack, state, throwable),
            ));
        }
        RaiseOutcome::Done(Box::new(result))
    }

    pub(super) fn route_throwable_result(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        handlers: &mut Vec<ExceptionHandler>,
        pending_control: &mut Option<PendingControl>,
        result: VmResult,
    ) -> RaiseOutcome {
        if vm_result_has_php_fatal_output(&result) {
            return RaiseOutcome::Done(Box::new(result));
        }
        if let Some(throwable) = state
            .pending_throw
            .take()
            .or_else(|| runtime_error_throwable(&result))
        {
            if let Some(target) = handle_throw(
                compiled,
                throwable.clone(),
                stack,
                state,
                handlers,
                pending_control,
            ) {
                return RaiseOutcome::Caught(target);
            }
            return RaiseOutcome::Done(Box::new(
                self.propagate_exception(output, stack, state, throwable),
            ));
        }
        RaiseOutcome::Done(Box::new(result))
    }

    pub(super) fn handle_uncaught_exception(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        value: Value,
    ) -> VmResult {
        // A registered exception handler may itself throw; PHP routes that new
        // exception to the handler active at that point (which the handler may
        // have just re-registered). Loop so a throwing handler is followed by the
        // current handler, capped to avoid runaway recursion.
        let mut value = value;
        for _ in 0..256 {
            let trace = state.pending_trace.take();
            let Some(callback) = state.exception_handlers.last().cloned() else {
                return uncaught_exception(output, compiled, stack, value, trace);
            };
            let result = self.call_callable(
                compiled,
                Value::Callable(Box::new(callback)),
                vec![CallArgument::positional(value)],
                output,
                stack,
                state,
            );
            if result.status.is_success() {
                return VmResult::success_no_output(None);
            }
            match state.pending_throw.take() {
                Some(next) => value = next,
                None => return result,
            }
        }
        let trace = state.pending_trace.take();
        uncaught_exception(output, compiled, stack, value, trace)
    }
}
