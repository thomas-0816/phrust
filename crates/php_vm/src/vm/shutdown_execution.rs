use super::prelude::*;

impl Vm {
    pub(super) fn run_shutdown_destructors(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        state: &mut ExecutionState,
    ) -> Result<Vec<RuntimeDiagnostic>, Box<VmResult>> {
        let mut diagnostics = Vec::new();
        let mut executed = 0usize;
        while !state.destructor_queue.entries.is_empty() {
            let entries = state.destructor_queue.drain_reverse();
            for entry in entries {
                executed += 1;
                if executed > 4096 {
                    let stack = CallStack::new();
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        &stack,
                        "E_PHP_VM_DESTRUCTOR_QUEUE_OVERFLOW: destructor queue exceeded 4096 executions",
                    )));
                }
                let mut stack = CallStack::new();
                if let Some(diagnostic) =
                    self.inaccessible_destructor_warning(compiled, &stack, &entry)
                {
                    if error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
                        emit_vm_diagnostic(
                            output,
                            state,
                            &diagnostic,
                            php_runtime::api::PhpDiagnosticChannel::Warning,
                            php_runtime::api::PHP_E_WARNING,
                        );
                        diagnostics.push(diagnostic);
                    }
                    continue;
                }
                let owner = destructor_entry_owner(compiled, state, &entry);
                let result = self.execute_function(
                    &owner,
                    entry.function,
                    FunctionCall::new(Vec::new(), Vec::new())
                        .with_this(entry.object.clone())
                        .with_class_context(
                            entry.class_name.clone(),
                            entry.object.display_name(),
                            entry.class_name.clone(),
                        ),
                    output,
                    &mut stack,
                    state,
                );
                if let Some(throwable) = state.pending_throw.take() {
                    return Err(Box::new(self.handle_uncaught_exception(
                        &owner, output, &mut stack, state, throwable,
                    )));
                }
                if !result.status.is_success() {
                    return Err(Box::new(result));
                }
                diagnostics.extend(result.diagnostics);
            }
        }
        Ok(diagnostics)
    }

    pub(super) fn run_shutdown_functions(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        state: &mut ExecutionState,
    ) -> Result<Vec<RuntimeDiagnostic>, Box<VmResult>> {
        let mut diagnostics = Vec::new();
        let mut executed = 0usize;
        while !state.shutdown_functions.is_empty() {
            let entries = std::mem::take(&mut state.shutdown_functions);
            for entry in entries {
                executed += 1;
                if executed > 4096 {
                    let stack = CallStack::new();
                    return Err(Box::new(self.runtime_error(
                        output,
                        compiled,
                        &stack,
                        "E_PHP_VM_SHUTDOWN_FUNCTION_QUEUE_OVERFLOW: shutdown function queue exceeded 4096 executions",
                    )));
                }
                let mut stack = CallStack::new();
                let result = self.call_callable(
                    compiled,
                    entry.callback,
                    entry.args,
                    output,
                    &mut stack,
                    state,
                );
                if let Some(throwable) = state.pending_throw.take() {
                    return Err(Box::new(self.handle_uncaught_exception(
                        compiled, output, &mut stack, state, throwable,
                    )));
                }
                if !result.status.is_success() {
                    return Err(Box::new(result));
                }
                diagnostics.extend(result.diagnostics);
            }
        }
        Ok(diagnostics)
    }

    pub(super) fn run_destructors_for_unreferenced_value(
        &self,
        cursor: ExecutionCursor<'_>,
        handlers: &mut Vec<ExceptionHandler>,
        pending_control: &mut Option<PendingControl>,
        _value: &Value,
    ) -> Option<RaiseOutcome> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        if state.destructor_queue.is_empty() {
            return None;
        }
        let rooted_object_ids = php_visible_root_object_ids(stack, state);
        let candidates = state.destructor_queue.objects_snapshot();
        self.run_destructors_for_unreferenced_candidates_with_roots(
            ExecutionCursor::new(compiled, output, stack, state),
            handlers,
            pending_control,
            candidates,
            &rooted_object_ids,
            None,
        )
        .outcome
    }

    pub(super) fn run_destructors_for_unreferenced_candidates_with_roots(
        &self,
        cursor: ExecutionCursor<'_>,
        handlers: &mut Vec<ExceptionHandler>,
        pending_control: &mut Option<PendingControl>,
        candidates: Vec<ObjectRef>,
        rooted_object_ids: &GcObjectIdSet,
        scope_override: Option<&str>,
    ) -> DestructorSweep {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        for object in candidates {
            if rooted_object_ids.contains(&object.id()) {
                continue;
            }
            let Some(entry) = state.destructor_queue.take_for_object(object.id()) else {
                object.release_php_handle();
                continue;
            };
            let scope = scope_override
                .map(str::to_owned)
                .or_else(|| current_scope_class(compiled, stack));
            if let Some(message) =
                self.inaccessible_destructor_message(compiled, state, &entry, scope.as_deref())
            {
                return DestructorSweep {
                    outcome: Some(self.raise_runtime_error(
                        ExecutionCursor::new(compiled, output, stack, state),
                        handlers,
                        pending_control,
                        php_ir::IrSpan::default(),
                        message,
                    )),
                };
            }
            let owner = destructor_entry_owner(compiled, state, &entry);
            let result = self.execute_function(
                &owner,
                entry.function,
                FunctionCall::new(Vec::new(), Vec::new())
                    .with_this(entry.object.clone())
                    .with_class_context(
                        entry.class_name.clone(),
                        entry.object.display_name(),
                        entry.class_name.clone(),
                    ),
                output,
                stack,
                state,
            );
            if !result.status.is_success() || state.pending_throw.is_some() {
                return DestructorSweep {
                    outcome: Some(self.route_throwable_result(
                        ExecutionCursor::new(compiled, output, stack, state),
                        handlers,
                        pending_control,
                        result,
                    )),
                };
            }
            object.release_php_handle();
        }

        DestructorSweep { outcome: None }
    }
}
