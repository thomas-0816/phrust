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
            call.positional_values.is_empty(),
            "pre-bound positional values reached the rich call path"
        );
        let unit = compiled.unit();
        let Some(function) = unit.functions.get(function_id.index()) else {
            return self.runtime_error(output, compiled, stack, "called function is missing");
        };
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
            compiled,
            function_id,
            function,
            &call,
            output,
            stack,
            state,
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
                    php_runtime::PHP_E_WARNING,
                    &diagnostic,
                ) {
                    Ok(handled) => handled,
                    Err(result) => return result,
                };
                if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
                    emit_vm_diagnostic(
                        output,
                        state,
                        &diagnostic,
                        php_runtime::PhpDiagnosticChannel::Warning,
                        php_runtime::PHP_E_WARNING,
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
            if let Some(value) = self.try_execute_jit_leaf(
                compiled,
                state,
                function_id,
                function,
                function_tier,
                jit_call_shape_supported,
                &args,
            ) {
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
                    compiled,
                    state,
                    function,
                    param,
                    arg_index,
                    &mut arg.value,
                    arg.reference.is_some(),
                    self.typecheck_fast_path_context(),
                    argument_policy.call_site_strict_types,
                    call.call_span,
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
                                                compiled,
                                                output,
                                                stack,
                                                state,
                                                &mut diagnostics,
                                                instruction.span,
                                                constant,
                                            )
                                    {
                                        return result;
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
                                compiled,
                                source_span,
                                output,
                                stack,
                                state,
                                &mut diagnostics,
                                name,
                            ) {
                                return result;
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
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                        if let Err(message) = execute_rich_unary_op(
                            RichUnaryRequest {
                                unit,
                                frame_index,
                                dst: *dst,
                                op: *op,
                                src: *src,
                            },
                            stack,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
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
                            compiled,
                            *kind,
                            &src,
                            source_span,
                            output,
                            stack,
                            state,
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
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                                compiled,
                                output,
                                stack,
                                state,
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
                            release_unrooted_object_handles(&value, stack, state);
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
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                        php_runtime::PHP_E_WARNING,
                                        &diagnostic,
                                    ) {
                                        Ok(handled) => handled,
                                        Err(result) => return result,
                                    };
                                    if !handled
                                        && error_reporting_allows(state, php_runtime::PHP_E_WARNING)
                                    {
                                        emit_vm_diagnostic(
                                            output,
                                            state,
                                            &diagnostic,
                                            php_runtime::PhpDiagnosticChannel::Warning,
                                            php_runtime::PHP_E_WARNING,
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
                            compiled,
                            output,
                            stack,
                            state,
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
                        release_unrooted_object_handles(&previous, stack, state);
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
                                Some(php_runtime::PhpReferenceClassification::Error),
                            );
                            let result = VmResult::runtime_error_with_diagnostic(
                                output.clone(),
                                message,
                                diagnostic,
                            );
                            match self.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
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
                            compiled, state, stack, &object, property, &dims, *append, cell,
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
                            compiled,
                            class_name,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                            stack,
                            state,
                        ) {
                            match self.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let class =
                            match resolve_static_class_name(compiled, state, stack, class_name) {
                                Ok(class) => class,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                                    compiled,
                                    object.clone(),
                                    "__set",
                                    property,
                                    vec![
                                        CallArgument::positional(Value::String(
                                            PhpString::from_test_str(property),
                                        )),
                                        CallArgument::positional(Value::Reference(cell.clone())),
                                    ],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(_)) => {
                                        self.record_counter_alias_state(local_alias_state(
                                            stack, *source,
                                        ));
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return result,
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
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                                compiled,
                                output,
                                stack,
                                state,
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
                                compiled,
                                output,
                                stack,
                                state,
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
                            compiled,
                            class_name,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                            stack,
                            state,
                        ) {
                            match self.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let class =
                            match resolve_static_class_name(compiled, state, stack, class_name) {
                                Ok(class) => class,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                                compiled,
                                output,
                                stack,
                                state,
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
                            compiled,
                            output,
                            stack,
                            state,
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
                            compiled,
                            object,
                            method,
                            values,
                            Some(instruction.span),
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
                    kind @ (InstructionKind::EnterTry { .. }
                    | InstructionKind::LeaveTry
                    | InstructionKind::EndFinally { .. }
                    | InstructionKind::Throw { .. }
                    | InstructionKind::MakeException { .. }) => {
                        match execute_rich_exception_instruction(
                            self,
                            compiled,
                            unit,
                            function,
                            frame_index,
                            kind,
                            instruction.span,
                            &mut call.shared_top_level_locals,
                            &mut diagnostics,
                            output,
                            stack,
                            state,
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
                    InstructionKind::DynamicNewObject {
                        dst,
                        class_name,
                        args,
                    } => {
                        let (class_name, display_class_name) = match read_operand(
                            unit,
                            stack,
                            *class_name,
                        ) {
                            Ok(Value::String(value)) => {
                                let display = display_class_name(&value.to_string_lossy());
                                (normalize_class_name(&display), display)
                            }
                            Ok(Value::Object(object)) => (
                                normalize_class_name(&object.class_name()),
                                object.display_name(),
                            ),
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
                        let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                            Ok(values) => values,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if is_instantiable_internal_throwable(&class_name) {
                            let object = match new_internal_throwable_object(
                                compiled,
                                stack,
                                &class_name,
                                &values,
                                instruction.span,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(&class_name) {
                            if !values.is_empty() {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_TOO_MANY_ARGS: constructor for class {class_name} does not accept arguments"
                                    ),
                                );
                            }
                            let object =
                                ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_php_token_runtime_class(&class_name) {
                            let object = match new_php_token_object(values) {
                                Ok(object) => object,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_fileinfo_runtime_class(&class_name) {
                            let object = match new_fileinfo_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_date_time_runtime_class(&class_name) {
                            let object = match new_date_time_object(
                                &class_name,
                                values,
                                &state.default_timezone,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_sqlite_runtime_class(&class_name) {
                            let object = match new_sqlite_object(
                                &class_name,
                                values,
                                &mut state.builtins.sqlite,
                                &self.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_mysqli_runtime_class(&class_name) {
                            let object = match new_mysqli_object(
                                &class_name,
                                values,
                                &mut state.builtins.mysql,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_redis_runtime_class(&class_name) {
                            let object = match new_redis_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_memcached_runtime_class(&class_name) {
                            let object = match new_memcached_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_soap_runtime_class(&class_name) {
                            let object = match new_soap_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_imagick_runtime_class(&class_name) {
                            let object = match new_imagick_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xsl_runtime_class(&class_name) {
                            let object = match new_xsl_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_pdo_runtime_class(&class_name) {
                            let object = match new_pdo_object(
                                &class_name,
                                values,
                                &mut state.builtins.sqlite,
                                &mut state.builtins.mysql,
                                &mut state.builtins.postgres,
                                &self.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_phar_runtime_class(&class_name) {
                            let object = match new_phar_object(
                                &class_name,
                                values,
                                &self.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_zip_runtime_class(&class_name) {
                            let object = match new_zip_object(
                                &class_name,
                                values,
                                &self.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xml_runtime_class(&class_name) {
                            let object = match new_xml_runtime_object(&class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        let class = if let Some(class) =
                            self.cached_class_entry(compiled, state, &class_name)
                        {
                            class
                        } else {
                            match self.class_like_exists_with_autoload_cache(
                                compiled,
                                &display_class_name,
                                AutoloadClassLookupKind::Class,
                                true,
                                Some((
                                    compiled_unit_cache_key(compiled),
                                    function_id,
                                    block_id,
                                    instruction.id,
                                )),
                                output,
                                stack,
                                state,
                            ) {
                                Ok(_) => {}
                                Err(result) => return result,
                            }
                            let Some(class) = self.cached_class_entry(compiled, state, &class_name)
                            else {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    instruction.span,
                                    format!(
                                        "E_PHP_VM_UNKNOWN_CLASS: Class \"{display_class_name}\" not found"
                                    ),
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            };
                            class
                        };
                        if let Err(result) = self.autoload_class_parents_if_missing(
                            compiled, &class, output, stack, state,
                        ) {
                            return result;
                        }
                        let class_owner = class_owner_in_state(compiled, state, &class.name);
                        let runtime_class =
                            match self.cached_runtime_class_entry(&class_owner, state, &class) {
                                Ok(class) => class,
                                Err(error) => {
                                    match self.raise_runtime_class_entry_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut exception_handlers,
                                        &mut pending_control,
                                        instruction.span,
                                        error,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                        if let Err(message) = validate_object_mvp(&runtime_class) {
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let spl_runtime_parent =
                            spl_runtime_parent_for_class(compiled, state, &class);
                        let slot_template = self.cached_default_slot_template(
                            state,
                            &runtime_class,
                            &display_class_name,
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
                        let constructor = match self.cached_constructor_resolution(
                            compiled,
                            state,
                            &class.name,
                            caller_scope.as_deref(),
                        ) {
                            Ok(constructor) => constructor,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Some(constructor) = constructor {
                            if let Err(message) = validate_constructor_callable_in_state_scope(
                                compiled,
                                state,
                                caller_scope.as_deref(),
                                &constructor.class,
                                &constructor.method,
                            ) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            let class_owner =
                                dynamic_class_owner_in_state(state, &constructor.class.name)
                                    .unwrap_or_else(|| compiled.clone());
                            let result = self.execute_function(
                                &class_owner,
                                constructor.method.function,
                                FunctionCall::new(values, Vec::new())
                                    .with_call_site_strict_types(call_site_strictness(
                                        compiled,
                                        Some(instruction.span),
                                    ))
                                    .with_call_span(instruction.span)
                                    .with_this(object.clone())
                                    .with_class_context_handles(
                                        self.class_name_handles(&constructor.class.name).normalized,
                                        object_called_class_handle(&object),
                                        self.class_name_handles(&constructor.class.name).normalized,
                                    )
                                    .inherit_fiber_context(&running_fiber),
                                output,
                                stack,
                                state,
                            );
                            if !result.status.is_success() {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            if result.fiber_suspension.is_some() {
                                return self.propagate_fiber_suspension(
                                    result,
                                    compiled,
                                    *dst,
                                    block_id,
                                    instruction_index + 1,
                                    &foreach_iterators,
                                    &exception_handlers,
                                    &pending_control,
                                    output,
                                    stack,
                                );
                            }
                            diagnostics.extend(result.diagnostics);
                        } else if let Some(spl_class) = spl_runtime_parent {
                            let init_values = if is_spl_iterator_runtime_class(&spl_class) {
                                match self.prepare_spl_iterator_constructor_args(
                                    compiled, &spl_class, values, output, stack, state,
                                ) {
                                    Ok(values) => values,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                }
                            } else {
                                values
                            };
                            if let Err(message) = initialize_spl_runtime_subclass_storage(
                                &object,
                                &spl_class,
                                init_values,
                                &self.options.runtime_context,
                                Some(&mut state.resources),
                            ) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                        }
                        self.register_destructor_if_needed(compiled, &class, object.clone(), state);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, Value::Object(object))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = unset_indirect_temporary_call_arg_registers_at_frame(
                            stack,
                            frame_index,
                            args,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::NewObject {
                        dst,
                        display_class_name,
                        class_name,
                        args,
                    } => {
                        let (resolved_class_name, resolved_display_class_name) =
                            if is_special_static_class_name(class_name) {
                                match resolve_static_class_name(compiled, state, stack, class_name)
                                {
                                    Ok(class) => (class.name.clone(), class.display_name.clone()),
                                    Err(message) => {
                                        match self.raise_runtime_error(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                }
                            } else {
                                (class_name.clone(), display_class_name.clone())
                            };
                        let class_name = resolved_class_name.as_str();
                        let display_class_name = resolved_display_class_name.as_str();
                        if is_closure_runtime_class(class_name) {
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
                                &mut exception_handlers,
                                &mut pending_control,
                                instruction.span,
                                "E_PHP_VM_CLOSURE_INSTANTIATION: Instantiation of class Closure is not allowed"
                                    .to_owned(),
                            ) {
                                RaiseOutcome::Caught(target) => {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                RaiseOutcome::Done(result) => return *result,
                            }
                        }
                        if is_fiber_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let fiber = match new_fiber_object(values) {
                                Ok(fiber) => fiber,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Fiber(fiber))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_reflection_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let values =
                                values.into_iter().map(|arg| arg.value).collect::<Vec<_>>();
                            if let Err(result) = self.preflight_reflection_constructor(
                                compiled, class_name, &values, output, stack, state,
                            ) {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            let object = match self.reflection_new_object(
                                compiled, class_name, values, output, stack, state,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_spl_iterator_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let values = match self.prepare_spl_iterator_constructor_args(
                                compiled, class_name, values, output, stack, state,
                            ) {
                                Ok(values) => values,
                                Err(result) => {
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            };
                            let object = match new_spl_iterator_object(
                                class_name,
                                values,
                                &self.options.runtime_context,
                                Some(&mut state.resources),
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_spl_container_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_spl_container_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_spl_heap_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_spl_heap_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_spl_file_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_spl_file_object(
                                class_name,
                                values,
                                &self.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            if !values.is_empty() {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_TOO_MANY_ARGS: constructor for class {class_name} does not accept arguments"
                                    ),
                                );
                            }
                            let object =
                                ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_php_token_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_php_token_object(values) {
                                Ok(object) => object,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_fileinfo_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_fileinfo_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_date_time_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_date_time_object(
                                class_name,
                                values,
                                &state.default_timezone,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_sqlite_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_sqlite_object(
                                class_name,
                                values,
                                &mut state.builtins.sqlite,
                                &self.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_pdo_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_pdo_object(
                                class_name,
                                values,
                                &mut state.builtins.sqlite,
                                &mut state.builtins.mysql,
                                &mut state.builtins.postgres,
                                &self.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_mysqli_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_mysqli_object(
                                class_name,
                                values,
                                &mut state.builtins.mysql,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_redis_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_redis_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_memcached_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_memcached_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_soap_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_soap_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_imagick_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_imagick_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xsl_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_xsl_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_phar_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_phar_object(
                                class_name,
                                values,
                                &self.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_zip_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_zip_object(
                                class_name,
                                values,
                                &self.options.runtime_context,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xml_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_xml_runtime_object(class_name, values) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_instantiable_internal_throwable(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let object = match new_internal_throwable_object(
                                compiled,
                                stack,
                                class_name,
                                &values,
                                instruction.span,
                            ) {
                                Ok(object) => object,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, Value::Object(object))
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        let class = match self.cached_class_entry(compiled, state, class_name) {
                            Some(class) => class,
                            None => {
                                if is_spl_iterator_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let values = match self.prepare_spl_iterator_constructor_args(
                                        compiled, class_name, values, output, stack, state,
                                    ) {
                                        Ok(values) => values,
                                        Err(result) => {
                                            match self.route_throwable_result(
                                                compiled,
                                                output,
                                                stack,
                                                state,
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
                                    };
                                    let object = match new_spl_iterator_object(
                                        class_name,
                                        values,
                                        &self.options.runtime_context,
                                        Some(&mut state.resources),
                                    ) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            match self.raise_runtime_error(
                                                compiled,
                                                output,
                                                stack,
                                                state,
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
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_spl_container_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let object = match new_spl_container_object(class_name, values)
                                    {
                                        Ok(object) => object,
                                        Err(message) => {
                                            let result = self
                                                .runtime_error(output, compiled, stack, message);
                                            match self.route_throwable_result(
                                                compiled,
                                                output,
                                                stack,
                                                state,
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
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_spl_heap_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let object = match new_spl_heap_object(class_name, values) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            let result = self
                                                .runtime_error(output, compiled, stack, message);
                                            match self.route_throwable_result(
                                                compiled,
                                                output,
                                                stack,
                                                state,
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
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_spl_file_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let object = match new_spl_file_object(
                                        class_name,
                                        values,
                                        &self.options.runtime_context,
                                    ) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            let result = self
                                                .runtime_error(output, compiled, stack, message);
                                            match self.route_throwable_result(
                                                compiled,
                                                output,
                                                stack,
                                                state,
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
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_std_class_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    if !values.is_empty() {
                                        return self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            format!(
                                                "E_PHP_VM_TOO_MANY_ARGS: constructor for class {class_name} does not accept arguments"
                                            ),
                                        );
                                    }
                                    let object = ObjectRef::new_with_display_name(
                                        &std_class_entry(),
                                        "stdClass",
                                    );
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if is_date_time_runtime_class(class_name) {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    let object = match new_date_time_object(
                                        class_name,
                                        values,
                                        &state.default_timezone,
                                    ) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if class_name
                                    .trim_start_matches('\\')
                                    .to_ascii_lowercase()
                                    .starts_with("reflection")
                                {
                                    let values = match read_call_args_at_frame(
                                        unit,
                                        stack,
                                        frame_index,
                                        args,
                                    ) {
                                        Ok(values) => values
                                            .into_iter()
                                            .map(|arg| arg.value)
                                            .collect::<Vec<_>>(),
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    if let Err(result) = self.preflight_reflection_constructor(
                                        compiled, class_name, &values, output, stack, state,
                                    ) {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                    let object = match self.reflection_new_object(
                                        compiled, class_name, values, output, stack, state,
                                    ) {
                                        Ok(object) => object,
                                        Err(message) => {
                                            let result = self
                                                .runtime_error(output, compiled, stack, message);
                                            match self.route_throwable_result(
                                                compiled,
                                                output,
                                                stack,
                                                state,
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
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, Value::Object(object))
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                match self.class_like_exists_with_autoload_cache(
                                    compiled,
                                    display_class_name,
                                    AutoloadClassLookupKind::Class,
                                    true,
                                    Some((
                                        compiled_unit_cache_key(compiled),
                                        function_id,
                                        block_id,
                                        instruction.id,
                                    )),
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(_) => {}
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                }
                                if let Some(class) =
                                    self.cached_class_entry(compiled, state, class_name)
                                {
                                    class
                                } else {
                                    return self.runtime_error_with_bringup_context(
                                        output,
                                        compiled,
                                        stack,
                                        state,
                                        runtime_source_span(compiled, instruction.span),
                                        format!(
                                            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
                                        ),
                                        BringupDiagnosticInput {
                                            error_class: Some("autoload_lookup"),
                                            requested_name: Some(class_name.to_owned()),
                                            lookup_kind: Some("class"),
                                            autoload_enabled: Some(true),
                                            ..BringupDiagnosticInput::default()
                                        },
                                    );
                                }
                            }
                        };
                        if let Err(result) = self.autoload_class_parents_if_missing(
                            compiled, &class, output, stack, state,
                        ) {
                            match self.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let class_owner = class_owner_in_state(compiled, state, &class.name);
                        let runtime_class =
                            match self.cached_runtime_class_entry(&class_owner, state, &class) {
                                Ok(class) => class,
                                Err(error) => {
                                    match self.raise_runtime_class_entry_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut exception_handlers,
                                        &mut pending_control,
                                        instruction.span,
                                        error,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            };
                        if let Err(message) = validate_object_mvp(&runtime_class) {
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                            Ok(values) => values,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let spl_runtime_parent =
                            spl_runtime_parent_for_class(compiled, state, &class);
                        let slot_template = self.cached_default_slot_template(
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
                        let constructor = match self.cached_constructor_resolution(
                            compiled,
                            state,
                            &class.name,
                            caller_scope.as_deref(),
                        ) {
                            Ok(constructor) => constructor,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Some(constructor) = constructor {
                            if let Err(message) = validate_constructor_callable_in_state_scope(
                                compiled,
                                state,
                                caller_scope.as_deref(),
                                &constructor.class,
                                &constructor.method,
                            ) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            let class_owner =
                                dynamic_class_owner_in_state(state, &constructor.class.name)
                                    .unwrap_or_else(|| compiled.clone());
                            let result = self.execute_function(
                                &class_owner,
                                constructor.method.function,
                                FunctionCall::new(values, Vec::new())
                                    .with_call_site_strict_types(call_site_strictness(
                                        compiled,
                                        Some(instruction.span),
                                    ))
                                    .with_call_span(instruction.span)
                                    .with_this(object.clone())
                                    .with_class_context_handles(
                                        self.class_name_handles(&constructor.class.name).normalized,
                                        object_called_class_handle(&object),
                                        self.class_name_handles(&constructor.class.name).normalized,
                                    )
                                    .inherit_fiber_context(&running_fiber),
                                output,
                                stack,
                                state,
                            );
                            if !result.status.is_success() {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            if result.fiber_suspension.is_some() {
                                return self.propagate_fiber_suspension(
                                    result,
                                    compiled,
                                    *dst,
                                    block_id,
                                    instruction_index + 1,
                                    &foreach_iterators,
                                    &exception_handlers,
                                    &pending_control,
                                    output,
                                    stack,
                                );
                            }
                            diagnostics.extend(result.diagnostics);
                        } else if let Some(spl_class) = spl_runtime_parent {
                            let init_values = if is_spl_iterator_runtime_class(&spl_class) {
                                match self.prepare_spl_iterator_constructor_args(
                                    compiled, &spl_class, values, output, stack, state,
                                ) {
                                    Ok(values) => values,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                }
                            } else {
                                values
                            };
                            if let Err(message) = initialize_spl_runtime_subclass_storage(
                                &object,
                                &spl_class,
                                init_values,
                                &self.options.runtime_context,
                                Some(&mut state.resources),
                            ) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                        }
                        self.register_destructor_if_needed(compiled, &class, object.clone(), state);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, Value::Object(object))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = unset_indirect_temporary_call_arg_registers_at_frame(
                            stack,
                            frame_index,
                            args,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::CloneObject { dst, object } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        match self.clone_object_value(
                            compiled,
                            &object,
                            instruction.span,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(value) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                            Err(ClassConstantFetch::Throwable(result)) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            Err(ClassConstantFetch::Raise(span, message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(ClassConstantFetch::Fatal(message)) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        }
                    }
                    InstructionKind::CloneWith {
                        dst,
                        object,
                        replacements,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_CLONE_NON_OBJECT: cannot clone {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let replacements = match read_operand_at_frame(
                            unit,
                            stack,
                            frame_index,
                            *replacements,
                        ) {
                            Ok(Value::Array(replacements)) => replacements,
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_CLONE_WITH_REPLACEMENTS: clone-with replacements must be array, got {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
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
                        let runtime_class = match runtime_class_entry(
                            compiled,
                            state,
                            &class,
                            &|value| self.constant_value(compiled.unit(), value),
                            &|reference| class_constant_reference_value(compiled, state, reference),
                            &|reference| named_constant_reference_value(compiled, state, reference),
                        ) {
                            Ok(class) => class,
                            Err(error) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    error.into_message(),
                                );
                            }
                        };
                        if let Err(message) = validate_object_mvp(&runtime_class) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        let copy = match self.clone_object_with_magic(
                            compiled,
                            object.clone(),
                            &class,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(copy) => copy,
                            Err(result) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                        };
                        self.register_destructor_if_needed(compiled, &class, copy.clone(), state);
                        for (key, value) in replacements.iter() {
                            let property = match clone_with_property_name(&key) {
                                Ok(property) => property,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            let Some(ir_property) =
                                class.properties.iter().find(|entry| entry.name == property)
                            else {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_UNKNOWN_PROPERTY: property {}::${property} is not declared",
                                        object.class_name()
                                    ),
                                );
                            };
                            if let Err(message) =
                                validate_property_access(compiled, stack, &class, ir_property)
                            {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            if let Err(message) =
                                validate_property_set_access(compiled, stack, &class, ir_property)
                            {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            if ir_property.flags.is_readonly || class.flags.is_readonly {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    instruction.span,
                                    format!(
                                        "E_PHP_VM_READONLY_PROPERTY_WRITE: Cannot modify protected(set) readonly property {}::${property} from global scope",
                                        class.display_name
                                    ),
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            if ir_property.flags.is_static {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    instruction.span,
                                    format!(
                                        "E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER: property {}::${property} uses modifiers outside the reflection-clone clone-with MVP",
                                        class.display_name
                                    ),
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            let storage_name = property_storage_name(&class, ir_property);
                            let Some(entry) = runtime_class
                                .properties
                                .iter()
                                .find(|entry| entry.name == storage_name)
                            else {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_UNKNOWN_PROPERTY: property {}::${property} is not declared",
                                        object.class_name()
                                    ),
                                );
                            };
                            if let Err(message) = check_property_type(
                                compiled,
                                Some(state),
                                class.display_name.as_str(),
                                &property,
                                &entry.type_,
                                value,
                                self.typecheck_fast_path_context(),
                            ) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            if let Some(function_id) = entry.hooks.set_function_id {
                                match self.call_property_hook(
                                    compiled,
                                    copy.clone(),
                                    &class,
                                    ir_property,
                                    FunctionId::new(function_id),
                                    vec![CallArgument::positional(value.clone())],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(_) => continue,
                                    Err(result) => return result,
                                }
                            }
                            if !entry.hooks.backed
                                && (entry.hooks.get_function_id.is_some()
                                    || entry.hooks.set_function_id.is_some())
                            {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_VIRTUAL_PROPERTY_WRITE: property {}::${property} has no backing storage",
                                        object.class_name()
                                    ),
                                );
                            }
                            copy.set_property(property, value.clone());
                        }
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, Value::Object(copy))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::FetchProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Object,
                            "property_fetch",
                        );
                        let _clone_source =
                            layout_source::enter(layout_source::OBJECT_PROPERTY_READ);
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let receiver_type = value_type_name(&other);
                                if let Err(result) = self.emit_non_object_property_read_warning(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut diagnostics,
                                    receiver_type,
                                    property,
                                    instruction.span,
                                ) {
                                    return result;
                                }
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Null)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if spl_array_object_uses_array_as_props(&object) {
                            let value = match spl_container_offset_get(
                                &object,
                                &Value::String(PhpString::from_test_str(property)),
                            ) {
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
                            continue;
                        }
                        if internal_throwable_instanceof(&object.class_name_handle(), "throwable")
                            .is_some()
                        {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_php_token_runtime_class(&object.class_name()) {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(&object.class_name()) {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_date_time_runtime_class(&object.class_name()) {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if normalize_class_name(&object.class_name()) == "simplexmlelement" {
                            let value = php_runtime::xml::simplexml_property(&object, property);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_pdo_runtime_class(&object.class_name()) {
                            let value = object.get_property(property).unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
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
                        let normalized_scope = scope.as_deref().map(normalize_class_name);
                        let receiver_class = normalize_class_name(&object.class_name());
                        let lookup_epoch = state.lookup_epoch();
                        let property_callsite = property_fetch_callsite(
                            compiled,
                            function_id,
                            block_id,
                            instruction.id,
                        );
                        let receiver_has_magic_get = class_has_public_magic_get(compiled, &class);
                        if let Some(target) = self.lookup_property_fetch_inline_cache(
                            compiled,
                            function_id,
                            block_id,
                            instruction.id,
                            property,
                            &receiver_class,
                            normalized_scope.as_deref(),
                            lookup_epoch,
                        ) {
                            match self
                                .read_property_fetch_target(compiled, target, &object, stack, state)
                            {
                                Ok(PropertyFetchCacheRead::Value(value)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(PropertyFetchCacheRead::Fallback) => {}
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                if let Some(value) = object.get_property(property) {
                                    self.record_counter_property_fetch_profile(
                                        property_fetch_profile_observation(
                                            &property_callsite,
                                            property,
                                            &receiver_class,
                                            &class,
                                            None,
                                            normalized_scope.as_deref(),
                                            lookup_epoch,
                                            receiver_has_magic_get,
                                            false,
                                            true,
                                            false,
                                            false,
                                            Vec::new(),
                                        ),
                                    );
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                self.record_counter_property_fetch_profile(
                                    property_fetch_profile_observation(
                                        &property_callsite,
                                        property,
                                        &receiver_class,
                                        &class,
                                        None,
                                        normalized_scope.as_deref(),
                                        lookup_epoch,
                                        receiver_has_magic_get,
                                        false,
                                        false,
                                        false,
                                        false,
                                        Vec::new(),
                                    ),
                                );
                                match self.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__get",
                                    property,
                                    vec![CallArgument::positional(Value::String(
                                        PhpString::from_test_str(property),
                                    ))],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(value)) => {
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return result,
                                }
                                if let Err(result) = self.emit_undefined_property_warning(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut diagnostics,
                                    &object.display_name(),
                                    property,
                                    instruction.span,
                                ) {
                                    return result;
                                }
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Null)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let resolved_class = &resolved.class;
                        let resolved_property = &resolved.property;
                        if resolved.property.flags.is_static {
                            if let Err(access_error) = validate_property_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                resolved_property,
                            ) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    instruction.span,
                                    access_error,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            emit_static_property_as_non_static_notice(
                                compiled,
                                output,
                                stack,
                                state,
                                resolved_class,
                                resolved_property,
                                instruction.span,
                            );
                            let value = match object.get_property(property) {
                                Some(value) => value,
                                None => {
                                    if let Err(result) = self.emit_undefined_property_warning(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut diagnostics,
                                        resolved_class.display_name.as_str(),
                                        property,
                                        instruction.span,
                                    ) {
                                        return result;
                                    }
                                    Value::Null
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
                            continue;
                        }
                        if let Err(access_error) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            resolved_class,
                            resolved_property,
                        ) {
                            self.record_counter_property_fetch_profile(
                                property_fetch_profile_observation(
                                    &property_callsite,
                                    property,
                                    &receiver_class,
                                    &class,
                                    Some((resolved_class, resolved_property)),
                                    normalized_scope.as_deref(),
                                    lookup_epoch,
                                    receiver_has_magic_get,
                                    property_has_hooks_or_active(
                                        state,
                                        &object,
                                        resolved_class,
                                        resolved_property,
                                    ),
                                    false,
                                    false,
                                    false,
                                    vec!["not_visible"],
                                ),
                            );
                            match self.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__get",
                                property,
                                vec![CallArgument::positional(Value::String(
                                    PhpString::from_test_str(property),
                                ))],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(value)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {
                                    if resolved.property.flags.is_private
                                        && normalize_class_name(&class.name)
                                            != normalize_class_name(&resolved_class.name)
                                        && let Some(value) = object.get_property(property)
                                    {
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    let result =
                                        self.runtime_error(output, compiled, stack, access_error);
                                    if let Some(throwable) = runtime_error_throwable(&result) {
                                        tag_throwable_location(
                                            &throwable,
                                            compiled,
                                            instruction.span,
                                        );
                                        state.pending_trace =
                                            Some(capture_backtrace_string(compiled, stack));
                                        if let Some(target) = handle_throw(
                                            compiled,
                                            throwable.clone(),
                                            stack,
                                            state,
                                            &mut exception_handlers,
                                            &mut pending_control,
                                        ) {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        return self
                                            .propagate_exception(output, stack, state, throwable);
                                    }
                                    return result;
                                }
                                Err(result) => return result,
                            }
                        }
                        let resolved_has_property_hook = property_has_hooks_or_active(
                            state,
                            &object,
                            resolved_class,
                            resolved_property,
                        );
                        if !property_hook_is_active(
                            state,
                            &object,
                            resolved_class,
                            resolved_property,
                        ) && let Some(function) = resolved.property.hooks.get
                        {
                            self.record_counter_property_fetch_profile(
                                property_fetch_profile_observation(
                                    &property_callsite,
                                    property,
                                    &receiver_class,
                                    &class,
                                    Some((resolved_class, resolved_property)),
                                    normalized_scope.as_deref(),
                                    lookup_epoch,
                                    receiver_has_magic_get,
                                    resolved_has_property_hook,
                                    false,
                                    true,
                                    false,
                                    Vec::new(),
                                ),
                            );
                            match self.call_property_hook(
                                compiled,
                                object.clone(),
                                resolved_class,
                                resolved_property,
                                function,
                                Vec::new(),
                                output,
                                stack,
                                state,
                            ) {
                                Ok(value) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Err(result) => return result,
                            }
                        }
                        let storage_name = property_storage_name(resolved_class, resolved_property);
                        let value = match object.get_property(&storage_name) {
                            Some(value) => value,
                            None => {
                                self.record_counter_property_fetch_profile(
                                    property_fetch_profile_observation(
                                        &property_callsite,
                                        property,
                                        &receiver_class,
                                        &class,
                                        Some((resolved_class, resolved_property)),
                                        normalized_scope.as_deref(),
                                        lookup_epoch,
                                        receiver_has_magic_get,
                                        resolved_has_property_hook,
                                        false,
                                        true,
                                        false,
                                        vec!["missing_declared_storage"],
                                    ),
                                );
                                match self.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__get",
                                    property,
                                    vec![CallArgument::positional(Value::String(
                                        PhpString::from_test_str(property),
                                    ))],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(value)) => {
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return result,
                                }
                                if let Err(result) = self.emit_undefined_property_warning(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut diagnostics,
                                    &object.display_name(),
                                    property,
                                    instruction.span,
                                ) {
                                    return result;
                                }
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Null)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                        };
                        if matches!(value, Value::Uninitialized) {
                            self.record_counter_property_fetch_profile(
                                property_fetch_profile_observation(
                                    &property_callsite,
                                    property,
                                    &receiver_class,
                                    &class,
                                    Some((resolved_class, resolved_property)),
                                    normalized_scope.as_deref(),
                                    lookup_epoch,
                                    receiver_has_magic_get,
                                    resolved_has_property_hook,
                                    false,
                                    true,
                                    true,
                                    Vec::new(),
                                ),
                            );
                            let message = format!(
                                "E_PHP_VM_UNINITIALIZED_PROPERTY: Typed property {}::${property} must not be accessed before initialization",
                                resolved.class.display_name
                            );
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        self.record_counter_property_fetch_profile(
                            property_fetch_profile_observation(
                                &property_callsite,
                                property,
                                &receiver_class,
                                &class,
                                Some((resolved_class, resolved_property)),
                                normalized_scope.as_deref(),
                                lookup_epoch,
                                receiver_has_magic_get,
                                resolved_has_property_hook,
                                false,
                                true,
                                false,
                                Vec::new(),
                            ),
                        );
                        self.maybe_install_property_fetch_inline_cache_target(
                            compiled,
                            function_id,
                            block_id,
                            instruction.id,
                            property,
                            &receiver_class,
                            &class,
                            resolved_class,
                            resolved_property,
                            &storage_name,
                            normalized_scope.as_deref(),
                            lookup_epoch,
                            receiver_has_magic_get,
                            state,
                            &object,
                            None,
                        );
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::FetchStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => {
                        match self.fetch_static_property_value(
                            compiled,
                            class_name,
                            property,
                            Some((function_id, block_id, instruction.id)),
                            None,
                            instruction.span,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(value) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                            Err(ClassConstantFetch::Throwable(result)) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            Err(ClassConstantFetch::Raise(span, message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(ClassConstantFetch::Fatal(message)) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        }
                    }
                    InstructionKind::FetchDynamicStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => {
                        let class_name_value =
                            match read_operand_at_frame(unit, stack, frame_index, *class_name) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let class_name =
                            match dynamic_static_class_name_from_value(&class_name_value) {
                                Ok(name) => name,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        if let Err(result) = self.autoload_static_class_if_missing(
                            compiled,
                            &class_name,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                            stack,
                            state,
                        ) {
                            match self.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let class =
                            match resolve_static_class_name(compiled, state, stack, &class_name) {
                                Ok(class) => class,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                        if !state.static_properties.contains_key(&key) {
                            let default = match static_property_default(
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
                            };
                            state.static_properties.insert(key.clone(), default);
                        }
                        let value = state
                            .static_properties
                            .get(&key)
                            .cloned()
                            .unwrap_or(Value::Uninitialized);
                        if matches!(value, Value::Uninitialized) {
                            let message = format!(
                                "E_PHP_VM_UNINITIALIZED_STATIC_PROPERTY: typed static property {}::${} must not be accessed before initialization",
                                resolved.class.display_name, resolved.property.name
                            );
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::IssetStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => {
                        let result = match static_property_isset_empty_result(
                            self,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            false,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(result) => result,
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            .set(*dst, Value::Bool(result))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => {
                        let result = match static_property_isset_empty_result(
                            self,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            true,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(result) => result,
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            .set(*dst, Value::Bool(result))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::IssetStaticPropertyDim {
                        dst,
                        class_name,
                        property,
                        dims,
                    } => {
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let result = match static_property_dim_isset_empty_result(
                            self,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            &dims,
                            false,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(result) => result,
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            .set(*dst, Value::Bool(result))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyStaticPropertyDim {
                        dst,
                        class_name,
                        property,
                        dims,
                    } => {
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let result = match static_property_dim_isset_empty_result(
                            self,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            &dims,
                            true,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(result) => result,
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            .set(*dst, Value::Bool(result))
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::FetchClassConstant {
                        dst,
                        class_name,
                        constant,
                    } => {
                        match self.fetch_class_constant_value(
                            compiled,
                            class_name,
                            constant,
                            Some((function_id, block_id, instruction.id)),
                            None,
                            instruction.span,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(value) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                            Err(ClassConstantFetch::Throwable(result)) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            Err(ClassConstantFetch::Raise(span, message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(ClassConstantFetch::Fatal(message)) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        }
                    }
                    InstructionKind::FetchDynamicProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let property = match self.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                let receiver_type = value_type_name(&other);
                                if let Err(result) = self.emit_non_object_property_read_warning(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut diagnostics,
                                    receiver_type,
                                    &property,
                                    instruction.span,
                                ) {
                                    return result;
                                }
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Null)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match self
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        if spl_array_object_uses_array_as_props(&object) {
                            let value = match spl_container_offset_get(
                                &object,
                                &Value::String(PhpString::from_test_str(&property)),
                            ) {
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
                            continue;
                        }
                        if normalize_class_name(&object.class_name()) == "simplexmlelement" {
                            let value = php_runtime::xml::simplexml_property(&object, &property);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if let Some(class) =
                            lookup_class_in_state(compiled, state, &object.class_name())
                        {
                            let scope = current_scope_class(compiled, stack);
                            match lookup_resolved_property_in_state(
                                compiled,
                                state,
                                &class,
                                &property,
                                scope.as_deref(),
                            ) {
                                Ok(Some(resolved)) => {
                                    if let Err(access_error) = validate_property_access_in_state(
                                        compiled,
                                        state,
                                        stack,
                                        &resolved.class,
                                        &resolved.property,
                                    ) {
                                        match self.call_magic_property_method(
                                            compiled,
                                            object.clone(),
                                            "__get",
                                            &property,
                                            vec![CallArgument::positional(Value::String(
                                                PhpString::from_test_str(&property),
                                            ))],
                                            output,
                                            stack,
                                            state,
                                        ) {
                                            Ok(Some(value)) => {
                                                if let Err(message) = stack
                                                    .frame_mut(frame_index)
                                                    .expect("frame was pushed")
                                                    .registers
                                                    .set(*dst, value)
                                                {
                                                    return self.runtime_error(
                                                        output, compiled, stack, message,
                                                    );
                                                }
                                                continue;
                                            }
                                            Ok(None) => {
                                                match self.raise_runtime_error(
                                                    compiled,
                                                    output,
                                                    stack,
                                                    state,
                                                    &mut exception_handlers,
                                                    &mut pending_control,
                                                    instruction.span,
                                                    access_error,
                                                ) {
                                                    RaiseOutcome::Caught(target) => {
                                                        block_id = target;
                                                        continue 'dispatch;
                                                    }
                                                    RaiseOutcome::Done(result) => return *result,
                                                }
                                            }
                                            Err(result) => return result,
                                        }
                                    }
                                }
                                Ok(None) => {}
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                        let value = match property_state_value(
                            compiled, state, stack, &object, &property,
                        ) {
                            Some(value) => value,
                            None => match self.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__get",
                                &property,
                                vec![CallArgument::positional(Value::String(
                                    PhpString::from_test_str(&property),
                                ))],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(value)) => value,
                                Ok(None) => object.get_property(&property).unwrap_or(Value::Null),
                                Err(result) => return result,
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
                    InstructionKind::IssetProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = match self
                            .isset_property_value(compiled, &object, property, output, stack, state)
                        {
                            Ok(value) => value,
                            Err(result) => return result,
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
                    InstructionKind::IssetDynamicProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let property = match self.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Bool(false))
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                let _ = (other, property);
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match self
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let value =
                            property_state_value(compiled, state, stack, &object, &property);
                        let result = if let Some(value) = value {
                            !matches!(value, Value::Uninitialized | Value::Null)
                        } else {
                            match self.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__isset",
                                &property,
                                vec![CallArgument::positional(Value::String(
                                    PhpString::from_test_str(&property),
                                ))],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(value)) => match to_bool(&value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                },
                                Ok(None) => false,
                                Err(result) => return result,
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
                    InstructionKind::EmptyProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = match self
                            .empty_property_value(compiled, &object, property, output, stack, state)
                        {
                            Ok(value) => value,
                            Err(result) => return result,
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
                    InstructionKind::EmptyDynamicProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let property = match self.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Bool(true))
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                let _ = (other, property);
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match self
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let result = match property_state_value(
                            compiled, state, stack, &object, &property,
                        ) {
                            Some(value) => match php_empty_access_value(&value) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            },
                            None => {
                                let isset = match self.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__isset",
                                    &property,
                                    vec![CallArgument::positional(Value::String(
                                        PhpString::from_test_str(&property),
                                    ))],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(value)) => match to_bool(&value) {
                                        Ok(value) => value,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    },
                                    Ok(None) => false,
                                    Err(result) => return result,
                                };
                                if !isset {
                                    true
                                } else {
                                    match self.call_magic_property_method(
                                        compiled,
                                        object.clone(),
                                        "__get",
                                        &property,
                                        vec![CallArgument::positional(Value::String(
                                            PhpString::from_test_str(&property),
                                        ))],
                                        output,
                                        stack,
                                        state,
                                    ) {
                                        Ok(Some(value)) => match php_empty_access_value(&value) {
                                            Ok(value) => value,
                                            Err(message) => {
                                                return self.runtime_error(
                                                    output, compiled, stack, message,
                                                );
                                            }
                                        },
                                        Ok(None) => true,
                                        Err(result) => return result,
                                    }
                                }
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
                    InstructionKind::IssetDynamicPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => Some(object),
                            Ok(_) => None,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match self
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = object.as_ref().and_then(|object| {
                            property_state_value(compiled, state, stack, object, &property)
                                .and_then(|value| {
                                    fetch_dim_path_value(&value, &dims).ok().flatten()
                                })
                        });
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(
                                *dst,
                                Value::Bool(!matches!(value, None | Some(Value::Null))),
                            )
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::EmptyDynamicPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => Some(object),
                            Ok(_) => None,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match self
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = object
                            .as_ref()
                            .and_then(|object| {
                                property_state_value(compiled, state, stack, object, &property)
                                    .and_then(|value| {
                                        fetch_dim_path_value(&value, &dims).ok().flatten()
                                    })
                            })
                            .unwrap_or(Value::Uninitialized);
                        let result = match php_empty_access_value(&value) {
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
                    InstructionKind::IssetPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Bool(false))
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                let _ = other;
                                continue;
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
                        // Borrowed probe: isset must not clone the property
                        // container (the clone shares the array handle and
                        // forces a full copy-on-write separation on the next
                        // write to the same registry-style array).
                        let borrowed = with_property_state_value(
                            compiled,
                            state,
                            stack,
                            &object,
                            property,
                            &mut |value| match value {
                                Some(value) => with_borrowed_dim_path(value, &dims, &mut |leaf| {
                                    !matches!(leaf, None | Some(Value::Null))
                                }),
                                None => Some(false),
                            },
                        )
                        .flatten();
                        let result = match borrowed {
                            Some(result) => {
                                self.record_counter_property_dim_probe_borrowed_hit();
                                result
                            }
                            None => {
                                let value =
                                    property_state_value(compiled, state, stack, &object, property)
                                        .and_then(|value| {
                                            fetch_dim_path_value(&value, &dims).ok().flatten()
                                        });
                                !matches!(value, None | Some(Value::Null))
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
                    InstructionKind::EmptyPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, Value::Bool(true))
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                let _ = other;
                                continue;
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
                        // Borrowed probe mirroring the isset arm: empty()
                        // only needs a borrowed view of the leaf value.
                        let borrowed = with_property_state_value(
                            compiled,
                            state,
                            stack,
                            &object,
                            property,
                            &mut |value| match value {
                                Some(value) => with_borrowed_dim_path(value, &dims, &mut |leaf| {
                                    php_empty_access_value(leaf.unwrap_or(&Value::Uninitialized))
                                }),
                                None => Some(php_empty_access_value(&Value::Uninitialized)),
                            },
                        )
                        .flatten();
                        let result = match borrowed {
                            Some(result) => {
                                self.record_counter_property_dim_probe_borrowed_hit();
                                result
                            }
                            None => {
                                let value =
                                    property_state_value(compiled, state, stack, &object, property)
                                        .and_then(|value| {
                                            fetch_dim_path_value(&value, &dims).ok().flatten()
                                        })
                                        .unwrap_or(Value::Uninitialized);
                                php_empty_access_value(&value)
                            }
                        };
                        let result = match result {
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
                    InstructionKind::UnsetProperty { object, property } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot unset property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        match self.unset_property_value(
                            compiled,
                            &object,
                            property,
                            instruction.span,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(()) => {}
                            Err(StaticPropertyAssignError::Vm(result)) => return *result,
                            Err(StaticPropertyAssignError::Raise(span, message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyAssignError::Fatal(message)) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        }
                    }
                    InstructionKind::UnsetPropertyDim {
                        object,
                        property,
                        dims,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot unset property {property} on {}",
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
                        if let Err(message) =
                            unset_property_dim(compiled, state, stack, &object, property, &dims)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::UnsetDynamicProperty { object, property } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                let property = match self.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot unset property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match self
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        let class = compiled.lookup_class(&object.class_name());
                        let scope = current_scope_class(compiled, stack);
                        let declared = match class {
                            Some(class) => match lookup_property_in_hierarchy(
                                compiled,
                                class,
                                &property,
                                scope.as_deref(),
                            ) {
                                Ok(property) => property,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            },
                            None => None,
                        };
                        if let Some(resolved) = declared {
                            if let Err(message) = validate_property_access(
                                compiled,
                                stack,
                                resolved.class,
                                resolved.property,
                            ) {
                                match self.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__unset",
                                    &property,
                                    vec![CallArgument::positional(Value::String(
                                        PhpString::from_test_str(&property),
                                    ))],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(_)) => {}
                                    Ok(None) => {
                                        match self.raise_runtime_error(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                    Err(result) => return result,
                                }
                                continue;
                            }
                            let storage_name =
                                property_storage_name(resolved.class, resolved.property);
                            if resolved.property.flags.is_typed {
                                object.set_property(storage_name, Value::Uninitialized);
                            } else {
                                object.unset_property(&storage_name);
                            }
                        } else {
                            match self.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__unset",
                                &property,
                                vec![CallArgument::positional(Value::String(
                                    PhpString::from_test_str(&property),
                                ))],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) | Ok(None) => {
                                    object.unset_property(&property);
                                }
                                Err(result) => return result,
                            }
                        }
                    }
                    InstructionKind::FetchObjectClassName { dst, object } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object) {
                            Ok(Value::Object(object)) => object,
                            Ok(other) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    instruction.span,
                                    format!(
                                        "E_PHP_VM_DYNAMIC_CLASS_NAME_TYPE: Cannot use \"::class\" on {}",
                                        value_type_name(&other)
                                    ),
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
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(
                                *dst,
                                Value::String(PhpString::from_test_str(&object.display_name())),
                            )
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::AssignProperty {
                        dst,
                        object,
                        property,
                        value,
                    } => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Object,
                            "property_assign",
                        );
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object) {
                            Ok(Value::Object(object)) => object,
                            Ok(Value::Callable(_)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                                        "E_PHP_VM_PROPERTY_ASSIGN_NON_OBJECT: cannot assign property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if spl_array_object_uses_array_as_props(&object) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            if let Err(message) = spl_container_offset_set(
                                &object,
                                Value::String(PhpString::from_test_str(property)),
                                value.clone(),
                            ) {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(&object.class_name()) {
                            self.record_counter_property_assign_ic_fallback(
                                "dynamic_property_fallback",
                            );
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            object.set_property(property, value.clone());
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
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
                        let normalized_scope = scope.as_deref().map(normalize_class_name);
                        let receiver_class = normalize_class_name(&object.class_name());
                        let lookup_epoch = state.lookup_epoch();
                        let receiver_has_magic_set = class_has_public_magic_set(compiled, &class);
                        if let Some(target) = self.lookup_property_assign_inline_cache(
                            compiled,
                            function_id,
                            block_id,
                            instruction.id,
                            property,
                            &receiver_class,
                            normalized_scope.as_deref(),
                            lookup_epoch,
                        ) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            match self.write_property_assign_target(
                                compiled, target, &object, value, stack, state,
                            ) {
                                Ok(PropertyAssignCacheWrite::Written(value)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(PropertyAssignCacheWrite::Fallback) => {}
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                        }
                        let resolved = match lookup_resolved_property_in_state(
                            compiled,
                            state,
                            &class,
                            property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                self.record_counter_property_assign_ic_fallback(
                                    "dynamic_property_fallback",
                                );
                                let value =
                                    match read_operand_at_frame(unit, stack, frame_index, *value) {
                                        Ok(value) => value,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                match self.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__set",
                                    property,
                                    vec![
                                        CallArgument::positional(Value::String(
                                            PhpString::from_test_str(property),
                                        )),
                                        CallArgument::positional(value.clone()),
                                    ],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(_)) => {
                                        self.record_counter_property_assign_ic_fallback(
                                            "magic_set_metadata",
                                        );
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return result,
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
                                object.set_property(property, value.clone());
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let resolved_class = &resolved.class;
                        let entry = &resolved.property;
                        if entry.flags.is_static {
                            if let Err(message) = validate_property_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                entry,
                            )
                            .and_then(|()| {
                                validate_property_set_access_in_state(
                                    compiled,
                                    state,
                                    stack,
                                    resolved_class,
                                    entry,
                                )
                            }) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            emit_static_property_as_non_static_notice(
                                compiled,
                                output,
                                stack,
                                state,
                                resolved_class,
                                entry,
                                instruction.span,
                            );
                            object.set_property(property, value.clone());
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if let Err(message) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            resolved_class,
                            entry,
                        )
                        .and_then(|()| {
                            validate_property_set_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                entry,
                            )
                        }) {
                            self.record_counter_property_assign_ic_fallback("visibility_mismatch");
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            match self.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__set",
                                property,
                                vec![
                                    CallArgument::positional(Value::String(
                                        PhpString::from_test_str(property),
                                    )),
                                    CallArgument::positional(value.clone()),
                                ],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) => {
                                    self.record_counter_property_assign_ic_fallback(
                                        "magic_set_metadata",
                                    );
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {
                                    if entry.flags.is_private
                                        && normalize_class_name(&class.name)
                                            != normalize_class_name(&resolved_class.name)
                                    {
                                        if let Some(diagnostic) =
                                            dynamic_property_deprecation_diagnostic(
                                                compiled,
                                                state,
                                                &class,
                                                &object,
                                                property.as_ref(),
                                                stack,
                                            )
                                        {
                                            diagnostics.push(diagnostic);
                                        }
                                        object.set_property(property, value.clone());
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                Err(result) => return result,
                            }
                        }
                        let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property_type = ir_runtime_type(entry.type_.as_ref());
                        if let Err(message) = check_property_type(
                            compiled,
                            Some(state),
                            resolved.class.display_name.as_str(),
                            property,
                            &property_type,
                            &value,
                            self.typecheck_fast_path_context(),
                        ) {
                            self.record_counter_property_assign_ic_fallback("type_mismatch");
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        if let Err(message) =
                            validate_property_write(resolved_class, entry, &object, stack, compiled)
                        {
                            self.record_counter_property_assign_ic_fallback("readonly_property");
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        if !property_hook_is_active(state, &object, resolved_class, entry)
                            && let Some(function) = entry.hooks.set
                        {
                            self.record_counter_property_assign_ic_fallback(
                                "property_hook_present",
                            );
                            match self.call_property_hook(
                                compiled,
                                object.clone(),
                                resolved_class,
                                entry,
                                function,
                                vec![CallArgument::positional(value.clone())],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(_) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Err(result) => return result,
                            }
                        }
                        if !entry.hooks.backed
                            && (entry.hooks.get.is_some() || entry.hooks.set.is_some())
                        {
                            self.record_counter_property_assign_ic_fallback(
                                "property_hook_present",
                            );
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_VIRTUAL_PROPERTY_WRITE: property {}::${} has no backing storage",
                                    resolved_class.name, entry.name
                                ),
                            );
                        }
                        let storage_name = property_storage_name(resolved_class, entry);
                        if !entry.flags.is_typed
                            && object.get_property(&storage_name).is_none()
                            && !magic_property_call_is_active(state, &object, "__set", property)
                        {
                            match self.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__set",
                                property,
                                vec![
                                    CallArgument::positional(Value::String(
                                        PhpString::from_test_str(property),
                                    )),
                                    CallArgument::positional(value.clone()),
                                ],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {}
                                Err(result) => return result,
                            }
                        }
                        if matches!(
                            object.get_property(&storage_name),
                            Some(Value::Reference(_))
                        ) {
                            self.record_counter_property_assign_ic_fallback("reference_slot");
                        }
                        write_property_storage_value(&object, &storage_name, value.clone());
                        self.maybe_install_property_assign_inline_cache_target(
                            compiled,
                            function_id,
                            block_id,
                            instruction.id,
                            property,
                            &receiver_class,
                            &class,
                            resolved_class,
                            entry,
                            &storage_name,
                            normalized_scope.as_deref(),
                            lookup_epoch,
                            receiver_has_magic_set,
                            state,
                            &object,
                            None,
                        );
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::AssignDynamicProperty {
                        dst,
                        object,
                        property,
                        value,
                    } => {
                        let object = match read_operand_at_frame(unit, stack, frame_index, *object)
                        {
                            Ok(Value::Object(object)) => object,
                            Ok(Value::Callable(_)) => {
                                let property = match self.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                                let property = match self.dynamic_property_name(
                                    unit, compiled, stack, *property, output, state,
                                ) {
                                    Ok(property) => property,
                                    Err(result) => return result,
                                };
                                return self.runtime_error(
                                    output,
                                    compiled,
                                    stack,
                                    format!(
                                        "E_PHP_VM_PROPERTY_ASSIGN_NON_OBJECT: cannot assign property {property} on {}",
                                        value_type_name(&other)
                                    ),
                                );
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property = match self
                            .dynamic_property_name(unit, compiled, stack, *property, output, state)
                        {
                            Ok(property) => property,
                            Err(result) => return result,
                        };
                        if spl_array_object_uses_array_as_props(&object) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            if let Err(message) = spl_container_offset_set(
                                &object,
                                Value::String(PhpString::from_test_str(&property)),
                                value.clone(),
                            ) {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_std_class_runtime_class(&object.class_name()) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            object.set_property(&property, value.clone());
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
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
                            &property,
                            scope.as_deref(),
                        ) {
                            Ok(Some(resolved)) => resolved,
                            Ok(None) => {
                                let value =
                                    match read_operand_at_frame(unit, stack, frame_index, *value) {
                                        Ok(value) => value,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                match self.call_magic_property_method(
                                    compiled,
                                    object.clone(),
                                    "__set",
                                    &property,
                                    vec![
                                        CallArgument::positional(Value::String(
                                            PhpString::from_test_str(&property),
                                        )),
                                        CallArgument::positional(value.clone()),
                                    ],
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(_)) => {
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(result) => return result,
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
                                object.set_property(&property, value.clone());
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let resolved_class = &resolved.class;
                        let entry = &resolved.property;
                        if entry.flags.is_static {
                            if let Err(message) = validate_property_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                entry,
                            )
                            .and_then(|()| {
                                validate_property_set_access_in_state(
                                    compiled,
                                    state,
                                    stack,
                                    resolved_class,
                                    entry,
                                )
                            }) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            emit_static_property_as_non_static_notice(
                                compiled,
                                output,
                                stack,
                                state,
                                resolved_class,
                                entry,
                                instruction.span,
                            );
                            object.set_property(&property, value.clone());
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame was pushed")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if let Err(message) = validate_property_access_in_state(
                            compiled,
                            state,
                            stack,
                            resolved_class,
                            entry,
                        )
                        .and_then(|()| {
                            validate_property_set_access_in_state(
                                compiled,
                                state,
                                stack,
                                resolved_class,
                                entry,
                            )
                        }) {
                            let value =
                                match read_operand_at_frame(unit, stack, frame_index, *value) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            match self.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__set",
                                &property,
                                vec![
                                    CallArgument::positional(Value::String(
                                        PhpString::from_test_str(&property),
                                    )),
                                    CallArgument::positional(value.clone()),
                                ],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {
                                    if entry.flags.is_private
                                        && normalize_class_name(&class.name)
                                            != normalize_class_name(&resolved_class.name)
                                    {
                                        if let Some(diagnostic) =
                                            dynamic_property_deprecation_diagnostic(
                                                compiled,
                                                state,
                                                &class,
                                                &object,
                                                property.as_ref(),
                                                stack,
                                            )
                                        {
                                            diagnostics.push(diagnostic);
                                        }
                                        object.set_property(&property, value.clone());
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame was pushed")
                                            .registers
                                            .set(*dst, value)
                                        {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                Err(result) => return result,
                            }
                        }
                        let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let property_type = ir_runtime_type(entry.type_.as_ref());
                        if let Err(message) = check_property_type(
                            compiled,
                            Some(state),
                            resolved_class.display_name.as_str(),
                            &property,
                            &property_type,
                            &value,
                            self.typecheck_fast_path_context(),
                        ) {
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        if let Err(message) =
                            validate_property_write(resolved_class, entry, &object, stack, compiled)
                        {
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        if !property_hook_is_active(state, &object, resolved_class, entry)
                            && let Some(function) = entry.hooks.set
                        {
                            match self.call_property_hook(
                                compiled,
                                object.clone(),
                                resolved_class,
                                entry,
                                function,
                                vec![CallArgument::positional(value.clone())],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(_) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Err(result) => return result,
                            }
                        }
                        if !entry.hooks.backed
                            && (entry.hooks.get.is_some() || entry.hooks.set.is_some())
                        {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                format!(
                                    "E_PHP_VM_VIRTUAL_PROPERTY_WRITE: property {}::${} has no backing storage",
                                    resolved_class.name, entry.name
                                ),
                            );
                        }
                        let storage_name = property_storage_name(resolved_class, entry);
                        if !entry.flags.is_typed
                            && object.get_property(&storage_name).is_none()
                            && !magic_property_call_is_active(state, &object, "__set", &property)
                        {
                            match self.call_magic_property_method(
                                compiled,
                                object.clone(),
                                "__set",
                                &property,
                                vec![
                                    CallArgument::positional(Value::String(
                                        PhpString::from_test_str(&property),
                                    )),
                                    CallArgument::positional(value.clone()),
                                ],
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(_)) => {
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame was pushed")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                Ok(None) => {}
                                Err(result) => return result,
                            }
                        }
                        object.set_property(storage_name, value.clone());
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::AssignPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                        append,
                        value,
                    } => {
                        let _profile = self.request_profile_operation_start(
                            RequestProfileOperationCategory::Object,
                            "property_dim_assign",
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
                                        "E_PHP_VM_PROPERTY_DIM_ASSIGN_NON_OBJECT: cannot assign property dimension {property} on {}",
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
                        let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        match self.assign_property_dim_value(
                            compiled,
                            object,
                            property,
                            &dims,
                            *append,
                            value,
                            instruction.span,
                            &mut diagnostics,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(value) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame was pushed")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            }
                            Err(PropertyDimAssign::Raise(span, message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(PropertyDimAssign::Fatal(message)) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            Err(PropertyDimAssign::Return(result)) => return *result,
                        }
                    }
                    InstructionKind::AssignStaticProperty {
                        dst,
                        class_name,
                        property,
                        value,
                    } => {
                        let value_operand = *value;
                        let value =
                            match read_operand_at_frame(unit, stack, frame_index, value_operand) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let (value, previous_effective) = match self.assign_static_property_value(
                            compiled,
                            class_name,
                            property,
                            value,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            instruction.span,
                            output,
                            stack,
                            state,
                        ) {
                            Ok(outcome) => outcome,
                            Err(StaticPropertyAssignError::Vm(result)) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            Err(StaticPropertyAssignError::Raise(span, message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    span,
                                    message,
                                ) {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                            Err(StaticPropertyAssignError::Fatal(message)) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                            compiled,
                            output,
                            stack,
                            state,
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
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = unset_consumed_assignment_value_operand_at_frame(
                            stack,
                            frame_index,
                            value_operand,
                            *dst,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::AssignDynamicStaticProperty {
                        dst,
                        class_name,
                        property,
                        value,
                    } => {
                        let class_name_value =
                            match read_operand_at_frame(unit, stack, frame_index, *class_name) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let class_name =
                            match dynamic_static_class_name_from_value(&class_name_value) {
                                Ok(name) => name,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        if let Err(result) = self.autoload_static_class_if_missing(
                            compiled,
                            &class_name,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                            stack,
                            state,
                        ) {
                            match self.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let class =
                            match resolve_static_class_name(compiled, state, stack, &class_name) {
                                Ok(class) => class,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                        let value_operand = *value;
                        let value =
                            match read_operand_at_frame(unit, stack, frame_index, value_operand) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let property_type = ir_runtime_type(resolved.property.type_.as_ref());
                        if let Err(message) = check_property_type(
                            compiled,
                            Some(state),
                            resolved.class.display_name.as_str(),
                            resolved.property.name.as_str(),
                            &property_type,
                            &value,
                            self.typecheck_fast_path_context(),
                        ) {
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let previous_effective = effective_value(&current);
                        if let Err(message) = write_static_property_lvalue(
                            &mut state.static_properties,
                            key,
                            current.clone(),
                            value.clone(),
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Some(outcome) = self.run_destructors_for_unreferenced_value(
                            compiled,
                            output,
                            stack,
                            state,
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
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = unset_consumed_assignment_value_operand_at_frame(
                            stack,
                            frame_index,
                            value_operand,
                            *dst,
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
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
                            compiled,
                            dispatch_contract::RichInstructionSite {
                                unit,
                                function,
                                function_id,
                                block_id,
                                instruction,
                                frame_index,
                            },
                            output,
                            stack,
                            state,
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
                            compiled,
                            output,
                            stack,
                            state,
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
                        release_unrooted_object_handles(&previous, stack, state);
                    }
                    InstructionKind::UnsetStaticPropertyDim {
                        class_name,
                        property,
                        dims,
                    } => {
                        let dims = match read_dim_operands_at_frame(unit, stack, frame_index, dims)
                        {
                            Ok(dims) => dims,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        match static_property_dim_unset_result(
                            self,
                            compiled,
                            state,
                            stack,
                            class_name,
                            property,
                            &dims,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                        ) {
                            Ok(()) => {}
                            Err(StaticPropertyIssetEmptyError::Runtime(message)) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            Err(StaticPropertyIssetEmptyError::Vm(result)) => {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                    kind @ (InstructionKind::ForeachInit { .. }
                    | InstructionKind::ForeachNext { .. }
                    | InstructionKind::ForeachCleanup { .. }
                    | InstructionKind::ForeachInitRef { .. }
                    | InstructionKind::ForeachNextRef { .. }) => {
                        match execute_rich_foreach_instruction(
                            self,
                            compiled,
                            unit,
                            frame_index,
                            kind,
                            instruction.span,
                            output,
                            stack,
                            state,
                            &mut foreach_iterators,
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
                                compiled,
                                output,
                                stack,
                                state,
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
                                php_runtime::PhpDiagnosticChannel::Warning,
                                php_runtime::PHP_E_WARNING,
                            ),
                            IrDiagnosticSeverity::Deprecation => (
                                RuntimeSeverity::Deprecation,
                                php_runtime::PhpDiagnosticChannel::Deprecated,
                                php_runtime::PHP_E_DEPRECATED,
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
                            Err(result) => return result,
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
                            Err(result) => return result,
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
                    InstructionKind::CallFunction { dst, name, args } => {
                        match self.try_execute_rich_preg_match_start_offset_ascii_fast(
                            name,
                            args,
                            unit,
                            stack,
                            frame_index,
                            state,
                        ) {
                            Ok(Some(return_value)) => {
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, return_value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Ok(None) => {}
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        }
                        let values = match read_call_args_for_function_at_frame(
                            unit,
                            stack,
                            frame_index,
                            name,
                            args,
                        ) {
                            Ok(values) => values,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let lowered_name = normalize_function_name(name);
                        let interned_name = PhpString::intern(lowered_name.as_bytes());
                        let epoch = state.lookup_epoch();
                        let call_shape = function_call_shape(&values);
                        let target = self
                            .lookup_function_call_inline_cache(
                                compiled,
                                function_id,
                                block_id,
                                instruction.id,
                                &interned_name,
                                epoch,
                                &call_shape,
                            )
                            .or_else(|| {
                                let resolved = self.resolve_function_call_target(
                                    compiled,
                                    state,
                                    &lowered_name,
                                )?;
                                if self.options.inline_caches.enabled()
                                    && function_call_target_is_builtin(&resolved)
                                {
                                    self.record_counter_builtin_call_ic(false);
                                }
                                self.install_function_call_inline_cache(
                                    compiled,
                                    function_id,
                                    block_id,
                                    instruction.id,
                                    &interned_name,
                                    epoch,
                                    call_shape.clone(),
                                    resolved.clone(),
                                );
                                Some(resolved)
                            });
                        let Some(target) = target else {
                            let diagnostic = undefined_function(
                                name,
                                RuntimeSourceSpan::default(),
                                stack_trace(compiled, stack),
                            );
                            return VmResult::runtime_error_with_diagnostic(
                                output.clone(),
                                diagnostic.message().to_owned(),
                                diagnostic,
                            );
                        };
                        let temporary_iterator_arg =
                            iterator_function_temporary_arg_value(&lowered_name, args, &values);
                        let result = self.execute_function_call_target(
                            compiled,
                            target,
                            values,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            Some(instruction.span),
                            output,
                            stack,
                            state,
                            &running_fiber,
                        );
                        if !result.status.is_success() {
                            let original_throwable = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result));
                            if let Some((arg_operand, arg_value)) = temporary_iterator_arg.clone() {
                                if let Err(message) =
                                    unset_register_operand_at_frame(stack, frame_index, arg_operand)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                let candidates = destructor_candidates_for_value(&arg_value);
                                let rooted_object_ids =
                                    php_visible_non_register_root_object_ids(stack, state);
                                let sweep = self
                                    .run_destructors_for_unreferenced_candidates_with_roots(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut exception_handlers,
                                        &mut pending_control,
                                        candidates,
                                        &rooted_object_ids,
                                        None,
                                    );
                                if let Some(outcome) = sweep.outcome {
                                    match outcome {
                                        RaiseOutcome::Caught(target) => {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            }
                            if let Some(throwable) = original_throwable {
                                state.pending_throw = Some(throwable);
                            }
                        }
                        if (!result.status.is_success() || state.pending_throw.is_some())
                            && let Some(throwable) = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result))
                        {
                            if let Some(target) = handle_throw(
                                compiled,
                                throwable.clone(),
                                stack,
                                state,
                                &mut exception_handlers,
                                &mut pending_control,
                            ) {
                                block_id = target;
                                continue 'dispatch;
                            }
                            return self.propagate_exception(output, stack, state, throwable);
                        }
                        if !result.status.is_success() || state.pending_throw.is_some() {
                            return result;
                        }
                        if result.fiber_suspension.is_some() {
                            return self.propagate_fiber_suspension(
                                result,
                                compiled,
                                *dst,
                                block_id,
                                instruction_index + 1,
                                &foreach_iterators,
                                &exception_handlers,
                                &pending_control,
                                output,
                                stack,
                            );
                        }
                        diagnostics.extend(result.diagnostics);
                        let return_value = result.return_value.unwrap_or(Value::Null);
                        if let Some((arg_operand, arg_value)) = temporary_iterator_arg {
                            if let Err(message) =
                                unset_register_operand_at_frame(stack, frame_index, arg_operand)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            let mut rooted_object_ids =
                                php_visible_non_register_root_object_ids(stack, state);
                            rooted_object_ids.extend(preserved_destructor_object_ids(
                                std::slice::from_ref(&return_value),
                            ));
                            let candidates = destructor_candidates_for_value(&arg_value);
                            let sweep = self
                                .run_destructors_for_unreferenced_candidates_with_roots(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    candidates,
                                    &rooted_object_ids,
                                    None,
                                );
                            if let Some(outcome) = sweep.outcome {
                                match outcome {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                        }
                        if let Err(message) = unset_consumed_call_arg_registers_at_frame(
                            stack,
                            frame_index,
                            args,
                            Some(*dst),
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, return_value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::CallMethod {
                        dst,
                        object,
                        method,
                        args,
                    } => {
                        let object_operand = *object;
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
                            Value::Fiber(fiber) => {
                                let value = match self.call_fiber_method(
                                    compiled, fiber, method, values, output, stack, state,
                                ) {
                                    Ok(value) => value,
                                    Err(result) => return result,
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Value::Generator(generator) => {
                                let value = match self.call_generator_method(
                                    compiled, generator, method, values, output, stack, state,
                                ) {
                                    Ok(value) => value,
                                    Err(result) => return result,
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Value::Callable(callable)
                                if method.eq_ignore_ascii_case("__invoke") =>
                            {
                                let value = match self
                                    .call_callable_inner(
                                        compiled,
                                        Value::Callable(callable),
                                        values,
                                        Some(instruction.span),
                                        output,
                                        stack,
                                        state,
                                        false,
                                        None,
                                    )
                                    .return_value
                                {
                                    Some(value) => value,
                                    None => Value::Null,
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Value::Callable(callable) if method.eq_ignore_ascii_case("call") => {
                                let result = self.call_closure_call_method(
                                    compiled,
                                    *callable,
                                    values,
                                    output,
                                    stack,
                                    state,
                                    instruction.span,
                                );
                                if !result.status.is_success()
                                    && let Some(throwable) = state
                                        .pending_throw
                                        .take()
                                        .or_else(|| runtime_error_throwable(&result))
                                {
                                    if let Some(target) = handle_throw(
                                        compiled,
                                        throwable.clone(),
                                        stack,
                                        state,
                                        &mut exception_handlers,
                                        &mut pending_control,
                                    ) {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    return self
                                        .propagate_exception(output, stack, state, throwable);
                                }
                                if !result.status.is_success() {
                                    return result;
                                }
                                let return_value = result.return_value.unwrap_or(Value::Null);
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, return_value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Value::Callable(callable) if method.eq_ignore_ascii_case("bindto") => {
                                let result = self.call_closure_bind_to_method(
                                    compiled,
                                    *callable,
                                    values,
                                    output,
                                    stack,
                                    state,
                                    instruction.span,
                                );
                                if !result.status.is_success()
                                    && let Some(throwable) = state
                                        .pending_throw
                                        .take()
                                        .or_else(|| runtime_error_throwable(&result))
                                {
                                    if let Some(target) = handle_throw(
                                        compiled,
                                        throwable.clone(),
                                        stack,
                                        state,
                                        &mut exception_handlers,
                                        &mut pending_control,
                                    ) {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    return self
                                        .propagate_exception(output, stack, state, throwable);
                                }
                                if !result.status.is_success() {
                                    return result;
                                }
                                let return_value = result.return_value.unwrap_or(Value::Null);
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, return_value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            Value::Object(object) => object,
                            other => {
                                let message = format!(
                                    "E_PHP_VM_METHOD_CALL_NON_OBJECT: Call to a member function {method}() on {}",
                                    value_type_name(&other)
                                );
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                        if internal_throwable_instanceof(&object.class_name_handle(), "throwable")
                            .is_some()
                        {
                            let value = match internal_throwable_method_value(
                                &object,
                                method,
                                values.into_iter().map(|arg| arg.value).collect(),
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_reflection_runtime_class(&object.class_name()) {
                            let values =
                                values.into_iter().map(|arg| arg.value).collect::<Vec<_>>();
                            if normalize_class_name(&object.class_name()) == "reflectionclass"
                                && matches!(
                                    normalize_method_name(method).as_str(),
                                    "newinstance" | "newinstanceargs"
                                )
                            {
                                let result = self.reflection_class_new_instance(
                                    compiled,
                                    &object,
                                    method,
                                    values,
                                    output,
                                    stack,
                                    state,
                                    instruction.span,
                                );
                                if !result.status.is_success() {
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                if result.fiber_suspension.is_some() {
                                    return self.propagate_fiber_suspension(
                                        result,
                                        compiled,
                                        *dst,
                                        block_id,
                                        instruction_index + 1,
                                        &foreach_iterators,
                                        &exception_handlers,
                                        &pending_control,
                                        output,
                                        stack,
                                    );
                                }
                                diagnostics.extend(result.diagnostics);
                                let value = result.return_value.unwrap_or(Value::Null);
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if normalize_class_name(&object.class_name()) == "reflectionattribute"
                                && normalize_method_name(method) == "newinstance"
                            {
                                let result = self.reflection_attribute_new_instance(
                                    compiled,
                                    &object,
                                    output,
                                    stack,
                                    state,
                                    instruction.span,
                                );
                                if !result.status.is_success() {
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                if result.fiber_suspension.is_some() {
                                    return self.propagate_fiber_suspension(
                                        result,
                                        compiled,
                                        *dst,
                                        block_id,
                                        instruction_index + 1,
                                        &foreach_iterators,
                                        &exception_handlers,
                                        &pending_control,
                                        output,
                                        stack,
                                    );
                                }
                                diagnostics.extend(result.diagnostics);
                                let value = result.return_value.unwrap_or(Value::Null);
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if let Err(result) = self.preflight_reflection_class_method(
                                compiled, &object, method, &values, output, stack, state,
                            ) {
                                match self.route_throwable_result(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            let value = match reflection_method_value(
                                compiled,
                                &object,
                                method,
                                values,
                                output,
                                stack,
                                state,
                                runtime_source_span(compiled, instruction.span),
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_php_token_runtime_class(&object.class_name()) {
                            let values =
                                values.into_iter().map(|arg| arg.value).collect::<Vec<_>>();
                            let value = match php_token_method_value(&object, method, values) {
                                Ok(value) => value,
                                Err(message) => {
                                    let result =
                                        self.runtime_error(output, compiled, stack, message);
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if spl_runtime_marker(&object).is_some_and(|class| {
                            is_spl_file_runtime_class(&class)
                                && spl_file_method_is_supported(method)
                        }) && normalize_method_name(method) == "fpassthru"
                            && !spl_file_is_initialized(&object)
                        {
                            let value = match call_spl_file_method_in_state(
                                compiled,
                                state,
                                &object,
                                method,
                                values,
                                &self.options.runtime_context,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
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
                                        if let Err(message) =
                                            validate_method_callable_in_state_scope(
                                                compiled,
                                                state,
                                                scope.as_deref(),
                                                &resolved.class,
                                                &resolved.method,
                                            )
                                        {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        let class_owner = class_owner_in_state(
                                            compiled,
                                            state,
                                            &resolved.class.name,
                                        );
                                        let result = self.execute_function(
                                            &class_owner,
                                            resolved.method.function,
                                            FunctionCall::new(values, Vec::new())
                                                .with_call_site_strict_types(
                                                    compiled.unit().strict_types,
                                                )
                                                .with_call_span(instruction.span)
                                                .with_this(object.clone())
                                                .with_class_context_handles(
                                                    self.class_name_handles(&resolved.class.name)
                                                        .normalized,
                                                    object_called_class_handle(&object),
                                                    self.class_name_handles(&resolved.class.name)
                                                        .normalized,
                                                ),
                                            output,
                                            stack,
                                            state,
                                        );
                                        if !result.status.is_success() {
                                            match self.route_throwable_result(
                                                compiled,
                                                output,
                                                stack,
                                                state,
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
                                        diagnostics.extend(result.diagnostics);
                                        let return_value =
                                            result.return_value.unwrap_or(Value::Null);
                                        if let Err(message) = stack
                                            .frame_mut(frame_index)
                                            .expect("frame is active")
                                            .registers
                                            .set(*dst, return_value)
                                        {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                        continue;
                                    }
                                    Ok(_) => {}
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                }
                            }
                        }
                        if spl_runtime_marker(&object).is_some_and(|class| {
                            is_spl_iterator_runtime_class(&class)
                                && spl_iterator_method_is_supported(method)
                        }) {
                            if spl_runtime_marker(&object).as_deref() == Some("appenditerator")
                                && matches!(
                                    normalize_method_name(method).as_str(),
                                    "append" | "rewind" | "next"
                                )
                            {
                                let value = match self.call_spl_append_iterator_method(
                                    compiled,
                                    &object,
                                    method,
                                    values,
                                    output,
                                    stack,
                                    state,
                                    Some(instruction.span),
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if spl_runtime_marker(&object).as_deref() == Some("limititerator")
                                && spl_limit_iterator_uses_live_inner(&object)
                                && matches!(
                                    normalize_method_name(method).as_str(),
                                    "rewind"
                                        | "valid"
                                        | "current"
                                        | "key"
                                        | "next"
                                        | "seek"
                                        | "getposition"
                                )
                            {
                                let value = match self.call_spl_limit_iterator_method(
                                    compiled, &object, method, values, output, stack, state,
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if spl_runtime_marker(&object)
                                .is_some_and(|class| is_spl_caching_iterator_class(&class))
                                && spl_caching_iterator_uses_live_inner(&object)
                                && matches!(
                                    normalize_method_name(method).as_str(),
                                    "rewind" | "valid" | "current" | "key" | "next"
                                )
                            {
                                let value = match self.call_spl_caching_iterator_method(
                                    compiled, &object, method, values, output, stack, state,
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if spl_runtime_marker(&object).as_deref() == Some("norewinditerator")
                                && matches!(
                                    normalize_method_name(method).as_str(),
                                    "rewind" | "valid" | "current" | "key" | "next"
                                )
                            {
                                let value = match self.call_spl_no_rewind_iterator_method(
                                    compiled, &object, method, values, output, stack, state,
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if spl_runtime_marker(&object)
                                .is_some_and(|class| is_spl_caching_iterator_class(&class))
                                && normalize_method_name(method) == "__tostring"
                            {
                                if let Err(message) = validate_spl_iterator_arg_count(
                                    &object.class_name(),
                                    &values,
                                    0,
                                    0,
                                ) {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                let value = match self.spl_caching_iterator_to_string(
                                    compiled,
                                    &object,
                                    runtime_source_span(compiled, instruction.span),
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(value) => Value::String(value),
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if spl_runtime_marker(&object)
                                .is_some_and(|class| is_spl_caching_iterator_class(&class))
                                && matches!(
                                    normalize_method_name(method).as_str(),
                                    "offsetget" | "offsetexists"
                                )
                            {
                                let result = self.call_spl_caching_iterator_offset_access_method(
                                    compiled,
                                    &object,
                                    method,
                                    values,
                                    Some(instruction.span),
                                    output,
                                    stack,
                                    state,
                                );
                                if !result.status.is_success() || state.pending_throw.is_some() {
                                    let cleanup_values =
                                        match take_method_call_temporary_registers_at_frame(
                                            stack,
                                            frame_index,
                                            object_operand,
                                            args,
                                        ) {
                                            Ok(values) => values,
                                            Err(message) => {
                                                return self.runtime_error(
                                                    output, compiled, stack, message,
                                                );
                                            }
                                        };
                                    for value in &cleanup_values {
                                        release_unrooted_object_handles(value, stack, state);
                                    }
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                let cleanup_values =
                                    match take_method_call_temporary_registers_at_frame(
                                        stack,
                                        frame_index,
                                        object_operand,
                                        args,
                                    ) {
                                        Ok(values) => values,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                for value in &cleanup_values {
                                    release_unrooted_object_handles(value, stack, state);
                                }
                                continue;
                            }
                            let value = if spl_runtime_marker(&object).as_deref()
                                == Some("multipleiterator")
                                && matches!(
                                    normalize_method_name(method).as_str(),
                                    "rewind" | "valid" | "current" | "key" | "next"
                                ) {
                                match self.call_spl_multiple_iterator_method(
                                    compiled, &object, method, values, output, stack, state,
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                }
                            } else if matches!(
                                spl_runtime_marker(&object).as_deref(),
                                Some("recursiveiteratoriterator" | "recursivetreeiterator")
                            ) && matches!(
                                normalize_method_name(method).as_str(),
                                "rewind" | "valid" | "current" | "next"
                            ) {
                                match self.call_spl_recursive_iterator_iterator_method(
                                    compiled,
                                    object,
                                    method,
                                    values,
                                    Some(instruction.span),
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                }
                            } else {
                                match call_spl_iterator_method(
                                    object,
                                    method,
                                    values,
                                    &self.options.runtime_context,
                                ) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        match self.raise_runtime_error(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if spl_runtime_marker(&object).is_some_and(|class| {
                            is_spl_container_runtime_class(&class)
                                && spl_container_method_is_supported(method)
                        }) {
                            let value = match self.call_spl_container_method_with_magic(
                                compiled,
                                object,
                                method,
                                values,
                                Some(instruction.span),
                                output,
                                stack,
                                state,
                            ) {
                                Ok(value) => value,
                                Err(result) => return result,
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if spl_runtime_marker(&object).is_some_and(|class| {
                            is_spl_heap_runtime_class(&class)
                                && spl_heap_method_is_supported(method)
                        }) {
                            let value = match self.call_spl_heap_method(
                                compiled, object, method, values, output, stack, state,
                            ) {
                                Ok(value) => value,
                                Err(SplHeapMethodError::Message(message)) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                Err(SplHeapMethodError::Runtime(result)) => {
                                    match self.route_throwable_result(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_hash_context_runtime_class(&object.class_name())
                            && hash_context_method_is_supported(method)
                        {
                            let value =
                                match self.call_hash_context_method(&object, method, &values) {
                                    Ok(value) => value,
                                    Err(message) => {
                                        match self.raise_runtime_error(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if spl_runtime_marker(&object).is_some_and(|class| {
                            is_spl_file_runtime_class(&class)
                                && spl_file_method_is_supported(method)
                        }) {
                            let value = match call_spl_file_method_in_state(
                                compiled,
                                state,
                                &object,
                                method,
                                values,
                                &self.options.runtime_context,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_date_time_runtime_class(&object.class_name()) {
                            let value = match call_date_time_method(object, method, values) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_sqlite_runtime_class(&object.class_name()) {
                            let value = match call_sqlite_method(
                                &object,
                                method,
                                values,
                                &mut state.builtins.sqlite,
                                &self.options.runtime_context,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_pdo_runtime_class(&object.class_name()) {
                            let value = match call_pdo_method(
                                &object,
                                method,
                                values,
                                &mut state.builtins.sqlite,
                                &mut state.builtins.mysql,
                                &mut state.builtins.postgres,
                                &self.options.runtime_context,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_phar_runtime_class(&object.class_name()) {
                            let value = match call_phar_method(
                                &object,
                                method,
                                values,
                                &self.options.runtime_context,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_zip_runtime_class(&object.class_name()) {
                            if zip_open_uses_empty_file(
                                method,
                                &values,
                                &self.options.runtime_context,
                            ) {
                                emit_zip_open_empty_file_deprecation(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    runtime_source_span(compiled, instruction.span),
                                );
                            }
                            let value = match call_zip_method(
                                &object,
                                method,
                                values,
                                &self.options.runtime_context,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_mysqli_runtime_class(&object.class_name()) {
                            let value = match call_mysqli_method(
                                &object,
                                method,
                                values,
                                &mut state.builtins.mysql,
                                compiled,
                                stack,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_redis_runtime_class(&object.class_name()) {
                            let value = match call_redis_method(
                                &object,
                                method,
                                values,
                                &mut state.builtins.redis_clients,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_memcached_runtime_class(&object.class_name()) {
                            let value = match call_memcached_method(
                                &object,
                                method,
                                values,
                                &mut state.builtins.memcached_clients,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_soap_runtime_class(&object.class_name()) {
                            let value = match call_soap_method(&object, method, values) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_fileinfo_runtime_class(&object.class_name()) {
                            let result = FileinfoMethodCall {
                                vm: self,
                                compiled,
                                object,
                                method,
                                call_span: Some(instruction.span),
                                output,
                                stack,
                                state,
                            }
                            .execute(values);
                            if !result.status.is_success() {
                                return result;
                            }
                            let value = result.return_value.unwrap_or(Value::Null);
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_imagick_runtime_class(&object.class_name()) {
                            let value = match call_imagick_method(&object, method, values) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xsl_runtime_class(&object.class_name()) {
                            let value = match call_xsl_method(&object, method, values) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_xml_runtime_class(&object.class_name()) {
                            let value = match call_xml_runtime_method(
                                &object,
                                method,
                                values.into_iter().map(|arg| arg.value).collect(),
                                &self.options.runtime_context,
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        let receiver_class = normalize_class_name(&object.class_name());
                        let lowered_method = normalize_method_name(method);
                        let scope = current_scope_class(compiled, stack);
                        let epoch = state.lookup_epoch();
                        let method_callsite =
                            method_call_callsite(compiled, function_id, block_id, instruction.id);
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
                        let receiver_class_owner =
                            class_owner_in_state(compiled, state, &class.name);
                        let has_magic_call =
                            class_has_public_magic_call(&receiver_class_owner, &class);
                        let (cached_target, cache_observation) = self
                            .lookup_method_call_inline_cache(
                                compiled,
                                function_id,
                                block_id,
                                instruction.id,
                                &lowered_method,
                                &receiver_class,
                                scope.as_deref(),
                                epoch,
                            );
                        if let Some(target) = cached_target {
                            if !matches!(self.options.jit, JitMode::Cranelift)
                                || method_direct_call_target_is_eligible(
                                    compiled,
                                    state,
                                    &target,
                                    &class,
                                    args,
                                    &values,
                                    has_magic_call,
                                    epoch,
                                )
                            {
                                self.record_counter_method_direct_dispatch_hit();
                                if matches!(self.options.jit, JitMode::Cranelift) {
                                    self.record_counter_direct_call_hit();
                                }
                                let result = self.execute_method_call_target(
                                    compiled,
                                    target,
                                    object.clone(),
                                    values,
                                    Some(instruction.span),
                                    output,
                                    stack,
                                    state,
                                    &running_fiber,
                                    None,
                                );
                                if !result.status.is_success()
                                    && let Some(throwable) = state
                                        .pending_throw
                                        .take()
                                        .or_else(|| runtime_error_throwable(&result))
                                {
                                    if let Some(target) = handle_throw(
                                        compiled,
                                        throwable.clone(),
                                        stack,
                                        state,
                                        &mut exception_handlers,
                                        &mut pending_control,
                                    ) {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    return self
                                        .propagate_exception(output, stack, state, throwable);
                                }
                                if !result.status.is_success() {
                                    return result;
                                }
                                if result.fiber_suspension.is_some() {
                                    return self.propagate_fiber_suspension(
                                        result,
                                        compiled,
                                        *dst,
                                        block_id,
                                        instruction_index + 1,
                                        &foreach_iterators,
                                        &exception_handlers,
                                        &pending_control,
                                        output,
                                        stack,
                                    );
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
                                continue;
                            }
                            self.record_counter_method_direct_dispatch_fallback();
                            if matches!(self.options.jit, JitMode::Cranelift) {
                                self.record_counter_direct_call_fallback();
                            }
                        } else if let Some(observation) = cache_observation {
                            if observation.fallback_call {
                                self.record_counter_method_direct_dispatch_fallback();
                            }
                            if matches!(self.options.jit, JitMode::Cranelift) {
                                self.record_counter_direct_call_fallback();
                            }
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
                                self.record_counter_method_call_profile(
                                    method_call_profile_observation(
                                        &method_callsite,
                                        &lowered_method,
                                        &receiver_class,
                                        &class,
                                        None,
                                        scope.as_deref(),
                                        epoch,
                                        class_has_public_magic_call(&receiver_class_owner, &class),
                                        true,
                                        method_call_args_are_simple_positional(args),
                                        method_call_has_by_ref_argument(args, None, None),
                                        false,
                                        false,
                                        vec!["missing_declared_method"],
                                    ),
                                );
                                if let Some(inner) = spl_inner_iterator_delegation_target(&object)
                                    && (spl_delegation_target_supports_method(
                                        compiled, state, &inner, method,
                                    ) || match self.spl_iterator_chain_has_userland_method(
                                        compiled, state, &inner, method,
                                    ) {
                                        Ok(result) => result,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    })
                                {
                                    let result = self.call_object_method_callable(
                                        compiled,
                                        inner,
                                        method,
                                        values,
                                        Some(instruction.span),
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
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                if class_is_or_extends_internal_throwable_in_state(
                                    compiled,
                                    state,
                                    &class.name,
                                )
                                .unwrap_or(false)
                                {
                                    let value = match internal_throwable_method_value(
                                        &object,
                                        method,
                                        values.into_iter().map(|arg| arg.value).collect(),
                                    ) {
                                        Ok(value) => value,
                                        Err(message) => {
                                            return self
                                                .runtime_error(output, compiled, stack, message);
                                        }
                                    };
                                    if let Err(message) = stack
                                        .frame_mut(frame_index)
                                        .expect("frame is active")
                                        .registers
                                        .set(*dst, value)
                                    {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                let result = match self.call_magic_instance_method(
                                    compiled,
                                    object.clone(),
                                    "__call",
                                    method,
                                    values,
                                    Some(instruction.span),
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(result)) => result,
                                    Ok(None) => {
                                        let message = format!(
                                            "E_PHP_VM_UNKNOWN_METHOD: Call to undefined method {}::{}()",
                                            object.display_name(),
                                            method
                                        );
                                        match self.raise_runtime_error(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                    Err(result) => return result,
                                };
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
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let method_entry = &resolved.method;
                        let declaring_class = &resolved.class;
                        let class_owner =
                            class_owner_in_state(compiled, state, &declaring_class.name);
                        let simple_positional_arguments =
                            method_call_args_are_simple_positional(args);
                        let has_by_ref_argument = method_call_has_by_ref_argument(
                            args,
                            Some(&class_owner),
                            Some(method_entry.function),
                        );
                        let callee_jit_eligible = method_callee_shape_is_jit_eligible(
                            &class_owner,
                            method_entry.function,
                        );
                        // PHP permits calling a static method through an instance
                        // (`$obj->staticMethod()`); it runs as a static call. Fall
                        // through to the normal dispatch — a static body never uses
                        // `$this`, so the bound receiver is inert.
                        self.record_counter_method_call_profile(method_call_profile_observation(
                            &method_callsite,
                            &lowered_method,
                            &receiver_class,
                            &class,
                            Some((&class_owner, declaring_class, method_entry)),
                            scope.as_deref(),
                            epoch,
                            has_magic_call,
                            false,
                            simple_positional_arguments,
                            has_by_ref_argument,
                            callee_jit_eligible,
                            false,
                            Vec::new(),
                        ));
                        if let Some(reason) = method_tiny_inline_rejection_reason(
                            &class_owner,
                            declaring_class,
                            method_entry,
                            args,
                            has_magic_call,
                        ) {
                            self.record_counter_method_tiny_inline_rejection(reason);
                        } else {
                            self.record_counter_method_tiny_inline_candidate();
                        }
                        if let Err(message) = validate_method_callable_in_state_scope(
                            compiled,
                            state,
                            scope.as_deref(),
                            declaring_class,
                            method_entry,
                        ) {
                            if method_entry.flags.is_private || method_entry.flags.is_protected {
                                let result = match self.call_magic_instance_method(
                                    compiled,
                                    object.clone(),
                                    "__call",
                                    method,
                                    values,
                                    Some(instruction.span),
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(result)) => result,
                                    Ok(None) => {
                                        let result =
                                            self.runtime_error(output, compiled, stack, message);
                                        if let Some(throwable) = runtime_error_throwable(&result) {
                                            tag_throwable_location(
                                                &throwable,
                                                compiled,
                                                instruction.span,
                                            );
                                            state.pending_trace =
                                                Some(capture_backtrace_string(compiled, stack));
                                            if let Some(target) = handle_throw(
                                                compiled,
                                                throwable.clone(),
                                                stack,
                                                state,
                                                &mut exception_handlers,
                                                &mut pending_control,
                                            ) {
                                                block_id = target;
                                                continue 'dispatch;
                                            }
                                            return self.propagate_exception(
                                                output, stack, state, throwable,
                                            );
                                        }
                                        return result;
                                    }
                                    Err(result) => return result,
                                };
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
                                continue;
                            }
                            let result = self.runtime_error(output, compiled, stack, message);
                            if let Some(throwable) = runtime_error_throwable(&result) {
                                tag_throwable_location(&throwable, compiled, instruction.span);
                                state.pending_trace =
                                    Some(capture_backtrace_string(compiled, stack));
                                if let Some(target) = handle_throw(
                                    compiled,
                                    throwable.clone(),
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                ) {
                                    block_id = target;
                                    continue 'dispatch;
                                }
                                return self.propagate_exception(output, stack, state, throwable);
                            }
                            return result;
                        }
                        let declaring_dynamic_owner_index =
                            dynamic_class_owner_index_in_state(state, &declaring_class.name);
                        let method_guard = method_call_guard_metadata(
                            &values,
                            &class,
                            declaring_class,
                            method_entry,
                            scope.as_deref(),
                            epoch,
                            has_magic_call,
                            has_by_ref_argument,
                        );
                        let method_target = Rc::new(MethodCallResolvedTarget {
                            declaring_class: declaring_class.name.clone(),
                            function: method_entry.function,
                            guard: method_guard,
                            // The rich arm lacks the owner plan here; hits
                            // keep the legacy re-resolving path.
                            route: None,
                        });
                        let target = match declaring_dynamic_owner_index {
                            Some(unit_index) => MethodCallCacheTarget::DynamicUnit {
                                unit_index,
                                target: method_target.clone(),
                            },
                            None => MethodCallCacheTarget::CurrentUnit {
                                target: method_target,
                            },
                        };
                        let direct_call_cacheable = simple_positional_arguments
                            && !has_by_ref_argument
                            && !has_magic_call
                            && !method_entry.flags.is_static;
                        if self.options.inline_caches.enabled()
                            || !matches!(self.options.jit, JitMode::Cranelift)
                            || direct_call_cacheable
                        {
                            self.install_method_call_inline_cache(
                                compiled,
                                function_id,
                                block_id,
                                instruction.id,
                                &lowered_method,
                                &receiver_class,
                                scope.as_deref(),
                                epoch,
                                target,
                            );
                        }
                        let result = self.execute_function(
                            &class_owner,
                            method_entry.function,
                            FunctionCall::new(values, Vec::new())
                                .with_call_site_strict_types(call_site_strictness(
                                    compiled,
                                    Some(instruction.span),
                                ))
                                .with_call_span(instruction.span)
                                .with_this(object.clone())
                                .with_class_context_handles(
                                    self.class_name_handles(&declaring_class.name).normalized,
                                    self.class_name_handles(&class.display_name).display,
                                    self.class_name_handles(&declaring_class.name).normalized,
                                )
                                .inherit_fiber_context(&running_fiber),
                            output,
                            stack,
                            state,
                        );
                        if (!result.status.is_success() || state.pending_throw.is_some())
                            && let Some(throwable) = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result))
                        {
                            if let Some(target) = handle_throw(
                                compiled,
                                throwable.clone(),
                                stack,
                                state,
                                &mut exception_handlers,
                                &mut pending_control,
                            ) {
                                block_id = target;
                                continue 'dispatch;
                            }
                            return self.propagate_exception(output, stack, state, throwable);
                        }
                        if !result.status.is_success() {
                            return result;
                        }
                        if result.fiber_suspension.is_some() {
                            return self.propagate_fiber_suspension(
                                result,
                                compiled,
                                *dst,
                                block_id,
                                instruction_index + 1,
                                &foreach_iterators,
                                &exception_handlers,
                                &pending_control,
                                output,
                                stack,
                            );
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
                    InstructionKind::CallStaticMethod {
                        dst,
                        class_name,
                        method,
                        args,
                    } => {
                        if is_closure_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let value = match closure_static_method_value(
                                compiled,
                                state,
                                stack,
                                method,
                                values,
                                output,
                                runtime_source_span(compiled, instruction.span),
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_fiber_runtime_class(class_name)
                            && normalize_method_name(method) == "suspend"
                        {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            return match self.suspend_current_fiber(
                                compiled,
                                &running_fiber,
                                values,
                                *dst,
                                block_id,
                                instruction_index + 1,
                                &foreach_iterators,
                                &exception_handlers,
                                &pending_control,
                                output,
                                stack,
                            ) {
                                Ok(result) => result,
                                Err(result) => result,
                            };
                        }
                        if is_php_token_runtime_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let trace_values = values
                                .iter()
                                .map(|arg| arg.value.clone())
                                .collect::<Vec<_>>();
                            let result = match php_token_static_method_value_with_diagnostics(
                                class_name, method, values,
                            ) {
                                Ok(result) => result,
                                Err(message) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            let trace_context = TokenizerStaticCallTraceContext {
                                call: format!("{class_name}::{method}"),
                                values: trace_values,
                                call_span: instruction.span,
                            };
                            if let Err(result) = self.route_tokenizer_static_method_diagnostics(
                                compiled,
                                output,
                                stack,
                                state,
                                result.diagnostics,
                                Some(&trace_context),
                            ) {
                                return result;
                            }
                            let value = result.value;
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if internal_extension_static_class(class_name) {
                            let values =
                                match read_call_args_at_frame(unit, stack, frame_index, args) {
                                    Ok(values) => values,
                                    Err(message) => {
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                };
                            let value = match call_internal_extension_static_method(
                                class_name,
                                method,
                                values.into_iter().map(|arg| arg.value).collect(),
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if let Err(result) = self.autoload_static_class_if_missing(
                            compiled,
                            class_name,
                            instruction.span,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction.id,
                            )),
                            output,
                            stack,
                            state,
                        ) {
                            match self.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let class =
                            match resolve_static_class_name(compiled, state, stack, class_name) {
                                Ok(class) => class,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        if let Err(result) = self.autoload_class_parents_if_missing(
                            compiled, &class, output, stack, state,
                        ) {
                            match self.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let scope =
                            method_lookup_scope_for_static_call(compiled, stack, class_name);
                        let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                            Ok(values) => values,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if class_extends_php_token(compiled, state, &class)
                            && normalize_method_name(method) == "tokenize"
                        {
                            let trace_values = values
                                .iter()
                                .map(|arg| arg.value.clone())
                                .collect::<Vec<_>>();
                            let result = match self.php_token_static_method_value_for_class(
                                compiled, state, &class, class_name, values,
                            ) {
                                Ok(result) => result,
                                Err(PhpTokenStaticMethodError::RuntimeClass(error)) => {
                                    match self.raise_runtime_class_entry_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut exception_handlers,
                                        &mut pending_control,
                                        instruction.span,
                                        error,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                                Err(PhpTokenStaticMethodError::Runtime(message)) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
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
                            let trace_context = TokenizerStaticCallTraceContext {
                                call: format!("{class_name}::{method}"),
                                values: trace_values,
                                call_span: instruction.span,
                            };
                            if let Err(result) = self.route_tokenizer_static_method_diagnostics(
                                compiled,
                                output,
                                stack,
                                state,
                                result.diagnostics,
                                Some(&trace_context),
                            ) {
                                return result;
                            }
                            let value = result.value;
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if normalize_method_name(method) == "__construct"
                            && is_supported_spl_runtime_class(&class.name)
                        {
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
                            let init_values = if is_spl_iterator_runtime_class(&class.name) {
                                match self.prepare_spl_iterator_constructor_args(
                                    compiled,
                                    &class.name,
                                    values,
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(values) => values,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                }
                            } else {
                                values
                            };
                            if let Err(message) = initialize_spl_runtime_subclass_storage(
                                &object,
                                &class.name,
                                init_values,
                                &self.options.runtime_context,
                                Some(&mut state.resources),
                            ) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, Value::Null)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if normalize_method_name(method) == "__construct"
                            && is_instantiable_internal_throwable(&class.name)
                        {
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
                            if let Err(message) = initialize_internal_throwable_object(
                                compiled,
                                stack,
                                &object,
                                &class.name,
                                &values,
                                instruction.span,
                            ) {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, Value::Null)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        if is_supported_spl_runtime_class(&class.name)
                            && let Some(object) = current_this_object(compiled, stack)
                            && spl_runtime_marker(&object).is_some_and(|spl_class| {
                                internal_runtime_class_is_or_extends(&spl_class, &class.name)
                            })
                        {
                            if normalize_class_name(&class.name) == "appenditerator"
                                && matches!(
                                    normalize_method_name(method).as_str(),
                                    "append" | "rewind" | "next"
                                )
                            {
                                let value = match self.call_spl_append_iterator_method(
                                    compiled,
                                    &object,
                                    method,
                                    values.clone(),
                                    output,
                                    stack,
                                    state,
                                    Some(instruction.span),
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if normalize_class_name(&class.name) == "norewinditerator"
                                && matches!(
                                    normalize_method_name(method).as_str(),
                                    "rewind" | "valid" | "current" | "key" | "next"
                                )
                            {
                                let value = match self.call_spl_no_rewind_iterator_method(
                                    compiled,
                                    &object,
                                    method,
                                    values.clone(),
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if matches!(
                                normalize_class_name(&class.name).as_str(),
                                "recursiveiteratoriterator" | "recursivetreeiterator"
                            ) && matches!(
                                normalize_method_name(method).as_str(),
                                "rewind"
                                    | "valid"
                                    | "current"
                                    | "next"
                                    | "callhaschildren"
                                    | "callgetchildren"
                            ) {
                                let value = match self.call_spl_recursive_iterator_iterator_method(
                                    compiled,
                                    object.clone(),
                                    method,
                                    values.clone(),
                                    Some(instruction.span),
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(value) => value,
                                    Err(result) => {
                                        match self.route_throwable_result(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                };
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                            if let Some(result) = call_spl_runtime_method(
                                &object,
                                &class.name,
                                method,
                                values.clone(),
                                &self.options.runtime_context,
                            ) {
                                let value = match result {
                                    Ok(value) => value,
                                    Err(message) => {
                                        match self.raise_runtime_error(
                                            compiled,
                                            output,
                                            stack,
                                            state,
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
                                if let Err(message) = stack
                                    .frame_mut(frame_index)
                                    .expect("frame is active")
                                    .registers
                                    .set(*dst, value)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                continue;
                            }
                        }
                        if class.flags.is_enum
                            && matches!(
                                normalize_method_name(method).as_str(),
                                "cases" | "from" | "tryfrom"
                            )
                        {
                            let value = match enum_static_method(
                                compiled,
                                state,
                                &class,
                                method,
                                values,
                                &|value| self.constant_value(compiled.unit(), value),
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
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
                                if let Some(object) = current_this_object(compiled, stack) {
                                    let result = match self.call_magic_instance_method(
                                        compiled,
                                        object,
                                        "__call",
                                        method,
                                        values.clone(),
                                        Some(instruction.span),
                                        output,
                                        stack,
                                        state,
                                    ) {
                                        Ok(Some(result)) => result,
                                        Ok(None) => {
                                            let called_class = called_class_for_static_call(
                                                compiled, stack, class_name, &class,
                                            );
                                            match self.call_magic_static_method(
                                                compiled,
                                                &class,
                                                "__callStatic",
                                                method,
                                                values,
                                                called_class,
                                                Some(instruction.span),
                                                output,
                                                stack,
                                                state,
                                            ) {
                                                Ok(Some(result)) => result,
                                                Ok(None) => {
                                                    return self.runtime_error(
                                                        output,
                                                        compiled,
                                                        stack,
                                                        format!(
                                                            "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                                                            class.name, method
                                                        ),
                                                    );
                                                }
                                                Err(result) => return result,
                                            }
                                        }
                                        Err(result) => return result,
                                    };
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
                                        return self
                                            .runtime_error(output, compiled, stack, message);
                                    }
                                    continue;
                                }
                                let called_class = called_class_for_static_call(
                                    compiled, stack, class_name, &class,
                                );
                                let result = match self.call_magic_static_method(
                                    compiled,
                                    &class,
                                    "__callStatic",
                                    method,
                                    values,
                                    called_class,
                                    Some(instruction.span),
                                    output,
                                    stack,
                                    state,
                                ) {
                                    Ok(Some(result)) => result,
                                    Ok(None) => {
                                        return self.runtime_error(
                                            output,
                                            compiled,
                                            stack,
                                            format!(
                                                "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                                                class.name, method
                                            ),
                                        );
                                    }
                                    Err(result) => return result,
                                };
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
                                continue;
                            }
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let method_entry = &resolved.method;
                        let declaring_class = &resolved.class;
                        let is_constructor_call = normalize_method_name(method) == "__construct";
                        if internal_throwable_instanceof(&declaring_class.name, "throwable")
                            .is_some()
                            && let Some(object) = current_this_object(compiled, stack)
                            && class_is_a_in_state(
                                compiled,
                                state,
                                &object.class_name(),
                                &declaring_class.name,
                            )
                            .unwrap_or(false)
                        {
                            let value = match internal_throwable_method_value(
                                &object,
                                method,
                                values.into_iter().map(|arg| arg.value).collect(),
                            ) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                            if let Err(message) = stack
                                .frame_mut(frame_index)
                                .expect("frame is active")
                                .registers
                                .set(*dst, value)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            continue;
                        }
                        let bound_this_for_scoped_call = if method_entry.flags.is_static {
                            None
                        } else {
                            let bound_this =
                                current_this_object(compiled, stack).filter(|object| {
                                    class_is_a_in_state(
                                        compiled,
                                        state,
                                        &object.class_name(),
                                        &declaring_class.name,
                                    )
                                    .unwrap_or(false)
                                });
                            if bound_this.is_none() {
                                let message = format!(
                                    "E_PHP_VM_NON_STATIC_METHOD_CALL: Non-static method {}::{}() cannot be called statically",
                                    declaring_class.display_name, method_entry.name
                                );
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                            bound_this
                        };
                        // A private/protected static method that is inaccessible
                        // from the current scope routes to __callStatic (or fails);
                        // one that IS accessible (e.g. a class calling its own
                        // private static method) falls through to a normal call.
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
                            let called_class =
                                called_class_for_static_call(compiled, stack, class_name, &class);
                            let result = match self.call_magic_static_method(
                                compiled,
                                &class,
                                "__callStatic",
                                method,
                                values,
                                called_class,
                                Some(instruction.span),
                                output,
                                stack,
                                state,
                            ) {
                                Ok(Some(result)) => result,
                                Ok(None) => {
                                    match self.raise_runtime_error(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut exception_handlers,
                                        &mut pending_control,
                                        instruction.span,
                                        inaccessible,
                                    ) {
                                        RaiseOutcome::Caught(target) => {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                                Err(result) => return result,
                            };
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
                            continue;
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
                            match self.raise_runtime_error(
                                compiled,
                                output,
                                stack,
                                state,
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
                        let class_owner =
                            class_owner_in_state(compiled, state, &declaring_class.name);
                        let called_class =
                            called_class_for_static_call(compiled, stack, class_name, &class);
                        let mut call = FunctionCall::new(values, Vec::new())
                            .with_call_site_strict_types(call_site_strictness(
                                compiled,
                                Some(instruction.span),
                            ))
                            .with_call_span(instruction.span)
                            .with_class_context_handles(
                                self.class_name_handles(&declaring_class.name).normalized,
                                self.class_name_handles(&called_class).display,
                                self.class_name_handles(&declaring_class.name).normalized,
                            )
                            .inherit_fiber_context(&running_fiber);
                        if let Some(bound_this) = bound_this_for_scoped_call {
                            call = call.with_this(bound_this);
                        }
                        let result = self.execute_function(
                            &class_owner,
                            method_entry.function,
                            call,
                            output,
                            stack,
                            state,
                        );
                        if !result.status.is_success() {
                            match self.route_throwable_result(
                                compiled,
                                output,
                                stack,
                                state,
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
                        if result.fiber_suspension.is_some() {
                            return self.propagate_fiber_suspension(
                                result,
                                compiled,
                                *dst,
                                block_id,
                                instruction_index + 1,
                                &foreach_iterators,
                                &exception_handlers,
                                &pending_control,
                                output,
                                stack,
                            );
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
                    InstructionKind::MakeClosure {
                        dst,
                        function,
                        captures,
                    } => {
                        let captured = match evaluate_closure_captures(unit, stack, captures) {
                            Ok(captures) => captures,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = make_closure_value(compiled, state, stack, *function, captured);
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::CallClosure { dst, callee, args } => {
                        let callee = match read_operand_at_frame(unit, stack, frame_index, *callee)
                        {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let Some(payload) = callee.as_closure() else {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                "E_PHP_VM_CALL_NON_CLOSURE: value is not a closure",
                            );
                        };
                        let values = match read_call_args_at_frame(unit, stack, frame_index, args) {
                            Ok(values) => values,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let mut call = FunctionCall::new(values, payload.captures.clone())
                            .with_call_site_strict_types(call_site_strictness(
                                compiled,
                                Some(instruction.span),
                            ))
                            .inherit_fiber_context(&running_fiber)
                            .with_call_span(instruction.span)
                            .with_error_context(compiled.clone());
                        if let Some(bound_this) = &payload.bound_this {
                            call = call.with_this(bound_this.clone());
                        }
                        if let Some(scope_class) = &payload.context.scope_class {
                            call = call.with_class_context_handles(
                                scope_class.clone(),
                                payload
                                    .context
                                    .called_class
                                    .as_ref()
                                    .unwrap_or(scope_class)
                                    .clone(),
                                payload
                                    .context
                                    .declaring_class
                                    .as_ref()
                                    .unwrap_or(scope_class)
                                    .clone(),
                            );
                        }
                        let closure_owner = closure_owner_for_function(
                            compiled,
                            state,
                            payload.function,
                            payload.debug.as_deref(),
                            payload.context.owner_unit,
                        );
                        let result = self.execute_function(
                            &closure_owner,
                            FunctionId::new(payload.function),
                            call,
                            output,
                            stack,
                            state,
                        );
                        if !result.status.is_success()
                            && let Some(throwable) = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result))
                        {
                            if let Some(target) = handle_throw(
                                compiled,
                                throwable.clone(),
                                stack,
                                state,
                                &mut exception_handlers,
                                &mut pending_control,
                            ) {
                                block_id = target;
                                continue 'dispatch;
                            }
                            return self.propagate_exception(output, stack, state, throwable);
                        }
                        if !result.status.is_success() {
                            return result;
                        }
                        if result.fiber_suspension.is_some() {
                            return self.propagate_fiber_suspension(
                                result,
                                compiled,
                                *dst,
                                block_id,
                                instruction_index + 1,
                                &foreach_iterators,
                                &exception_handlers,
                                &pending_control,
                                output,
                                stack,
                            );
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
                    InstructionKind::ResolveCallable { dst, callable } => {
                        let value = match resolve_callable(compiled, state, callable) {
                            Ok(value) => value,
                            Err(message) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::AcquireCallable { dst, value } => {
                        let value = match read_operand_at_frame(unit, stack, frame_index, *value) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let value = match acquire_callable_value(compiled, state, stack, value) {
                            Ok(value) => value,
                            Err(message) => {
                                match self.raise_runtime_error(
                                    compiled,
                                    output,
                                    stack,
                                    state,
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
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame was pushed")
                            .registers
                            .set(*dst, value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::CallCallable { dst, callee, args } => {
                        let callee = match read_operand_at_frame(unit, stack, frame_index, *callee)
                        {
                            Ok(value) => value,
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
                        let temporary_iterator_arg =
                            callable_iterator_function_temporary_arg_value(&callee, args, &values);
                        let result = self.execute_callable_value_call(
                            compiled,
                            callee,
                            values,
                            function_id,
                            block_id,
                            instruction.id,
                            Some(instruction.span),
                            output,
                            stack,
                            state,
                            &running_fiber,
                        );
                        if !result.status.is_success() {
                            let original_throwable = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result));
                            if let Some((arg_operand, arg_value)) = temporary_iterator_arg.clone() {
                                if let Err(message) =
                                    unset_register_operand_at_frame(stack, frame_index, arg_operand)
                                {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                                let candidates = destructor_candidates_for_value(&arg_value);
                                let rooted_object_ids =
                                    php_visible_non_register_root_object_ids(stack, state);
                                let sweep = self
                                    .run_destructors_for_unreferenced_candidates_with_roots(
                                        compiled,
                                        output,
                                        stack,
                                        state,
                                        &mut exception_handlers,
                                        &mut pending_control,
                                        candidates,
                                        &rooted_object_ids,
                                        None,
                                    );
                                if let Some(outcome) = sweep.outcome {
                                    match outcome {
                                        RaiseOutcome::Caught(target) => {
                                            block_id = target;
                                            continue 'dispatch;
                                        }
                                        RaiseOutcome::Done(result) => return *result,
                                    }
                                }
                            }
                            if let Some(throwable) = original_throwable {
                                state.pending_throw = Some(throwable);
                            }
                        }
                        if !result.status.is_success()
                            && let Some(throwable) = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result))
                        {
                            if let Some(target) = handle_throw(
                                compiled,
                                throwable.clone(),
                                stack,
                                state,
                                &mut exception_handlers,
                                &mut pending_control,
                            ) {
                                block_id = target;
                                continue 'dispatch;
                            }
                            return self.propagate_exception(output, stack, state, throwable);
                        }
                        if !result.status.is_success() {
                            return result;
                        }
                        diagnostics.extend(result.diagnostics);
                        let return_value = result.return_value.unwrap_or(Value::Null);
                        if let Some((arg_operand, arg_value)) = temporary_iterator_arg {
                            if let Err(message) =
                                unset_register_operand_at_frame(stack, frame_index, arg_operand)
                            {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            let mut rooted_object_ids = php_visible_root_object_ids(stack, state);
                            rooted_object_ids.extend(preserved_destructor_object_ids(
                                std::slice::from_ref(&return_value),
                            ));
                            let candidates = destructor_candidates_for_value(&arg_value);
                            let sweep = self
                                .run_destructors_for_unreferenced_candidates_with_roots(
                                    compiled,
                                    output,
                                    stack,
                                    state,
                                    &mut exception_handlers,
                                    &mut pending_control,
                                    candidates,
                                    &rooted_object_ids,
                                    None,
                                );
                            if let Some(outcome) = sweep.outcome {
                                match outcome {
                                    RaiseOutcome::Caught(target) => {
                                        block_id = target;
                                        continue 'dispatch;
                                    }
                                    RaiseOutcome::Done(result) => return *result,
                                }
                            }
                        }
                        if let Err(message) = unset_consumed_call_arg_registers_at_frame(
                            stack,
                            frame_index,
                            args,
                            Some(*dst),
                        ) {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                        if let Err(message) = stack
                            .frame_mut(frame_index)
                            .expect("frame is active")
                            .registers
                            .set(*dst, return_value)
                        {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    }
                    InstructionKind::Pipe {
                        dst,
                        input,
                        callable,
                    } => {
                        let input = match read_operand_at_frame(unit, stack, frame_index, *input) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let callable =
                            match read_operand_at_frame(unit, stack, frame_index, *callable) {
                                Ok(value) => value,
                                Err(message) => {
                                    return self.runtime_error(output, compiled, stack, message);
                                }
                            };
                        let result = self.call_callable(
                            compiled,
                            callable,
                            vec![CallArgument::positional(input)],
                            output,
                            stack,
                            state,
                        );
                        if !result.status.is_success()
                            && let Some(throwable) = state
                                .pending_throw
                                .take()
                                .or_else(|| runtime_error_throwable(&result))
                        {
                            if let Some(target) = handle_throw(
                                compiled,
                                throwable.clone(),
                                stack,
                                state,
                                &mut exception_handlers,
                                &mut pending_control,
                            ) {
                                block_id = target;
                                continue 'dispatch;
                            }
                            return self.propagate_exception(output, stack, state, throwable);
                        }
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
                    InstructionKind::Include { dst, kind, path } => {
                        let path = match read_operand_at_frame(unit, stack, frame_index, *path) {
                            Ok(value) => value,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        let result = self.execute_include(
                            compiled,
                            None,
                            compiled_unit_cache_key(compiled),
                            function_id,
                            block_id,
                            instruction.id,
                            instruction.span,
                            *kind,
                            &path,
                            output,
                            stack,
                            state,
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
                        Err(result) => return result,
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
                            php_runtime::layout_stats::SOURCE_RETURN_REFERENCE_BINDING,
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
                            compiled,
                            output,
                            stack,
                            state,
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
                    self.record_tiering_backedge(function_id, block_id, *target);
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
                        self.record_tiering_backedge(function_id, block_id, next);
                        block_id = next;
                    } else {
                        self.record_tiering_backedge(function_id, block_id, *target);
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
                        self.record_tiering_backedge(function_id, block_id, *target);
                        block_id = *target;
                    } else {
                        let next = match next_block_id(function, block_id) {
                            Ok(block_id) => block_id,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        self.record_tiering_backedge(function_id, block_id, next);
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
                    self.record_tiering_backedge(function_id, block_id, next);
                    block_id = next;
                }
            }
        }
    }
}
