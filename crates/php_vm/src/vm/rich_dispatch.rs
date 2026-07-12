use super::*;

impl Vm {
    pub(super) fn execute_function(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        call: FunctionCall<'_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let profile_boundary = self.request_profile_boundary_start();
        // The name clone only pays off when a profile boundary is active;
        // unprofiled requests skip it on every call.
        let function_profile = profile_boundary.is_some().then(|| {
            compiled
                .unit()
                .functions
                .get(function_id.index())
                .map(|function| (function.name.clone(), function.flags.is_method))
        });
        let function_profile = function_profile.flatten();
        let result = self.execute_function_inner(compiled, function_id, call, output, stack, state);
        if let Some((name, is_method)) = function_profile {
            self.record_counter_function_profile(&name, is_method, profile_boundary);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_function_inner(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        mut call: FunctionCall<'_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        // Bare positional values are a dense-executor hand-off; the rich path
        // binds from `call.args` and would silently see a zero-arg call.
        debug_assert!(
            call.positional_values.is_none(),
            "pre-bound positional values reached the rich call path"
        );
        let unit = compiled.unit();
        let Some(function) = unit.functions.get(function_id.index()) else {
            return self.runtime_error(output, compiled, stack, "called function is missing");
        };
        if !function.attributes.is_empty()
            && let Some(message) = Self::deprecated_attribute_call_message(compiled, function)
            && let Err(result) = self.emit_deprecated_call(
                ExecutionCursor::new(compiled, output, stack, state),
                message,
                call.call_span,
            )
        {
            return *result;
        }
        if call.this_value.is_some()
            && !function.locals.iter().any(|local| local == "this")
            && let Some(declaring_class) = call.declaring_class.as_deref()
            && let Some(owner) = dynamic_class_owner_in_state(state, declaring_class)
            && &owner != compiled
            && owner
                .unit()
                .functions
                .get(function_id.index())
                .is_some_and(|entry| entry.locals.iter().any(|local| local == "this"))
        {
            return self.execute_function(&owner, function_id, call, output, stack, state);
        }
        // Copy-and-patch native leaf tier runs before dense dispatch so a
        // recognized scalar-int leaf executes natively rather than densely. A
        // recognized native→userland tail call performs the callee here, on the
        // normal path, so the returned `VmResult` (result or exception) is
        // propagated faithfully.
        #[cfg(feature = "jit-copy-patch")]
        if let Some(result) = self.try_execute_copy_patch_leaf(
            ExecutionCursor::new(compiled, output, stack, state),
            function_id,
            function,
            &call,
        ) {
            return result;
        }
        call = match self.try_execute_cached_dense_function_dispatch(
            compiled,
            function_id,
            function,
            call,
            output,
            stack,
            state,
        ) {
            CachedDenseFunctionDispatch::Executed(result) => return *result,
            CachedDenseFunctionDispatch::Continue(call) => call,
        };
        let function_tier = self.tiering.borrow_mut().record_function_entry(
            compiled_unit_cache_key(compiled),
            function_id,
            self.options.quickening,
            self.options.jit,
        );
        self.record_counter_jit_tiering_decision(function_tier);
        let mut diagnostics = Vec::new();
        let mut block_id;
        let mut start_instruction_index = 0usize;
        let mut steps = 0usize;
        let mut foreach_iterators;
        let mut exception_handlers: Vec<ExceptionHandler>;
        let mut pending_control: Option<PendingControl>;
        let running_fiber = call.running_fiber.clone();

        if let Some(continuation) = call.resume_fiber_continuation.take() {
            block_id = continuation.block_id;
            start_instruction_index = continuation.instruction_index;
            foreach_iterators = continuation.foreach_iterators;
            exception_handlers = continuation.exception_handlers;
            pending_control = continuation.pending_control;
            stack.push(continuation.frame);
            self.record_counter_frame_reuse_blocked("fiber_continuation");
            stack
                .current_mut()
                .expect("resumed fiber frame is active")
                .reuse_eligible = false;
            match call
                .resume_fiber_input
                .take()
                .unwrap_or(FiberResumeInput::Value(Value::Null))
            {
                FiberResumeInput::Value(value) => {
                    if let Err(message) = stack
                        .current_mut()
                        .expect("resumed fiber frame is active")
                        .registers
                        .set(continuation.resume_result, value)
                    {
                        return self.runtime_error(output, compiled, stack, message);
                    }
                }
                FiberResumeInput::Throw(value) => {
                    if let Some(target) = handle_throw(
                        compiled,
                        value.clone(),
                        stack,
                        state,
                        &mut exception_handlers,
                        &mut pending_control,
                    ) {
                        block_id = target;
                        start_instruction_index = 0;
                    } else {
                        stack.pop();
                        return self
                            .handle_uncaught_exception(compiled, output, stack, state, value);
                    }
                }
            }
        } else if let Some(continuation) = call.resume_continuation.take() {
            block_id = continuation.block_id;
            start_instruction_index = continuation.instruction_index;
            foreach_iterators = continuation.foreach_iterators;
            exception_handlers = continuation.exception_handlers;
            pending_control = continuation.pending_control;
            stack.push(continuation.frame);
            self.record_counter_frame_reuse_blocked("generator_continuation");
            stack
                .current_mut()
                .expect("resumed generator frame is active")
                .reuse_eligible = false;
            match call
                .resume_input
                .take()
                .unwrap_or(GeneratorResumeInput::Value(Value::Null))
            {
                GeneratorResumeInput::Value(value) => {
                    if let Err(message) = stack
                        .current_mut()
                        .expect("resumed generator frame is active")
                        .registers
                        .set(continuation.yield_result, value)
                    {
                        return self.runtime_error(output, compiled, stack, message);
                    }
                }
                GeneratorResumeInput::Throw(value) => {
                    if let Some(target) = handle_throw(
                        compiled,
                        value.clone(),
                        stack,
                        state,
                        &mut exception_handlers,
                        &mut pending_control,
                    ) {
                        block_id = target;
                        start_instruction_index = 0;
                    } else {
                        stack.pop();
                        return self
                            .handle_uncaught_exception(compiled, output, stack, state, value);
                    }
                }
            }
        } else {
            let jit_call_shape_supported = call.captures.is_empty()
                && call.this_value.is_none()
                && call.scope_class.is_none()
                && call.called_class.is_none()
                && call.declaring_class.is_none()
                && call.shared_top_level_locals.is_none()
                && call.running_generator.is_none()
                && call.running_fiber.is_none();
            let frame_shape = self.frame_shape_flags(compiled, function_id, function);
            let frame_reuse_call_shape_reason = frame_reuse_call_shape_blocked_reason(
                function,
                &call,
                frame_shape,
                self.options.reuse_class_context_frames,
            );
            let frame_layout = call_frame_layout_class(function, &call, frame_shape);
            let argument_policy = call.argument_binding_policy(compiled);
            let elide_frame_args = self.frame_args_elidable(compiled, function_id, function);
            self.record_counter_direct_frame(frame_layout, function, elide_frame_args);
            let prepared = match arguments::prepare_arguments(
                compiled,
                function,
                call.args,
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
                    let error_span = call.call_span.unwrap_or(function.span);
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
                                function,
                                error_span,
                            )
                        });
                        state.pending_throw = Some(throwable);
                        return VmResult::propagating_exception(output.clone());
                    }
                    return result;
                }
            };
            let frame_reuse_blocked_reason = frame_reuse_call_shape_reason
                .or_else(|| frame_reuse_prepared_args_blocked_reason(&prepared.args));
            self.record_counter_call_frame_layout(frame_layout);
            let specialized_frame_fallback = specialized_call_frame_fallback_reason(
                frame_layout,
                frame_reuse_blocked_reason,
                frame_reuse_prepared_args_blocked_reason(&prepared.args).is_some(),
            );
            let specialized_tiny_frame = specialized_frame_fallback.is_none();
            if frame_layout == "tiny_leaf_frame" {
                self.record_counter_tiny_frame_candidate();
            }
            if let Some(reason) = specialized_frame_fallback {
                self.record_counter_generic_frame_fallback(reason);
            }
            let args = prepared.args;
            let frame_args = prepared.frame_args;
            for diagnostic in prepared.diagnostics {
                let handled = match self.dispatch_error_handler(
                    compiled,
                    output,
                    stack,
                    state,
                    php_runtime::api::PHP_E_WARNING,
                    &diagnostic,
                ) {
                    Ok(handled) => handled,
                    Err(result) => return *result,
                };
                if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
                    emit_vm_diagnostic(
                        output,
                        state,
                        &diagnostic,
                        php_runtime::api::PhpDiagnosticChannel::Warning,
                        php_runtime::api::PHP_E_WARNING,
                    );
                    diagnostics.push(diagnostic);
                }
            }
            if function.flags.is_generator && call.running_generator.is_none() {
                self.record_counter_frame_reuse_blocked("generator");
                if function.returns_by_ref {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_RUNTIME_GENERATOR_BY_REF_YIELD_GAP: by-reference generator yields are not implemented in generator-yield",
                    );
                }
                if args.iter().any(|arg| arg.reference.is_some()) {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_UNSUPPORTED_GENERATOR_BY_REF_ARG: generator by-reference arguments are not implemented in generator-argument",
                    );
                }
                let generator_context = GeneratorCallContext {
                    this_value: call.this_value.clone(),
                    scope_class: call.scope_class.clone(),
                    called_class: call.called_class.clone(),
                    declaring_class: call.declaring_class.clone(),
                    call_site_strict_types: call.call_site_strict_types,
                };
                let generator_args = args.into_iter().map(|arg| arg.value).collect();
                return VmResult::success_no_output(Some(Value::Generator(
                    GeneratorRef::new_with_context(
                        function_id.raw(),
                        generator_args,
                        generator_context,
                    ),
                )));
            }
            if let Some(value) = self.try_execute_jit_leaf(JitLeafRequest {
                compiled,
                state,
                function_id,
                function,
                tier: function_tier,
                call_shape_supported: jit_call_shape_supported,
                args: &args,
            }) {
                return VmResult::success_no_output(Some(value));
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
                    function.register_count,
                    function.local_count,
                    activation_context,
                );
                false
            } else {
                stack.push_reusable_frame(
                    function_id,
                    function.register_count,
                    function.local_count,
                    activation_context,
                )
            };
            self.record_counter_frame_activation(
                reused_frame,
                function.register_count,
                function.local_count,
            );
            if specialized_tiny_frame {
                self.record_counter_specialized_frame_hit();
                if reused_frame {
                    self.record_counter_heap_frame_avoided();
                }
            }
            {
                let frame = stack.current_mut().expect("frame was pushed");
                // Backtrace arguments reconstruct lazily from the live
                // locals (reference-engine semantics); nothing to build here.
                frame.trace_arguments = TraceArguments::Lazy {
                    arg_count: args.len() as u32,
                };
                if specialized_tiny_frame {
                    if !args.is_empty() {
                        self.record_counter_arg_array_avoided();
                    }
                } else {
                    frame.arguments = frame_args;
                }
            }
            if let Err(message) = initialize_captures(function, call.captures, stack) {
                let result = self.runtime_error(output, compiled, stack, message);
                stack.pop_recycle();
                return result;
            }
            if function.captures.iter().any(|capture| capture.by_ref) {
                self.record_counter_alias_state_transition(
                    AliasState::NoReferencesObserved,
                    AliasState::EscapedReference,
                );
                self.record_counter_fast_path_disabled_by_reference(AliasState::EscapedReference);
            }
            if let Some(this_value) = call.this_value
                && let Err(message) =
                    initialize_this(compiled, state, function_id, function, this_value, stack)
            {
                let result = self.runtime_error(output, compiled, stack, message);
                stack.pop_recycle();
                return result;
            }
            if let Some(shared) = call.shared_top_level_locals.as_deref_mut() {
                import_shared_locals(
                    function,
                    stack,
                    state,
                    shared,
                    call.shared_top_level_bind_missing_globals,
                );
            } else if function.flags.is_top_level {
                bind_top_level_global_locals(function, stack, state);
                self.record_counter_alias_state(AliasState::GlobalOrSuperglobalReference);
            }
            let mut args = args;
            let bound_count = args.len().min(function.params.len());
            for arg_index in 0..bound_count {
                let param = &function.params[arg_index];
                let mut arg = std::mem::replace(
                    &mut args[arg_index],
                    PreparedArg {
                        value: Value::Uninitialized,
                        reference: None,
                        trace_holds_reference: false,
                    },
                );
                if let Err(message) = coerce_or_check_param_type(
                    ParamTypecheckRequest {
                        compiled,
                        state,
                        function,
                        param,
                        arg_index,
                        fast_path: self.typecheck_fast_path_context(),
                        strict_types: argument_policy.call_site_strict_types,
                        call_span: call.call_span,
                    },
                    &mut arg.value,
                ) {
                    // Cold: materialize the trace snapshot (bound params from
                    // locals, the failing and later ones from the raw args) so
                    // the TypeError's trace shows the real argument values —
                    // the lazy reconstruction has no bound local for them.
                    args[arg_index] = arg;
                    let entries: Vec<FrameTraceArgument> = (0..args.len())
                        .map(|index| {
                            let param = function.params.get(index);
                            let value = match param {
                                Some(param) if index < arg_index => stack
                                    .current()
                                    .and_then(|frame| frame.locals.get(param.local))
                                    .unwrap_or(Value::Null),
                                _ => args[index].value.clone(),
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
                        tag_throwable_location(&throwable, compiled, function.span);
                        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                        return self.propagate_exception(output, stack, state, throwable);
                    }
                    stack.pop_recycle();
                    return result;
                }
                let locals = &mut stack.current_mut().expect("frame was pushed").locals;
                let result = if param.by_ref {
                    if let Some(reference) = arg.reference {
                        reference.set(arg.value);
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::EscapedReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::EscapedReference,
                        );
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
                if param.by_ref {
                    self.record_counter_alias_state(local_alias_state(stack, param.local));
                }
            }
            block_id = BlockId::new(0);
            foreach_iterators = HashMap::new();
            exception_handlers = Vec::new();
            pending_control = None;
        }
        let frame_index = stack.len().saturating_sub(1);

        'dispatch: loop {
            if let Some(code) = state.process_exit_code {
                stack.pop_frame_recycle(frame_index);
                return script_exit_result(output, state, code);
            }
            steps += 1;
            match execution_limit_exceeded(state, steps, self.options.max_steps) {
                Some(ExecutionLimitExceeded::Timeout) => {
                    return self.execution_timeout(output, compiled, stack);
                }
                Some(ExecutionLimitExceeded::StepLimit) => {
                    return self.runtime_error(output, compiled, stack, "VM step limit exceeded");
                }
                None => {}
            }

            let Some(block) = function.blocks.get(block_id.index()) else {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!("invalid block block:{}", block_id.raw()),
                );
            };
            let instruction_start = start_instruction_index;
            start_instruction_index = 0;
            let mut batched_echo_skip_until = instruction_start;

            for (instruction_index, instruction) in block
                .instructions
                .iter()
                .enumerate()
                .skip(instruction_start)
            {
                if should_skip_top_level_auto_global_bind(function, instruction) {
                    continue;
                }
                if self.options.trace {
                    self.record_trace_event(
                        function_id,
                        function,
                        stack,
                        block_id,
                        instruction,
                        output.len(),
                    );
                }
                self.record_counter_instruction(&instruction.kind);
                self.observe_quickening(function_id, block_id, instruction.id, &instruction.kind);
                self.observe_inline_cache(
                    compiled_unit_cache_key(compiled),
                    function_id,
                    block_id,
                    instruction.id,
                    &instruction.kind,
                );
                if instruction_index < batched_echo_skip_until {
                    continue;
                }
                match &instruction.kind {
                    InstructionKind::Nop => {}
                    InstructionKind::LoadConst { dst, constant } => {
                        let value = match self.constant_value(unit, *constant) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    instruction_runtime_error_context(
                                        message,
                                        unit,
                                        function,
                                        block_id,
                                        instruction_index,
                                        instruction,
                                    ),
                                );
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::FetchConst { dst, name } => {
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
                                                instruction.span,
                                                constant,
                                            )
                                    {
                                        return *result;
                                    }
                                    resolved.value
                                }
                                Ok(None) => {
                                    return self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        format!(
                                            "E_PHP_RUNTIME_UNDEFINED_CONSTANT: undefined constant {name}"
                                        ),
                                    );
                                }
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            },
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::RegisterConstant { name, value } => {
                        let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    instruction_runtime_error_context(
                                        message,
                                        unit,
                                        function,
                                        block_id,
                                        instruction_index,
                                        instruction,
                                    ),
                                );
                            }
                        };
                        if compiled.lookup_constant(name).is_some()
                            || state.user_constants.contains_key(name)
                            || state
                                .dynamic_constants
                                .iter()
                                .any(|entry| entry.name == *name)
                        {
                            let source_span = current_instruction_diagnostic_span(
                                compiled,
                                state,
                                instruction.span,
                            );
                            if let Err(result) = self.emit_constant_already_defined_warning(
                                ExecutionCursor::new(compiled, output, stack, state),
                                source_span,
                                &mut diagnostics,
                                name,
                            ) {
                                return *result;
                            }
                            continue;
                        }
                        state
                            .user_constants
                            .insert(name.clone(), effective_value(&value));
                        state.bump_lookup_epoch();
                    }
                    InstructionKind::DeclareFunction { name, function } => {
                        if let Err(message) =
                            declare_runtime_function(compiled, state, name, *function)
                        {
                            return function_redeclaration_fatal_result(
                                output,
                                compiled,
                                stack,
                                instruction.span,
                                message,
                            );
                        }
                    }
                    InstructionKind::DeclareClass { name } => {
                        if let Err(message) = declare_runtime_class(compiled, state, name) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::Move { dst, src } => {
                        let value = match read_operand_at_frame(unit, stack, frame_index, *src) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::Binary { dst, op, lhs, rhs } => {
                        match execute_rich_binary_op(
                            self,
                            RichBinaryRequest {
                                compiled,
                                unit,
                                frame_index,
                                function_id,
                                block_id,
                                instruction_id: instruction.id,
                                dst: *dst,
                                op: *op,
                                lhs: *lhs,
                                rhs: *rhs,
                                span: instruction.span,
                            },
                            output,
                            stack,
                            state,
                        ) {
                            Ok(()) => {}
                            Err(RichBinaryError::Direct(result)) => return *result,
                            Err(RichBinaryError::Route(result)) => {
                                match self.route_throwable_result(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    *result,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                        }
                    }
                    InstructionKind::Compare { dst, op, lhs, rhs } => {
                        if let Err(message) = execute_rich_compare_op(
                            RichCompareRequest {
                                unit,
                                frame_index,
                                dst: *dst,
                                op: *op,
                                lhs: *lhs,
                                rhs: *rhs,
                            },
                            stack,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::InstanceOf {
                        dst,
                        object,
                        class_name,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = match self
                            .object_instanceof_cached(compiled, state, &object, class_name)
                        {
                            Ok(value) => Value::Bool(value),
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::DynamicInstanceOf {
                        dst,
                        object,
                        target,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let class_name = match read_operand_at_frame(
                            unit,
                            stack,
                            frame_index,
                            *target,
                        ) {
                            Ok(Value::String(value)) => {
                                normalize_class_name(&value.to_string_lossy())
                            }
                            Ok(Value::Object(object)) => normalize_class_name(&object.class_name()),
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_INVALID_DYNAMIC_CLASS_NAME: class name must be string or object, {} given",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = match self.object_instanceof_cached(
                            compiled,
                            state,
                            &object,
                            &class_name,
                        ) {
                            Ok(value) => Value::Bool(value),
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::Unary { dst, op, src } => {
                        match execute_rich_unary_op(
                            RichUnaryRequest {
                                unit,
                                frame_index,
                                dst: *dst,
                                op: *op,
                                src: *src,
                            },
                            stack,
                        ) {
                            Ok(None) => {}
                            Ok(Some(deprecation)) => {
                                if let Err(result) = self.emit_implicit_int_deprecation(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    deprecation,
                                    runtime_source_span(compiled, instruction.span),
                                ) {
                                    return *result;
                                }
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        }
                    }
                    InstructionKind::Cast { dst, kind, src } => {
                        let src = match read_operand_at_frame(unit, stack, frame_index, *src) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let source_span = runtime_source_span(compiled, instruction.span);
                        let value = match self.execute_cast(
                            *kind,
                            &src,
                            source_span,
                            ExecutionCursor::new(compiled, output, stack, state),
                        ) {
                            Ok(value) => value,
                            Err(result) => {
                                if let Some(throwable) = runtime_error_throwable(&result) {
                                    tag_throwable_location(&throwable, compiled, instruction.span);
                                    let is_caching_iterator_string_cast = matches!(
                                        effective_value(&src),
                                        Value::Object(object)
                                            if spl_runtime_marker(&object)
                                                .is_some_and(|class| is_spl_caching_iterator_class(&class))
                                    );
                                    if matches!(*kind, CastKind::String)
                                        && is_caching_iterator_string_cast
                                    {
                                        state.pending_trace = Some(
                                            capture_backtrace_string_with_builtin_failed_call(
                                                compiled,
                                                stack,
                                                "CachingIterator->__toString",
                                                &[],
                                                instruction.span,
                                            ),
                                        );
                                    } else {
                                        state.pending_trace =
                                            Some(capture_backtrace_string(compiled, stack));
                                    }
                                    state.pending_throw = Some(throwable);
                                }
                                match self.route_throwable_result(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    *result,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::Discard { src } => {
                        let value =
                            match take_discard_operand_at_frame(unit, stack, frame_index, *src) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        if let Some(value) = value {
                            if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                &value,
                            ) {
                                match outcome {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            release_unrooted_object_handles(&value);
                        }
                    }
                    InstructionKind::LoadLocal { dst, local } => {
                        let value = if is_globals_local(function, *local) {
                            self.record_counter_local_slot_fast_path(false);
                            Value::Array(state.globals.globals_array())
                        } else {
                            self.record_counter_local_slot_fast_path(
                                local_slot_is_in_bounds_at_frame(stack, frame_index, *local),
                            );
                            let local_value = stack
                                .frames()
                                .get(frame_index)
                                .expect("frame was pushed")
                                .locals
                                .get(*local);
                            match local_value {
                                Some(Value::Uninitialized) if is_this_local(function, *local) => {
                                    match self.raise_runtime_error(
                                        ExecutionCursor::new(compiled, output, stack, state),
                                        &mut exception_handlers,
                                        &mut pending_control,
                                        instruction.span,
                                        "E_PHP_VM_THIS_OUTSIDE_METHOD: Using $this when not in object context"
                                            .to_owned(),
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                                Some(Value::Uninitialized)
                                    if load_local_is_pre_call_by_ref_out_param(
                                        compiled,
                                        state,
                                        &block.instructions,
                                        instruction_index,
                                        *dst,
                                        *local,
                                    ) =>
                                {
                                    Value::Null
                                }
                                Some(Value::Uninitialized) => {
                                    let local_name = function
                                        .locals
                                        .get(local.index())
                                        .cloned()
                                        .unwrap_or_else(|| format!("local:{}", local.raw()));
                                    let diagnostic = if is_auto_global_name(&local_name) {
                                        undefined_global_variable_warning(
                                            local_name,
                                            runtime_source_span(compiled, instruction.span),
                                            stack_trace(compiled, stack),
                                        )
                                    } else {
                                        undefined_variable_warning(
                                            local_name,
                                            runtime_source_span(compiled, instruction.span),
                                            stack_trace(compiled, stack),
                                        )
                                    };
                                    let handled = match self.dispatch_error_handler(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        php_runtime::api::PHP_E_WARNING,
                                        &diagnostic,
                                    ) {
                                        Ok(handled) => handled,
                                        Err(result) => return *result,
                                    };
                                    if !handled
                                        && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING)
                                    {
                                        emit_vm_diagnostic(
                                            output,
                                            state,
                                            &diagnostic,
                                            php_runtime::api::PhpDiagnosticChannel::Warning,
                                            php_runtime::api::PHP_E_WARNING,
                                        );
                                        diagnostics.push(diagnostic);
                                    }
                                    Value::Null
                                }
                                Some(value) => value,
                                None => {
                                    return self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        format!("invalid local local:{}", local.raw()),
                                    );
                                }
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::LoadLocalQuiet { dst, local } => {
                        let value = if is_globals_local(function, *local) {
                            self.record_counter_local_slot_fast_path(false);
                            Value::Array(state.globals.globals_array())
                        } else {
                            self.record_counter_local_slot_fast_path(
                                local_slot_is_in_bounds_at_frame(stack, frame_index, *local),
                            );
                            match stack
                                .frames()
                                .get(frame_index)
                                .expect("frame was pushed")
                                .locals
                                .get(*local)
                            {
                                Some(Value::Uninitialized) => Value::Null,
                                Some(value) => value.clone(),
                                None => {
                                    return self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        format!("invalid local local:{}", local.raw()),
                                    );
                                }
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::StoreLocal { local, src } => {
                        let value = match read_operand_at_frame(unit, stack, frame_index, *src) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds_at_frame(
                            stack,
                            frame_index,
                            *local,
                        ));
                        let previous = stack
                            .frames()
                            .get(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .get(*local)
                            .unwrap_or(Value::Uninitialized);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .set(*local, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                            ExecutionCursor::new(compiled, output, stack, state),
                            &mut exception_handlers,
                            &mut pending_control,
                            &previous,
                        ) {
                            match outcome {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        release_unrooted_object_handles(&previous);
                    }
                    InstructionKind::BindReference { target, source } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::LocalOnlyReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::LocalOnlyReference,
                        );
                        self.record_counter_local_slot_fast_path(
                            local_slot_is_in_bounds_at_frame(stack, frame_index, *target)
                                && local_slot_is_in_bounds_at_frame(stack, frame_index, *source),
                        );
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .bind_reference(*target, *source)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::BindGlobal { local, name } => {
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
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .bind_reference_cell(*local, cell)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::BindReferenceDim {
                        local,
                        dims,
                        append,
                        source,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        self.record_counter_local_slot_fast_path(
                            local_slot_is_in_bounds_at_frame(stack, frame_index, *local)
                                && local_slot_is_in_bounds_at_frame(stack, frame_index, *source),
                        );
                        let cell = match stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .ensure_reference_cell(*source)
                        {
                            Ok(cell) => cell,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Some(object) =
                            spl_array_access_local_object_at_frame(stack, frame_index, *local)
                        {
                            emit_spl_array_access_bind_reference_notice(
                                compiled,
                                output,
                                stack,
                                state,
                                &object,
                                instruction.span,
                            );
                            let message =
                                "Cannot assign by reference to an array dimension of an object"
                                    .to_owned();
                            let diagnostic = RuntimeDiagnostic::new(
                                "E_PHP_VM_ARRAY_ACCESS_BIND_REFERENCE",
                                RuntimeSeverity::FatalError,
                                message.clone(),
                                runtime_source_span(compiled, instruction.span),
                                stack_trace(compiled, stack),
                                Some(php_runtime::api::PhpReferenceClassification::Error),
                            );
                            let result = VmResult::runtime_error_with_diagnostic(
                                output.clone(),
                                message,
                                diagnostic,
                            );
                            match self.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if let Err(message) =
                            bind_dim_local_to_reference_cell(stack, *local, &dims, *append, cell)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *local,
                        ));
                        self.record_lvalue_trace_event(
                            if *append {
                                "bind-reference-dim-append"
                            } else {
                                "bind-reference-dim"
                            },
                            *local,
                            &dims,
                        );
                    }
                    InstructionKind::BindReferencePropertyDim {
                        object,
                        property,
                        dims,
                        append,
                        source,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_REF_DIM_NON_OBJECT: cannot bind property dimension on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let cell = match stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .ensure_reference_cell(*source)
                        {
                            Ok(cell) => cell,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = bind_property_dim_to_reference_cell(
                            compiled,
                            state,
                            stack,
                            PropertyDimReferenceBinding {
                                object: &object,
                                property,
                                dims: &dims,
                                append: *append,
                                cell,
                            },
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::BindReferenceDimFromProperty {
                        local,
                        dims,
                        append,
                        object,
                        property,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_REF_DIM_SOURCE_NON_OBJECT: cannot reference property on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let cell = match ensure_property_reference_cell(
                            compiled,
                            Some(state),
                            stack,
                            &object,
                            property,
                        ) {
                            Ok(cell) => cell,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if dims.is_empty() && !append {
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .locals
                                .bind_reference_cell(*local, cell)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        } else {
                            if let Err(message) = bind_dim_local_to_reference_cell(
                                stack, *local, &dims, *append, cell,
                            ) {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        }
                    }
                    InstructionKind::BindReferenceFromProperty {
                        target,
                        object,
                        property,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_REF_SOURCE_NON_OBJECT: cannot reference property on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let cell = match ensure_property_reference_cell(
                            compiled,
                            Some(state),
                            stack,
                            &object,
                            property,
                        ) {
                            Ok(cell) => cell,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .bind_reference_cell(*target, cell)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *target,
                        ));
                    }
                    InstructionKind::BindReferenceFromPropertyDim {
                        target,
                        object,
                        property,
                        dims,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_REF_DIM_SOURCE_NON_OBJECT: cannot reference property dimension on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let cell = match ensure_property_dim_reference_cell(
                            compiled,
                            Some(state),
                            stack,
                            &object,
                            property,
                            &dims,
                        ) {
                            Ok(cell) => cell,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .bind_reference_cell(*target, cell)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *target,
                        ));
                    }
                    InstructionKind::BindReferenceFromDim {
                        target,
                        local,
                        dims,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let cell = match ensure_dim_reference_cell(stack, *local, &dims) {
                            Ok(cell) => cell,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .bind_reference_cell(*target, cell)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *target,
                        ));
                        self.record_lvalue_trace_event("bind-reference-from-dim", *local, &dims);
                    }
                    InstructionKind::BindReferenceFromStaticPropertyDim {
                        target,
                        class_name,
                        property,
                        dims,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        if let Err(result) = self.autoload_static_class_if_missing(
                            ExecutionCursor::new(compiled, output, stack, state),
                            class_name,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                        ) {
                            match self.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        let class =
                            match resolve_static_class_name(compiled, state, stack, class_name) {
                                Ok(class) => class,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        ExecutionCursor::new(compiled, output, stack, state),
                                        &mut exception_handlers,
                                        &mut pending_control,
                                        instruction.span,
                                        message,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                        let scope = current_scope_class(compiled, stack);
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                let message = format!(
                                    "E_PHP_VM_UNKNOWN_STATIC_PROPERTY: Access to undeclared static property {}::${property}",
                                    class.display_name
                                );
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if !resolved.property.flags.is_static {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is not static",
                                    resolved.class.name, resolved.property.name
                                ),
                            );
                        }
                        if let Err(message) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            &resolved.class,
                            &resolved.property,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        let key = static_property_key(&resolved.class, &resolved.property);
                        let current = if let Some(value) = state.static_properties.get(&key) {
                            value.clone()
                        } else {
                            match static_property_default(
                                compiled,
                                state,
                                stack,
                                &resolved.class,
                                &resolved.property,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                        };
                        if let Err(message) = validate_static_property_write(
                            compiled,
                            stack,
                            &resolved.class,
                            &resolved.property,
                            &current,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let cell = match ensure_static_property_dim_reference_cell(
                            &mut state.static_properties,
                            key,
                            current,
                            &dims,
                        ) {
                            Ok(cell) => cell,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .bind_reference_cell(*target, cell)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *target,
                        ));
                    }
                    InstructionKind::BindReferenceProperty {
                        object,
                        property,
                        source,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object) {
                            Ok(Value::Object(object)) => object,
                            Ok(Value::Callable(_)) => {
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    instruction.span,
                                    format!(
                                        "E_PHP_VM_DYNAMIC_PROPERTY_ERROR: Cannot create dynamic property Closure::${property}"
                                    ),
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_REFERENCE_NON_OBJECT: cannot assign property reference {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if is_std_class_runtime_class(&object.class_name()) {
                            let cell = match stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .locals
                                .ensure_reference_cell(*source)
                            {
                                Ok(cell) => cell,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            object.set_property(property, Value::Reference(cell));
                            self.record_counter_alias_state(local_alias_state_at_frame(
                                stack,
                                frame_index,
                                *source,
                            ));
                            continue;
                        }
                        let class =
                            match lookup_class_in_state(compiled, state, &object.class_name()) {
                                Some(class) => class,
                                None => {
                                    return self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        format!(
                                            "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
                                            object.class_name()
                                        ),
                                    );
                                }
                            };
                        let scope = current_scope_class(compiled, stack);
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                let cell = match stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .locals
                                    .ensure_reference_cell(*source)
                                {
                                    Ok(cell) => cell,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                                match self.call_magic_property_method(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    object.clone(),
                                    "__set",
                                    property,
                                    vec![
                                        CallArgument::positional(Value::String(
                                            PhpString::from_test_str(property),
                                        )),
                                        CallArgument::positional(Value::Reference(cell.clone())),
                                    ],
                                ) {
                                    Ok(Some(_)) => {
                                        self.record_counter_alias_state(local_alias_state(
                                            stack, *source,
                                        ));
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return *result,
                                }
                                if let Some(diagnostic) = dynamic_property_deprecation_diagnostic(
                                    compiled,
                                    state,
                                    &class,
                                    &object,
                                    property.as_ref(),
                                    stack,
                                ) {
                                    diagnostics.push(diagnostic);
                                }
                                object.set_property(property, Value::Reference(cell));
                                self.record_counter_alias_state(local_alias_state_at_frame(
                                    stack,
                                    frame_index,
                                    *source,
                                ));
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let entry = &resolved.property;
                        if entry.flags.is_static {
                            if let Err(message) = validate_property_access_in_state(
                                compiled,
                                state,
                                stack,
                                &resolved.class,
                                entry,
                            )
                            .and_then(|()| {
                                validate_property_set_access_in_state(
                                    compiled,
                                    state,
                                    stack,
                                    &resolved.class,
                                    entry,
                                )
                            }) {
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            let cell = match stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .locals
                                .ensure_reference_cell(*source)
                            {
                                Ok(cell) => cell,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            emit_static_property_as_non_static_notice(
                                compiled,
                                output,
                                stack,
                                state,
                                &resolved.class,
                                entry,
                                instruction.span,
                            );
                            object.set_property(property, Value::Reference(cell));
                            self.record_counter_alias_state(local_alias_state_at_frame(
                                stack,
                                frame_index,
                                *source,
                            ));
                            continue;
                        }
                        if let Err(message) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            &resolved.class,
                            entry,
                        )
                        .and_then(|()| {
                            validate_property_set_access_in_state(
                                compiled,
                                state,
                                stack,
                                &resolved.class,
                                entry,
                            )
                        }) {
                            match self.raise_runtime_error(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if entry.hooks.get.is_some() || entry.hooks.set.is_some() {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_PROPERTY_REFERENCE_HOOK: property {}::${} cannot be assigned by reference through hooks yet",
                                    resolved.class.name, entry.name
                                ),
                            );
                        }
                        let cell = match stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .ensure_reference_cell(*source)
                        {
                            Ok(cell) => cell,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let reference = Value::Reference(cell.clone());
                        let property_type = ir_runtime_type(entry.type_.as_ref());
                        if let Err(message) = check_property_type(
                            compiled,
                            Some(state),
                            resolved.class.display_name.as_str(),
                            property,
                            &property_type,
                            &reference,
                            self.typecheck_fast_path_context(),
                        ) {
                            match self.raise_runtime_error(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if let Err(message) = validate_property_write(
                            &resolved.class,
                            entry,
                            &object,
                            stack,
                            compiled,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        let storage_name = property_storage_name(&resolved.class, entry);
                        object.set_property(storage_name, reference);
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *source,
                        ));
                    }
                    InstructionKind::BindReferenceStaticProperty {
                        class_name,
                        property,
                        source,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        if let Err(result) = self.autoload_static_class_if_missing(
                            ExecutionCursor::new(compiled, output, stack, state),
                            class_name,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                        ) {
                            match self.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        let class =
                            match resolve_static_class_name(compiled, state, stack, class_name) {
                                Ok(class) => class,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        ExecutionCursor::new(compiled, output, stack, state),
                                        &mut exception_handlers,
                                        &mut pending_control,
                                        instruction.span,
                                        message,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                        let scope = current_scope_class(compiled, stack);
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                let message = format!(
                                    "E_PHP_VM_UNKNOWN_STATIC_PROPERTY: Access to undeclared static property {}::${property}",
                                    class.display_name
                                );
                                match self.raise_runtime_error(
                                    ExecutionCursor::new(compiled, output, stack, state),
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    instruction.span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let resolved_class = &resolved.class;
                        let resolved_property = &resolved.property;
                        if !resolved.property.flags.is_static {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is not static",
                                    resolved.class.name, resolved.property.name
                                ),
                            );
                        }
                        if let Err(message) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            resolved_class,
                            resolved_property,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        let cell = match stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .ensure_reference_cell(*source)
                        {
                            Ok(cell) => cell,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let reference = Value::Reference(cell.clone());
                        let property_type = ir_runtime_type(resolved.property.type_.as_ref());
                        if let Err(message) = check_property_type(
                            compiled,
                            Some(state),
                            resolved.class.display_name.as_str(),
                            resolved.property.name.as_str(),
                            &property_type,
                            &reference,
                            self.typecheck_fast_path_context(),
                        ) {
                            match self.raise_runtime_error(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                instruction.span,
                                message,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        let key = static_property_key(resolved_class, resolved_property);
                        let current = if let Some(value) = state.static_properties.get(&key) {
                            value.clone()
                        } else {
                            match static_property_default(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                resolved_property,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                        };
                        if let Err(message) = validate_static_property_write(
                            compiled,
                            stack,
                            resolved_class,
                            resolved_property,
                            &current,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        let previous_effective = effective_value(&current);
                        if let Err(message) = bind_static_property_lvalue(
                            &mut state.static_properties,
                            key,
                            current.clone(),
                            cell,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                            ExecutionCursor::new(compiled, output, stack, state),
                            &mut exception_handlers,
                            &mut pending_control,
                            &previous_effective,
                        ) {
                            match outcome {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *source,
                        ));
                    }
                    InstructionKind::InitStaticLocal {
                        local,
                        name,
                        default,
                    } => {
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
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *default) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let cell = ReferenceCell::new(value);
                            state.static_locals.insert(key, cell.clone());
                            cell
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .bind_reference_cell(*local, cell)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *local,
                        ));
                    }
                    InstructionKind::BindReferenceFromCall { target, name, args } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::EscapedReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::EscapedReference,
                        );
                        let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                            Ok(values) => values,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let Some(callee) = compiled.lookup_function(name) else {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_BY_REF_RETURN_NOT_CALLABLE: function {name} is not a user function"
                                ),
                            );
                        };
                        let result = self.execute_function(
                            compiled,
                            callee,
                            FunctionCall::new(values, Vec::new()),
                            output,
                            stack,
                            state,
                        );
                        if !result.status.is_success() {
                            return result;
                        }
                        diagnostics.extend(result.diagnostics);
                        let Some(reference) = result.return_ref else {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_BY_REF_RETURN_NOT_REFERENCEABLE: function {name} did not return a reference"
                                ),
                            );
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .locals
                            .bind_reference_cell(*target, reference)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *target,
                        ));
                    }
                    InstructionKind::BindReferenceFromMethodCall {
                        target,
                        object,
                        method,
                        args,
                    } => {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::EscapedReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::EscapedReference,
                        );
                        let receiver =
                            match read_operand_at_frame(unit, stack, frame_index, *object) {
                                Ok(receiver) => receiver,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                            Ok(values) => values,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let object = match receiver {
                            Value::Object(object) => object,
                            other => {
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
                        let result = self.call_object_method_callable(
                            ExecutionCursor::new(compiled, output, stack, state),
                            object,
                            method,
                            values,
                            Some(instruction.span),
                        );
                        if !result.status.is_success() {
                            return result;
                        }
                        diagnostics.extend(result.diagnostics);
                        let Some(reference) = result.return_ref else {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_BY_REF_RETURN_NOT_REFERENCEABLE: method {method} did not return a reference"
                                ),
                            );
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .locals
                            .bind_reference_cell(*target, reference)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        self.record_counter_alias_state(local_alias_state_at_frame(
                            stack,
                            frame_index,
                            *target,
                        ));
                    }
                    InstructionKind::EnterTry { .. }
                    | InstructionKind::LeaveTry
                    | InstructionKind::EndFinally { .. }
                    | InstructionKind::Throw { .. }
                    | InstructionKind::MakeException { .. } => {
                        match execute_rich_exception_instruction(
                            self,
                            ExecutionCursor::new(compiled, output, stack, state),
                            dispatch_contract::RichInstructionSite {
                                unit,
                                function,
                                function_id,
                                block_id,
                                instruction,
                                instruction_index,
                                frame_index,
                            },
                            &mut call.shared_top_level_locals,
                            &mut diagnostics,
                            dispatch_contract::RichControlState {
                                exception_handlers: &mut exception_handlers,
                                pending_control: &mut pending_control,
                            },
                        ) {
                            RichDispatchOutcome::Continue => {}
                            RichDispatchOutcome::Jump(target) => {
                                block_id = target;
                                continue 'dispatch;
                            }
                            RichDispatchOutcome::Return(result) => return *result,
                        }
                    }
                    InstructionKind::NewArray { dst } => {
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Array(PhpArray::new()))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::DynamicNewObject { .. }
                    | InstructionKind::NewObject { .. }
                    | InstructionKind::CloneObject { .. }
                    | InstructionKind::CloneWith { .. } => {
                        rich_object_dispatch::execute_rich_object_instruction!(
                            self,
                            compiled,
                            unit,
                            function_id,
                            block_id,
                            instruction,
                            instruction_index,
                            frame_index,
                            output,
                            stack,
                            state,
                            diagnostics,
                            foreach_iterators,
                            running_fiber,
                            exception_handlers,
                            pending_control,
                            'dispatch
                        );
                    }
                    InstructionKind::FetchProperty { .. }
                    | InstructionKind::FetchStaticProperty { .. }
                    | InstructionKind::FetchDynamicStaticProperty { .. }
                    | InstructionKind::IssetStaticProperty { .. }
                    | InstructionKind::EmptyStaticProperty { .. }
                    | InstructionKind::IssetStaticPropertyDim { .. }
                    | InstructionKind::EmptyStaticPropertyDim { .. }
                    | InstructionKind::FetchClassConstant { .. }
                    | InstructionKind::FetchDynamicProperty { .. }
                    | InstructionKind::IssetProperty { .. }
                    | InstructionKind::IssetDynamicProperty { .. }
                    | InstructionKind::EmptyProperty { .. }
                    | InstructionKind::EmptyDynamicProperty { .. }
                    | InstructionKind::IssetDynamicPropertyDim { .. }
                    | InstructionKind::EmptyDynamicPropertyDim { .. }
                    | InstructionKind::IssetPropertyDim { .. }
                    | InstructionKind::EmptyPropertyDim { .. }
                    | InstructionKind::UnsetProperty { .. }
                    | InstructionKind::UnsetPropertyDim { .. }
                    | InstructionKind::UnsetDynamicProperty { .. }
                    | InstructionKind::FetchObjectClassName { .. }
                    | InstructionKind::AssignProperty { .. }
                    | InstructionKind::AssignDynamicProperty { .. }
                    | InstructionKind::AssignPropertyDim { .. }
                    | InstructionKind::AssignStaticProperty { .. }
                    | InstructionKind::AssignDynamicStaticProperty { .. }
                    | InstructionKind::UnsetStaticPropertyDim { .. } => {
                        rich_property_dispatch::execute_rich_property_instruction!(
                            self,
                            compiled,
                            unit,
                            function_id,
                            block_id,
                            instruction,
                            frame_index,
                            output,
                            stack,
                            state,
                            diagnostics,
                            exception_handlers,
                            pending_control,
                            'dispatch
                        );
                    }
                    InstructionKind::ArrayInsert { .. }
                    | InstructionKind::ArraySpread { .. }
                    | InstructionKind::FetchDim { .. }
                    | InstructionKind::AssignDim { .. }
                    | InstructionKind::AppendDim { .. }
                    | InstructionKind::IssetDim { .. }
                    | InstructionKind::EmptyDim { .. }
                    | InstructionKind::UnsetDim { .. } => {
                        match rich_array_dispatch::execute_rich_array_instruction(
                            self,
                            ExecutionCursor::new(compiled, output, stack, state),
                            dispatch_contract::RichInstructionSite {
                                unit,
                                function,
                                function_id,
                                block_id,
                                instruction,
                                instruction_index,
                                frame_index,
                            },
                            &mut diagnostics,
                            &mut exception_handlers,
                            &mut pending_control,
                        ) {
                            RichDispatchOutcome::Continue => {}
                            RichDispatchOutcome::Jump(target) => {
                                block_id = target;
                                continue 'dispatch;
                            }
                            RichDispatchOutcome::Return(result) => return *result,
                        }
                    }
                    InstructionKind::IssetLocal { dst, local } => {
                        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(
                            stack, *local,
                        ));
                        let value = read_local_value_at_frame(stack, frame_index, *local)
                            .unwrap_or(Value::Uninitialized);
                        let result = !matches!(value, Value::Uninitialized | Value::Null);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Bool(result))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyLocal { dst, local } => {
                        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(
                            stack, *local,
                        ));
                        let value = read_local_value_at_frame(stack, frame_index, *local)
                            .unwrap_or(Value::Uninitialized);
                        let result = match php_empty(&value) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Bool(result))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::UnsetLocal { local } => {
                        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(
                            stack, *local,
                        ));
                        let previous = stack
                            .current()
                            .expect("frame was pushed")
                            .locals
                            .get(*local)
                            .unwrap_or(Value::Uninitialized);
                        if function.flags.is_top_level
                            && !is_globals_local(function, *local)
                            && let Some(Slot::Reference(cell)) = stack
                                .current()
                                .expect("frame was pushed")
                                .locals
                                .get_slot(*local)
                                .cloned()
                            && let Some(name) = function.locals.get(local.index())
                            && state
                                .globals
                                .get_slot(name)
                                .is_some_and(|global| global.ptr_eq(&cell))
                        {
                            cell.set(Value::Uninitialized);
                        }
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .locals
                            .unset(*local)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                            ExecutionCursor::new(compiled, output, stack, state),
                            &mut exception_handlers,
                            &mut pending_control,
                            &previous,
                        ) {
                            match outcome {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        release_unrooted_object_handles(&previous);
                    }
                    InstructionKind::ForeachInit { .. }
                    | InstructionKind::ForeachNext { .. }
                    | InstructionKind::ForeachCleanup { .. }
                    | InstructionKind::ForeachInitRef { .. }
                    | InstructionKind::ForeachNextRef { .. } => {
                        match execute_rich_foreach_instruction(
                            self,
                            ExecutionCursor::new(compiled, output, stack, state),
                            dispatch_contract::RichInstructionSite {
                                unit,
                                function,
                                function_id,
                                block_id,
                                instruction,
                                instruction_index,
                                frame_index,
                            },
                            &mut foreach_iterators,
                            dispatch_contract::RichControlState {
                                exception_handlers: &mut exception_handlers,
                                pending_control: &mut pending_control,
                            },
                        ) {
                            RichDispatchOutcome::Continue => {}
                            RichDispatchOutcome::Jump(target) => {
                                block_id = target;
                                continue 'dispatch;
                            }
                            RichDispatchOutcome::Return(result) => return *result,
                        }
                    }
                    InstructionKind::Echo { src } => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Output,
                            "echo",
                        );
                        let value = match read_operand_ref_at_frame(unit, stack, frame_index, *src)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if !self.options.trace
                            && let Some((parts, skip_until)) = collect_exact_echo_batch_at_frame(
                                self,
                                unit,
                                stack,
                                frame_index,
                                &block.instructions,
                                instruction_index,
                                value.as_value(),
                            )
                            && skip_until > instruction_index + 1
                        {
                            write_exact_echo_batch(output, &parts);
                            batched_echo_skip_until = skip_until;
                            continue;
                        }
                        {
                            let _source =
                                layout_source::enter(layout_source::OUTPUT_STRING_CONVERSION);
                            if Self::try_write_echo_fast(output, value.as_value()) {
                                continue;
                            }
                        }
                        let value = value.into_owned();
                        if let Err(result) = self.write_echo(compiled, output, stack, state, &value)
                        {
                            match self.route_throwable_result(
                                ExecutionCursor::new(compiled, output, stack, state),
                                &mut exception_handlers,
                                &mut pending_control,
                                *result,
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                    }
                    InstructionKind::EmitDiagnostic {
                        severity,
                        diagnostic_id,
                        message,
                        leading_newline,
                    } => {
                        let (runtime_severity, channel, level) = match severity {
                            IrDiagnosticSeverity::Warning => (
                                RuntimeSeverity::Warning,
                                php_runtime::api::PhpDiagnosticChannel::Warning,
                                php_runtime::api::PHP_E_WARNING,
                            ),
                            IrDiagnosticSeverity::Deprecation => (
                                RuntimeSeverity::Deprecation,
                                php_runtime::api::PhpDiagnosticChannel::Deprecated,
                                php_runtime::api::PHP_E_DEPRECATED,
                            ),
                        };
                        let diagnostic = RuntimeDiagnostic::new(
                            diagnostic_id.clone(),
                            runtime_severity,
                            message.clone(),
                            runtime_source_span(compiled, instruction.span),
                            stack_trace(compiled, stack),
                            None,
                        );
                        let handled = match self.dispatch_error_handler(
                            compiled,
                            output,
                            stack,
                            state,
                            level,
                            &diagnostic,
                        ) {
                            Ok(handled) => handled,
                            Err(result) => return *result,
                        };
                        if !handled && error_reporting_allows(state, level) {
                            emit_vm_diagnostic_with_options(
                                output,
                                state,
                                &diagnostic,
                                channel,
                                level,
                                *leading_newline,
                            );
                            diagnostics.push(diagnostic);
                        }
                    }
                    InstructionKind::Yield { dst, key, value } => {
                        let key = match key {
                            Some(key) => {
                                match read_operand_at_frame(unit, stack, frame_index, *key) {
                                    Ok(value) => Some(value),
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                }
                            }
                            None => None,
                        };
                        let value = match value {
                            Some(value) => {
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                }
                            }
                            None => Value::Null,
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Null)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Some(shared) = call.shared_top_level_locals.as_deref_mut() {
                            export_shared_locals_at_frame(function, stack, frame_index, shared);
                        }
                        let Some(frame) = stack.pop_frame(frame_index) else {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                "E_PHP_VM_GENERATOR_FRAME_MISSING: generator frame missing at yield",
                            );
                        };
                        if let Some(generator) = call.running_generator.as_ref() {
                            state.generator_continuations.insert(
                                generator.id(),
                                GeneratorContinuation {
                                    frame,
                                    block_id,
                                    instruction_index: instruction_index + 1,
                                    yield_result: *dst,
                                    foreach_iterators: foreach_iterators.clone(),
                                    exception_handlers: exception_handlers.clone(),
                                    pending_control: pending_control.clone(),
                                },
                            );
                        }
                        let mut result =
                            VmResult::success_with_diagnostics_no_output(None, diagnostics);
                        result.yielded = Some(Box::new(GeneratorYield { key, value }));
                        return result;
                    }
                    InstructionKind::YieldFrom { dst, source } => {
                        let Some(owner) = call.running_generator.as_ref() else {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                "E_PHP_VM_YIELD_FROM_OUTSIDE_GENERATOR: yield from executed outside a generator",
                            );
                        };
                        let delegation_key = YieldFromKey {
                            generator_id: owner.id(),
                            block_id,
                            instruction_index,
                        };
                        let step = match self.advance_yield_from_delegation(
                            compiled,
                            delegation_key.clone(),
                            *source,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(step) => step,
                            Err(result) => return *result,
                        };
                        match step {
                            YieldFromStep::Yield { key, value } => {
                                let Some(frame) = stack.pop_frame(frame_index) else {
                                    return self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        "E_PHP_VM_GENERATOR_FRAME_MISSING: generator frame missing at yield from",
                                    );
                                };
                                state.generator_continuations.insert(
                                    owner.id(),
                                    GeneratorContinuation {
                                        frame,
                                        block_id,
                                        instruction_index,
                                        yield_result: *dst,
                                        foreach_iterators: foreach_iterators.clone(),
                                        exception_handlers: exception_handlers.clone(),
                                        pending_control: pending_control.clone(),
                                    },
                                );
                                let mut result =
                                    VmResult::success_with_diagnostics_no_output(None, diagnostics);
                                result.yielded = Some(Box::new(GeneratorYield { key, value }));
                                return result;
                            }
                            YieldFromStep::Complete(return_value) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, return_value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                    }
                    InstructionKind::CallFunction { .. }
                    | InstructionKind::CallMethod { .. }
                    | InstructionKind::CallStaticMethod { .. }
                    | InstructionKind::MakeClosure { .. }
                    | InstructionKind::CallClosure { .. }
                    | InstructionKind::ResolveCallable { .. }
                    | InstructionKind::AcquireCallable { .. }
                    | InstructionKind::CallCallable { .. }
                    | InstructionKind::Pipe { .. } => {
                        match rich_call_dispatch::execute_rich_call_instruction(
                            self,
                            ExecutionCursor::new(compiled, output, stack, state),
                            dispatch_contract::RichInstructionSite {
                                unit,
                                function,
                                function_id,
                                block_id,
                                instruction,
                                instruction_index,
                                frame_index,
                            },
                            &running_fiber,
                            &mut diagnostics,
                            &mut foreach_iterators,
                            dispatch_contract::RichControlState {
                                exception_handlers: &mut exception_handlers,
                                pending_control: &mut pending_control,
                            },
                        ) {
                            RichDispatchOutcome::Continue => {}
                            RichDispatchOutcome::Jump(target) => {
                                block_id = target;
                                continue 'dispatch;
                            }
                            RichDispatchOutcome::Return(result) => return *result,
                        }
                    }
                    InstructionKind::Include { dst, kind, path } => {
                        let path = match read_operand_at_frame(unit, stack, frame_index, *path) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let result = self.execute_include(
                            ExecutionCursor::new(compiled, output, stack, state),
                            IncludeExecutionRequest {
                                site: UnitInlineCacheSite::new(
                                    None,
                                    compiled_unit_cache_key(compiled),
                                    function_id,
                                    block_id,
                                    instruction.id,
                                ),
                                instruction_span: instruction.span,
                                kind: *kind,
                                path: &path,
                            },
                        );
                        if !result.status.is_success() {
                            if include_failure_allows_continuation(*kind, &result) {
                                diagnostics.extend(result.diagnostics);
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, Value::Bool(false))
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            return result;
                        }
                        diagnostics.extend(result.diagnostics);
                        let return_value = result.return_value.unwrap_or(Value::Int(1));
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, return_value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::Eval { dst, code } => {
                        let code = match read_operand_at_frame(unit, stack, frame_index, *code) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let result = self.execute_eval(
                            compiled,
                            &code,
                            instruction.span,
                            output,
                            stack,
                            state,
                        );
                        if !result.status.is_success() {
                            return result;
                        }
                        diagnostics.extend(result.diagnostics);
                        let return_value = result.return_value.unwrap_or(Value::Null);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, return_value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::ArrayGet { dst, array, index } => {
                        let array = match read_operand_at_frame(unit, stack, frame_index, *array) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let index = match read_operand_at_frame(unit, stack, frame_index, *index) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = match packed_array_get(&array, &index) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::Unsupported { diagnostic_id } => {
                        let diagnostic = unsupported_feature(
                            diagnostic_id.clone(),
                            format!("unsupported IR instruction {diagnostic_id}"),
                            RuntimeSourceSpan::default(),
                            stack_trace(compiled, stack),
                        );
                        return VmResult {
                            status: ExecutionStatus::unsupported(diagnostic.message().to_owned()),
                            output: output.clone(),
                            diagnostics: vec![diagnostic],
                            return_value: None,
                            returned_explicitly: false,
                            process_exit_code: None,
                            process_exit_terminates_process: false,
                            yielded: None,
                            fiber_suspension: None,
                            return_ref: None,
                            trace: Vec::new(),
                            counters: None,
                            tiering_stats: None,
                            http_response: None,
                            upload_registry: None,
                            session: None,
                        };
                    }
                    InstructionKind::RuntimeError {
                        diagnostic_id,
                        message,
                    } => {
                        let result = self.runtime_error(
                            output,
                            compiled,
                            stack,
                            format!("{diagnostic_id}: {message}"),
                        );
                        if let Some(throwable) = runtime_error_throwable(&result) {
                            tag_throwable_location(&throwable, compiled, instruction.span);
                            state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                            return self.propagate_exception(output, stack, state, throwable);
                        }
                        return result;
                    }
                }
                if let Some(code) = state.process_exit_code {
                    stack.pop_frame_recycle(frame_index);
                    return script_exit_result(output, state, code);
                }
            }

            let Some(terminator) = &block.terminator else {
                return self.runtime_error(output, compiled, stack, "block has no terminator");
            };
            match &terminator.kind {
                TerminatorKind::Exit { value } => {
                    let value = match value {
                        Some(value) => {
                            match read_operand_at_frame(unit, stack, frame_index, *value) {
                                Ok(value) => Some(value),
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                        None => None,
                    };
                    let code = match self.resolve_exit_value(compiled, output, stack, state, value)
                    {
                        Ok(code) => code,
                        Err(result) => return *result,
                    };
                    state.process_exit_code = Some(code);
                    stack.pop_frame_recycle(frame_index);
                    return script_exit_result(output, state, code);
                }
                TerminatorKind::Return {
                    value,
                    by_ref_local,
                } => {
                    // A pending finally re-enters this frame's blocks and may
                    // still read registers; only then keep the cloning read.
                    let finally_pending = exception_handlers
                        .iter()
                        .any(|handler| handler.finally.is_some());
                    let value = match value {
                        Some(Operand::Register(id)) if !finally_pending => {
                            let taken = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .take(*id);
                            match taken {
                                Ok(value) if value.is_uninitialized() => {
                                    return self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        format!("read uninitialized register r{}", id.raw()),
                                    );
                                }
                                Ok(value) => Some(value),
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                        Some(value) => {
                            let _source = layout_source::enter(layout_source::RETURN_VALUE);
                            match read_operand_at_frame(unit, stack, frame_index, *value) {
                                Ok(value) => Some(value),
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                        None => None,
                    };
                    // A return unwinds through every enclosing try scope:
                    // catch-only handlers are discarded, the innermost
                    // pending finally runs first, and EndFinally resumes the
                    // unwind for outer finallys.
                    let mut resume_finally = None;
                    while let Some(handler) = exception_handlers.pop() {
                        if let Some(finally) = handler.finally {
                            resume_finally = Some(finally);
                            break;
                        }
                    }
                    if let Some(finally) = resume_finally {
                        pending_control = Some(PendingControl::Return(value));
                        block_id = finally;
                        continue 'dispatch;
                    }
                    let value = match coerce_return_value(
                        compiled,
                        state,
                        function,
                        value,
                        self.typecheck_fast_path_context(),
                    ) {
                        Ok(value) => value,
                        Err(message) => {
                            let result = self.runtime_error(output, compiled, stack, message);
                            if function.name.ends_with("::__toString")
                                && let Some(throwable) = runtime_error_throwable(&result)
                            {
                                state.pending_throw = Some(throwable);
                                stack.pop_frame_recycle(frame_index);
                                return VmResult::propagating_exception(output.clone());
                            }
                            return result;
                        }
                    };
                    let return_ref = if function.returns_by_ref {
                        self.record_counter_alias_state_transition(
                            AliasState::NoReferencesObserved,
                            AliasState::EscapedReference,
                        );
                        self.record_counter_fast_path_disabled_by_reference(
                            AliasState::EscapedReference,
                        );
                        let Some(local) = by_ref_local else {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_BY_REF_RETURN_TEMPORARY: function {} must return a variable by reference",
                                    function.name
                                ),
                            );
                        };
                        let frame = stack.frame_mut(frame_index).expect("frame is active");
                        let _source = layout_source::enter(
                            php_runtime::experimental::layout_stats::SOURCE_RETURN_REFERENCE_BINDING,
                        );
                        match frame.locals.ensure_reference_cell(*local) {
                            Ok(reference) => Some(reference),
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        }
                    } else {
                        None
                    };
                    if let Some(shared) = call.shared_top_level_locals.as_deref_mut() {
                        export_shared_locals_at_frame(function, stack, frame_index, shared);
                    }
                    let completed_scope = current_scope_class(compiled, stack);
                    stack.pop_frame_recycle(frame_index);
                    if !state.destructor_queue.is_empty()
                        && function_may_hold_destructor_sensitive_value(function)
                    {
                        let mut preserved_values = Vec::new();
                        if let Some(value) = &value {
                            preserved_values.push(value.clone());
                        }
                        if let Some(reference) = &return_ref {
                            preserved_values.push(Value::Reference(reference.clone()));
                        }
                        let preserved_object_ids =
                            preserved_destructor_object_ids(&preserved_values);
                        let mut destructor_handlers = Vec::new();
                        let mut destructor_pending_control = None;
                        let mut rooted_object_ids = php_visible_root_object_ids(stack, state);
                        rooted_object_ids.extend(preserved_object_ids);
                        let candidates = state.destructor_queue.objects_snapshot();
                        let sweep = self.run_destructors_for_unreferenced_candidates_with_roots(
                            ExecutionCursor::new(compiled, output, stack, state),
                            &mut destructor_handlers,
                            &mut destructor_pending_control,
                            candidates,
                            &rooted_object_ids,
                            completed_scope.as_deref(),
                        );
                        if let Some(outcome) = sweep.outcome {
                            match outcome {
                                RaiseOutcome::Caught(_) => {
                                    return self.runtime_error(
                                        output,
                                        compiled,
                                        stack,
                                        "E_PHP_VM_DESTRUCTOR_RETURN_CATCH: destructor during frame teardown cannot resume a completed frame",
                                    );
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                    }
                    let mut result =
                        VmResult::success_with_diagnostics_no_output(value, diagnostics);
                    result.return_ref = return_ref;
                    result.returned_explicitly = !is_synthetic_eof_return(
                        function,
                        terminator.span,
                        result.return_value.as_ref(),
                    );
                    return result;
                }
                TerminatorKind::Jump { target } => {
                    self.record_tiering_backedge(compiled, function_id, block_id, *target);
                    block_id = *target;
                }
                TerminatorKind::JumpIfFalse { condition, target } => {
                    let truthy = match operand_truthy_at_frame(unit, stack, frame_index, *condition)
                    {
                        Ok(value) => value,
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    };
                    if truthy {
                        let next = match next_block_id(function, block_id) {
                            Ok(block_id) => block_id,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        self.record_tiering_backedge(compiled, function_id, block_id, next);
                        block_id = next;
                    } else {
                        self.record_tiering_backedge(compiled, function_id, block_id, *target);
                        block_id = *target;
                    }
                }
                TerminatorKind::JumpIfTrue { condition, target } => {
                    let truthy = match operand_truthy_at_frame(unit, stack, frame_index, *condition)
                    {
                        Ok(value) => value,
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    };
                    if truthy {
                        self.record_tiering_backedge(compiled, function_id, block_id, *target);
                        block_id = *target;
                    } else {
                        let next = match next_block_id(function, block_id) {
                            Ok(block_id) => block_id,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        self.record_tiering_backedge(compiled, function_id, block_id, next);
                        block_id = next;
                    }
                }
                TerminatorKind::JumpIf {
                    condition,
                    if_true,
                    if_false,
                } => {
                    let truthy = match operand_truthy_at_frame(unit, stack, frame_index, *condition)
                    {
                        Ok(value) => value,
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    };
                    let next = if truthy { *if_true } else { *if_false };
                    self.record_tiering_backedge(compiled, function_id, block_id, next);
                    block_id = next;
                }
            }
        }
    }
}
