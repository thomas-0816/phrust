use super::*;

impl Vm {
    #[cfg(not(feature = "jit-cranelift"))]
    pub(super) fn try_execute_jit_leaf(
        &self,
        _compiled: &CompiledUnit,
        _state: &ExecutionState,
        _function_id: FunctionId,
        _function: &IrFunction,
        tier: ExecutionTier,
        _call_shape_supported: bool,
        _args: &[PreparedArg],
    ) -> Option<Value> {
        if tier == ExecutionTier::Jit && matches!(self.options.jit, JitMode::Cranelift) {
            self.record_counter_native_candidate();
            self.record_counter_native_platform_unavailable();
        }
        None
    }

    /// Copy-and-patch native leaf tier (behind the default-on `jit-copy-patch`
    /// feature; disable per process via `PHRUST_JIT_COPY_PATCH=0` or per VM via
    /// `VmOptions::copy_patch_leaf_override`). Runs before the dense-dispatch and
    /// interpreter paths: if the callee is a recognized leaf called with plain
    /// positional value arguments, compile it once (cached), run it natively
    /// over the argument values, and return the result — otherwise `None` to
    /// fall through. An instance-method leaf (the `$this` property
    /// getter/setter shapes) marshals the call's receiver into slot `0` ahead
    /// of the declared parameters (`$this` is local `0` in method IR); the
    /// receiver's presence must match the function's methodness. Closures
    /// (captures), named arguments, by-reference arguments, and arity
    /// mismatches are rejected here; guard failures take the region's side exit
    /// (also `None`), so behavior is identical to interpreting the function.
    #[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
    pub(super) fn try_execute_copy_patch_leaf(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
        call: &FunctionCall<'_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        if !self
            .options
            .copy_patch_leaf_override
            .unwrap_or_else(crate::copy_patch_bridge::copy_patch_leaf_enabled)
        {
            return None;
        }
        // Plain positional value arguments only; the receiver's presence must
        // match the function's methodness (an instance-method leaf needs
        // `$this` for slot 0, a free function must not get one).
        if function.flags.is_method != call.this_value.is_some() || !call.captures.is_empty() {
            return None;
        }
        if call.arg_count() != function.params.len() {
            return None;
        }
        // Named args would misalign positional slots. By-reference *arg* fields
        // (`by_ref_local` etc.) only track a variable arg's source location for
        // potential write-back; they are set for any variable passed positionally
        // and are moot here because the recognizer already rejects functions with
        // by-reference *parameters* — the value is passed by value regardless.
        // (`positional_values` is positional by construction, so only the
        // `CallArgument` form can carry names.)
        if call.args.iter().any(|arg| arg.name.is_some()) {
            return None;
        }
        let leaf = crate::copy_patch_bridge::cached_leaf(
            compiled,
            function_id.raw(),
            function,
            &compiled.unit().constants,
        )?;
        // Buffer slot `i` is marshaled from `params[i]`; a method's `$this`
        // occupies local 0 in method IR, so the receiver leads and the
        // declared parameters follow at their local indices.
        let mut params: Vec<Value> =
            Vec::with_capacity(call.arg_count() + usize::from(call.this_value.is_some()));
        if let Some(this) = call.this_value.as_ref() {
            params.push(Value::Object(this.clone()));
        }
        if call.positional_values.is_empty() {
            params.extend(call.args.iter().map(|arg| arg.value.clone()));
        } else {
            params.extend(call.positional_values.iter().cloned());
        }
        // Return-and-resume call compositions need the VM to drive the
        // suspend/perform-call/re-enter loop rather than a single region run.
        if leaf.resume_plan().is_some() {
            return self.execute_copy_patch_resume_leaf(
                compiled,
                function_id,
                function,
                call,
                &leaf,
                &params,
                output,
                stack,
                state,
            );
        }
        match leaf.run_outcome(&params) {
            crate::copy_patch_bridge::LeafOutcome::Value(value) => {
                Some(VmResult::success_no_output(Some(value)))
            }
            crate::copy_patch_bridge::LeafOutcome::Fallback => None,
            // The native prefix computed the arguments and requested the userland
            // call. The bridge never re-enters the VM; the call runs here, on the
            // identical normal path, so behavior matches the interpreter exactly.
            crate::copy_patch_bridge::LeafOutcome::TailCall { callee_name, args } => self
                .execute_copy_patch_tailcall(
                    compiled,
                    function_id,
                    function,
                    call.call_span,
                    &params,
                    &callee_name,
                    args,
                    &call.running_fiber,
                    output,
                    stack,
                    state,
                ),
        }
    }

    /// Perform a copy-and-patch tail call: resolve `callee_name` exactly as the
    /// interpreter resolves an unqualified `CallFunction`, validate it is a plain
    /// userland function whose by-value arity matches the natively-computed
    /// `args`, then run it through the normal [`Self::execute_function`] path and
    /// return its result faithfully (exceptions/errors included).
    ///
    /// Materializes the leaf's own stack frame around the call so a throwing or
    /// stack-inspecting callee observes the identical call stack (name, arguments,
    /// and call-site spans) it would under the interpreter. The leaf is a free
    /// function whose parameters are all int-by-value, and the native region only
    /// requested the tail call after guarding every parameter as `Int`, so
    /// `leaf_args` are exactly the int values the leaf was called with — no
    /// argument-coercion divergence. The callee pops its own frame on every exit
    /// (return, runtime error, and `propagate_exception`), so popping the leaf
    /// frame afterward keeps the stack balanced, mirroring the interpreter popping
    /// the leaf once its body returns.
    ///
    /// Returns `None` — so the caller falls back to interpreting the *whole* leaf
    /// — when the callee is a builtin, a dynamic miss, a method/closure/generator,
    /// declared by-reference return, or has any by-reference/variadic parameter or
    /// a mismatched arity. A tail call to a userland scalar leaf simply re-enters
    /// `execute_function`, which may itself run natively.
    #[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
    #[allow(clippy::too_many_arguments)]
    fn execute_copy_patch_tailcall(
        &self,
        compiled: &CompiledUnit,
        leaf_function_id: FunctionId,
        leaf_function: &IrFunction,
        leaf_call_span: Option<php_ir::IrSpan>,
        leaf_args: &[Value],
        callee_name: &str,
        args: Vec<Value>,
        running_fiber: &Option<FiberRef>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        let normalized = normalize_function_name(callee_name);
        let (callee_unit, callee_id) =
            match self.resolve_function_call_target(compiled, state, &normalized)? {
                FunctionCallCacheTarget::CurrentUnit {
                    unit_identity,
                    function,
                } if unit_identity == compiled.cache_identity() => (compiled.clone(), function),
                FunctionCallCacheTarget::CurrentUnit { .. } => return None,
                FunctionCallCacheTarget::DynamicUnit {
                    unit_index,
                    unit_identity,
                    function,
                } => (
                    resolve_dynamic_unit_by_identity(state, unit_index, unit_identity)?,
                    function,
                ),
                // A builtin tail call is out of scope; interpret the whole leaf.
                FunctionCallCacheTarget::Builtin { .. } => return None,
            };
        let callee = callee_unit.unit().functions.get(callee_id.index())?;
        let flags = callee.flags;
        if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
            return None;
        }
        if callee.returns_by_ref || callee.params.len() != args.len() {
            return None;
        }
        if callee
            .params
            .iter()
            .any(|param| param.by_ref || param.variadic)
        {
            return None;
        }

        // The tail call's call-site span (where the leaf calls the callee). The
        // leaf is single-block with exactly one `CallFunction` (the tail call), so
        // the last `CallFunction` instruction is it; used for the callee frame's
        // backtrace line.
        let callee_call_span = leaf_function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .rev()
            .find_map(|instruction| match instruction.kind {
                InstructionKind::CallFunction { .. } => Some(instruction.span),
                _ => None,
            });

        // Materialize the leaf's frame (free function: no class scope) with its
        // guaranteed-int arguments, so the callee sees the same stack.
        stack.push_fresh_frame(
            leaf_function_id,
            leaf_function.register_count,
            leaf_function.local_count,
            FrameActivationContext {
                scope_class: None,
                called_class: None,
                declaring_class: None,
                call_span: leaf_call_span,
            },
        );
        if let Some(frame) = stack.current_mut() {
            // Native leaf frames never bind arguments into locals, so the
            // lazy trace reconstruction has no source — keep the eager
            // snapshot here (guaranteed-int args, so the clones are cheap).
            frame.trace_arguments = TraceArguments::Materialized(
                leaf_args
                    .iter()
                    .map(|value| FrameTraceArgument {
                        name: None,
                        value: value.clone(),
                    })
                    .collect(),
            );
            frame.arguments = leaf_args.to_vec();
        }

        let sub_args: Vec<CallArgument> = args.into_iter().map(CallArgument::positional).collect();
        let sub_call = FunctionCall::new(sub_args, Vec::new())
            .with_call_site_strict_types(compiled.unit().strict_types)
            .with_optional_call_span(callee_call_span)
            .inherit_fiber_context(running_fiber);
        let mut result =
            self.execute_function(&callee_unit, callee_id, sub_call, output, stack, state);
        // The interpreter coerces the callee's value against the *leaf's*
        // declared return type at the leaf's `return g(...)` site (weak-mode
        // `"5"` → `int(5)` through `: int`, or the exact `TypeError`); the
        // callee's own return coercion does not subsume it. Mirror that here on
        // the normal-return path only — an exception/exit/suspension result
        // never reaches the leaf's return, so it propagates untouched. The
        // leaf's frame is still on the stack, so a thrown `TypeError`
        // attributes exactly as under the interpreter.
        if result.status.is_success()
            && result.process_exit_code.is_none()
            && result.yielded.is_none()
            && result.fiber_suspension.is_none()
        {
            match coerce_return_value(
                compiled,
                state,
                leaf_function,
                result.return_value.take(),
                self.typecheck_fast_path_context(),
            ) {
                Ok(value) => result.return_value = value,
                Err(message) => {
                    let error = self.runtime_error(output, compiled, stack, message);
                    stack.pop_recycle();
                    return Some(error);
                }
            }
        }
        stack.pop_recycle();
        Some(result)
    }

    /// Drive a return-and-resume call-composition leaf: run the region until
    /// it suspends, perform each requested userland call through the normal
    /// interpreter path, write the `Int` result into the site's slot, and
    /// re-enter the region — repeating until the region completes.
    ///
    /// Soundness contract (mirrors `compile_scalar_int_resume_leaf`):
    ///
    /// - Before the first performed call nothing has run but pure native
    ///   prefix work, so any mismatch (side exit, resolution change) falls
    ///   back to interpreting the whole leaf.
    /// - After a call has been performed, re-running is unsound (the callee's
    ///   side effects happened). Every anomaly past that point is an engine
    ///   invariant violation surfaced as a deterministic runtime error — by
    ///   construction none is reachable: arguments are guarded/proven `Int`,
    ///   callees are compile-time-resolved unit functions whose names cannot
    ///   be legally redeclared, and their declared `: int` return coercion
    ///   guarantees an `Int` result or a throw (which propagates instead of
    ///   resuming).
    /// - Generator/fiber/continuation contexts are rejected up front: a
    ///   suspension inside a callee could otherwise abandon the region with
    ///   the call half-performed. Outside a fiber, `Fiber::suspend()` inside
    ///   the callee is PHP error behavior and propagates as such.
    ///
    /// The leaf's own frame is materialized around the whole loop (exactly
    /// like the tail-call path) so throwing or stack-inspecting callees
    /// observe the interpreter-identical stack, and the final result runs
    /// through the leaf's return-site coercion.
    #[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
    #[allow(clippy::too_many_arguments)]
    fn execute_copy_patch_resume_leaf(
        &self,
        compiled: &CompiledUnit,
        leaf_function_id: FunctionId,
        leaf_function: &IrFunction,
        call: &FunctionCall<'_>,
        leaf: &std::rc::Rc<crate::copy_patch_bridge::NativeLeaf>,
        params: &[Value],
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        use crate::copy_patch_bridge::ResumeStep;

        if call.resume_continuation.is_some()
            || call.resume_fiber_continuation.is_some()
            || call.running_generator.is_some()
            || call.running_fiber.is_some()
        {
            return None;
        }
        let plan = leaf.resume_plan()?;
        let (mut session, mut step) = leaf.begin_resume(params)?;
        let mut frame_pushed = false;
        let mut calls_performed = false;
        let resume_invariant = |this: &Self,
                                output: &mut OutputBuffer,
                                stack: &mut CallStack,
                                detail: &str|
         -> VmResult {
            this.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_COPY_PATCH_RESUME_INVARIANT: {detail}"),
            )
        };
        loop {
            match step {
                ResumeStep::Fallback => {
                    if calls_performed {
                        // Unreachable by construction; never re-run a leaf
                        // whose callee side effects already happened.
                        let result = resume_invariant(
                            self,
                            output,
                            stack,
                            "post-call side exit or unrepresentable result",
                        );
                        stack.pop_recycle();
                        return Some(result);
                    }
                    // Pure native prefix only — interpreting the whole leaf is
                    // sound (no frame was pushed yet).
                    return None;
                }
                ResumeStep::CallRequest { site } => {
                    let expected = plan.targets.get(site).copied()?;
                    let normalized = plan.normalized_names.get(site)?;
                    let resolved = self.resolve_function_call_target(compiled, state, normalized);
                    let matches_expected = matches!(
                        resolved,
                        Some(FunctionCallCacheTarget::CurrentUnit {
                            unit_identity,
                            function,
                        }) if unit_identity == compiled.cache_identity() && function == expected
                    );
                    if !matches_expected {
                        if calls_performed {
                            let result = resume_invariant(
                                self,
                                output,
                                stack,
                                "resume callee resolution changed mid-region",
                            );
                            stack.pop_recycle();
                            return Some(result);
                        }
                        // Nothing ran yet; the interpreter handles whatever
                        // the divergent resolution means.
                        return None;
                    }
                    let Some(args) = leaf.resume_args(&session, site) else {
                        if calls_performed {
                            let result = resume_invariant(
                                self,
                                output,
                                stack,
                                "non-int marshaled argument slot",
                            );
                            stack.pop_recycle();
                            return Some(result);
                        }
                        return None;
                    };
                    if !frame_pushed {
                        // Materialize the leaf's frame around the whole loop
                        // (mirrors `execute_copy_patch_tailcall`).
                        stack.push_fresh_frame(
                            leaf_function_id,
                            leaf_function.register_count,
                            leaf_function.local_count,
                            FrameActivationContext {
                                scope_class: None,
                                called_class: None,
                                declaring_class: None,
                                call_span: call.call_span,
                            },
                        );
                        if let Some(frame) = stack.current_mut() {
                            // Native leaf frame: see the tail-call arm above.
                            frame.trace_arguments = TraceArguments::Materialized(
                                params
                                    .iter()
                                    .map(|value| FrameTraceArgument {
                                        name: None,
                                        value: value.clone(),
                                    })
                                    .collect(),
                            );
                            frame.arguments = params.to_vec();
                        }
                        frame_pushed = true;
                    }
                    let sub_args: Vec<CallArgument> =
                        args.into_iter().map(CallArgument::positional).collect();
                    let sub_call = FunctionCall::new(sub_args, Vec::new())
                        .with_call_site_strict_types(compiled.unit().strict_types)
                        .with_optional_call_span(plan.call_spans.get(site).copied().flatten())
                        .inherit_fiber_context(&call.running_fiber);
                    let result =
                        self.execute_function(compiled, expected, sub_call, output, stack, state);
                    calls_performed = true;
                    if !result.status.is_success()
                        || result.process_exit_code.is_some()
                        || result.yielded.is_some()
                        || result.fiber_suspension.is_some()
                    {
                        // Exception/exit/suspension: the leaf's return never
                        // completes; propagate faithfully.
                        stack.pop_recycle();
                        return Some(result);
                    }
                    let value = result.return_value.unwrap_or(Value::Null);
                    if !matches!(value, Value::Int(_)) {
                        let result = resume_invariant(
                            self,
                            output,
                            stack,
                            "resume callee returned a non-int despite a declared int return",
                        );
                        stack.pop_recycle();
                        return Some(result);
                    }
                    step = leaf.resume(&mut session, site, &value);
                }
                ResumeStep::Value(value) => {
                    // The interpreter coerces at the leaf's return site; mirror
                    // it exactly (identity for the proven-`Int` result, and the
                    // frame is still pushed for error attribution).
                    match coerce_return_value(
                        compiled,
                        state,
                        leaf_function,
                        Some(value),
                        self.typecheck_fast_path_context(),
                    ) {
                        Ok(value) => {
                            if frame_pushed {
                                stack.pop_recycle();
                            }
                            return Some(VmResult::success_no_output(value));
                        }
                        Err(message) => {
                            let result = self.runtime_error(output, compiled, stack, message);
                            if frame_pushed {
                                stack.pop_recycle();
                            }
                            return Some(result);
                        }
                    }
                }
            }
        }
    }

    /// Hosts without the copy-patch emitter always fall back to the interpreter.
    #[cfg(all(feature = "jit-copy-patch", not(all(unix, target_arch = "aarch64"))))]
    pub(super) fn try_execute_copy_patch_leaf(
        &self,
        _compiled: &CompiledUnit,
        _function_id: FunctionId,
        _function: &IrFunction,
        _call: &FunctionCall<'_>,
        _output: &mut OutputBuffer,
        _stack: &mut CallStack,
        _state: &mut ExecutionState,
    ) -> Option<VmResult> {
        None
    }

    #[cfg(feature = "jit-cranelift")]
    // Audited native-tier helper boundary (docs/performance/cranelift/
    // safety-audit.md): reconstitutes Box<Value> pointers produced by JIT
    // helpers for this synchronous call.
    #[allow(unsafe_code)]
    pub(super) fn try_execute_jit_leaf(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        function_id: FunctionId,
        function: &IrFunction,
        tier: ExecutionTier,
        call_shape_supported: bool,
        args: &[PreparedArg],
    ) -> Option<Value> {
        if tier != ExecutionTier::Jit || !self.options.tiering.enabled {
            return None;
        }
        if self.options.jit != JitMode::Cranelift {
            return None;
        }
        self.record_counter_native_candidate();
        if !jit_leaf_call_shape_is_supported(function, call_shape_supported, args) {
            let reason = native_leaf_rejection_reason(function, call_shape_supported, args);
            self.record_counter_native_eligibility_rejection(reason);
            return None;
        }

        let key = JitFunctionKey {
            unit: compiled_unit_cache_key(compiled),
            function: function_id,
        };
        let cache_key = jit_compile_cache_key(function_id, function, &self.options);
        let runtime_layout_epoch = state.lookup_epoch().raw();
        {
            let mut jit = self.jit.borrow_mut();
            let entry = jit.functions.entry(key).or_default();
            if (self.options.jit_blacklist.enabled() && entry.blacklisted) || entry.disabled {
                self.record_counter_jit_tiering_blacklist_rejection();
                return None;
            }
            entry.calls = entry.calls.saturating_add(1);
        }

        let cache_lookup = self
            .jit
            .borrow_mut()
            .lookup_compile_cache(&cache_key, runtime_layout_epoch);
        let handle = match cache_lookup {
            JitCompileCacheLookup::Hit(handle) => {
                self.record_counter_jit_compile_cache_hit();
                if let Some(entry) = self.jit.borrow_mut().functions.get_mut(&key) {
                    entry.compiled = true;
                    entry.handle = Some(handle.clone());
                }
                handle
            }
            JitCompileCacheLookup::Invalidated => {
                self.record_counter_jit_compile_cache_invalidations(1);
                self.record_counter_jit_compile_cache_miss();
                if let Some(entry) = self.jit.borrow_mut().functions.get_mut(&key) {
                    entry.compiled = false;
                    entry.handle = None;
                }
                self.compile_cranelift_jit_leaf(
                    compiled,
                    function_id,
                    function,
                    key,
                    cache_key,
                    runtime_layout_epoch,
                )?
            }
            JitCompileCacheLookup::Miss => {
                self.record_counter_jit_compile_cache_miss();
                self.compile_cranelift_jit_leaf(
                    compiled,
                    function_id,
                    function,
                    key,
                    cache_key,
                    runtime_layout_epoch,
                )?
            }
        };

        if handle.expects_value_metadata() {
            let [object_arg] = args else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_property_load_guard_exit();
                self.record_counter_property_load_slow_call();
                return None;
            };
            let Some(metadata) = handle.property_load_metadata() else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::GuardFailed),
                );
                self.record_counter_jit_guard_failure();
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_property_load_guard_exit();
                self.record_counter_property_load_slow_call();
                return None;
            };
            if let Some(status) =
                property_load_pre_guard_status(compiled, state, &object_arg.value, metadata)
            {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::GuardFailed)
                        .with_status(status),
                );
                self.record_counter_jit_guard_failure();
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_property_load_guard_exit();
                self.record_counter_property_load_slow_call();
                if status == JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT {
                    self.record_counter_property_load_layout_exit();
                }
                return None;
            }
            let value_ptr = &object_arg.value as *const Value as usize;
            let metadata_ptr = metadata as *const php_jit::JitPropertyLoadMetadata as usize;
            match handle.invoke_value_metadata(
                value_ptr,
                metadata_ptr,
                php_jit::JIT_RUNTIME_ABI_HASH,
            ) {
                Ok(value_ptr) if value_ptr != 0 => {
                    // SAFETY: Successful property-load helpers return a pointer
                    // created with `Box::into_raw(Box<Value>)` specifically for
                    // this synchronous VM call.
                    let value = unsafe { *Box::from_raw(value_ptr as *mut Value) };
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    self.record_counter_property_load_fast_hit();
                    self.record_counter_jit_executed();
                    return Some(value);
                }
                Ok(_) => {
                    self.record_jit_side_exit_for_key(
                        key,
                        php_jit::JitSideExit::new(php_jit::SideExitReason::HelperStatus),
                    );
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    self.record_counter_property_load_guard_exit();
                    self.record_counter_property_load_slow_call();
                    return None;
                }
                Err(error) => {
                    let status = error.native_status();
                    let side_exit = match status {
                        Some(
                            JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT
                            | JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT
                            | JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT
                            | JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT,
                        ) => php_jit::JitSideExit::new(php_jit::SideExitReason::GuardFailed)
                            .with_status(status.unwrap()),
                        _ => error.side_exit(),
                    };
                    self.record_jit_side_exit_for_key(key, side_exit);
                    self.record_counter_jit_guard_failure();
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    self.record_counter_property_load_guard_exit();
                    self.record_counter_property_load_slow_call();
                    if status == Some(JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT) {
                        self.record_counter_property_load_layout_exit();
                    }
                    if status == Some(JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT) {
                        self.record_counter_property_load_uninitialized_exit();
                    }
                    return None;
                }
            }
        }

        if handle.expects_value() {
            let [array_arg] = args else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            let value_ptr = &array_arg.value as *const Value as usize;
            match handle.invoke_value(value_ptr, php_jit::JIT_RUNTIME_ABI_HASH) {
                Ok(value) => {
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::PackedForeachIntSum => {
                            self.record_counter_packed_foreach_sum_fast_hit();
                        }
                        php_jit::JitNativeSpecialization::KnownCallStrlen
                        | php_jit::JitNativeSpecialization::KnownCallCount => {
                            self.record_counter_known_call_fast_hit();
                        }
                        php_jit::JitNativeSpecialization::StringConcat
                        | php_jit::JitNativeSpecialization::PropertyLoad
                        | php_jit::JitNativeSpecialization::RecordArrayLookup
                        | php_jit::JitNativeSpecialization::Generic => {}
                    }
                    self.record_counter_jit_executed();
                    return Some(Value::Int(value));
                }
                Err(error) => {
                    let side_exit = error.side_exit();
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::PackedForeachIntSum => {
                            match error.native_status() {
                                Some(status) if status == php_jit::JIT_HELPER_STATUS_OVERFLOW => {
                                    self.record_counter_jit_overflow_exit();
                                    self.record_counter_packed_foreach_sum_overflow_exit();
                                }
                                Some(status)
                                    if status == php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT
                                        || status == php_runtime::PHP_JIT_ARRAY_STATUS_FALLBACK =>
                                {
                                    self.record_counter_packed_foreach_sum_layout_exit();
                                }
                                _ => {}
                            }
                        }
                        php_jit::JitNativeSpecialization::KnownCallStrlen
                        | php_jit::JitNativeSpecialization::KnownCallCount => {
                            self.record_counter_known_call_guard_exit();
                            self.record_counter_known_call_slow_call();
                        }
                        php_jit::JitNativeSpecialization::StringConcat
                        | php_jit::JitNativeSpecialization::PropertyLoad
                        | php_jit::JitNativeSpecialization::RecordArrayLookup
                        | php_jit::JitNativeSpecialization::Generic => {}
                    }
                    self.record_jit_side_exit_for_key(key, side_exit);
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    return None;
                }
            }
        }

        if handle.expects_value_value() {
            let [lhs_arg, rhs_arg] = args else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_string_concat_fast_path(false);
                return None;
            };
            let lhs_ptr = &lhs_arg.value as *const Value as usize;
            let rhs_ptr = &rhs_arg.value as *const Value as usize;
            match handle.invoke_value_value(lhs_ptr, rhs_ptr, php_jit::JIT_RUNTIME_ABI_HASH) {
                Ok(value_ptr) if value_ptr != 0 => {
                    // SAFETY: Successful value/value helpers return a pointer
                    // created with `Box::into_raw(Box<Value>)` specifically for
                    // this synchronous VM call.
                    let value = unsafe { *Box::from_raw(value_ptr as *mut Value) };
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::StringConcat => {
                            self.record_counter_string_concat_fast_path(true);
                        }
                        php_jit::JitNativeSpecialization::RecordArrayLookup => {
                            self.record_counter_record_lookup_fast_hit();
                        }
                        _ => {}
                    }
                    self.record_counter_jit_executed();
                    return Some(value);
                }
                Ok(_) => {
                    self.record_jit_side_exit_for_key(
                        key,
                        php_jit::JitSideExit::new(php_jit::SideExitReason::HelperStatus),
                    );
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    if handle.specialization() == php_jit::JitNativeSpecialization::StringConcat {
                        self.record_counter_string_concat_fast_path(false);
                    }
                    return None;
                }
                Err(error) => {
                    let side_exit = error.side_exit();
                    self.record_jit_side_exit_for_key(key, side_exit);
                    if matches!(
                        error.native_status(),
                        Some(status) if status == php_jit::JIT_HELPER_STATUS_OVERFLOW
                    ) {
                        self.record_counter_jit_overflow_exit();
                    }
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::StringConcat => {
                            self.record_counter_string_concat_fast_path(false);
                        }
                        php_jit::JitNativeSpecialization::RecordArrayLookup => {
                            match error.native_status() {
                                Some(status)
                                    if status
                                        == php_runtime::PHP_JIT_ARRAY_STATUS_KEY_MISS_EXIT =>
                                {
                                    self.record_counter_record_lookup_key_miss_exit();
                                }
                                Some(status)
                                    if status == php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT =>
                                {
                                    self.record_counter_record_lookup_layout_exit();
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    return None;
                }
            }
        }

        if handle.expects_value_i64() {
            let [array_arg, index_arg] = args else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            let Value::Int(index) = index_arg.value else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            let value_ptr = &array_arg.value as *const Value as usize;
            match handle.invoke_value_i64(value_ptr, index, php_jit::JIT_RUNTIME_ABI_HASH) {
                Ok(value) => {
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    self.record_counter_packed_fetch_fast_hit();
                    self.record_counter_jit_executed();
                    return Some(Value::Int(value));
                }
                Err(error) => {
                    let mut side_exit = error.side_exit();
                    match error.native_status() {
                        Some(status) if status == php_runtime::PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT => {
                            self.record_counter_packed_fetch_bounds_exit();
                            side_exit =
                                php_jit::JitSideExit::new(php_jit::SideExitReason::HelperStatus)
                                    .with_status(status);
                        }
                        Some(status) if status == php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT => {
                            self.record_counter_packed_fetch_layout_exit();
                        }
                        _ => {}
                    }
                    self.record_jit_side_exit_for_key(key, side_exit);
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    return None;
                }
            }
        }

        let native_args = match args
            .iter()
            .map(|arg| value_as_jit_int(&arg.value))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(args) => args,
            Err(()) => {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            }
        };
        match handle.invoke_i64(&native_args, php_jit::JIT_RUNTIME_ABI_HASH) {
            Ok(value) => {
                self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                self.record_counter_jit_executed();
                Some(Value::Int(value))
            }
            Err(error) => {
                let side_exit = error.side_exit();
                if side_exit.reason == php_jit::SideExitReason::Overflow {
                    self.record_counter_jit_overflow_exit();
                }
                self.record_jit_side_exit_for_key(key, side_exit);
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                None
            }
        }
    }

    #[cfg(feature = "jit-cranelift")]
    fn compile_cranelift_jit_leaf(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
        key: JitFunctionKey,
        cache_key: JitCompileCacheKey,
        runtime_layout_epoch: u64,
    ) -> Option<php_jit::JitFunctionHandle> {
        if !self.jit_compile_budget_allows_attempt() {
            self.record_counter_jit_tiering_budget_rejection();
            return None;
        }
        self.record_counter_jit_compile_attempt();
        let mut engine = php_jit::JitEngine::with_options(php_jit::JitOptions {
            enabled: true,
            allow_native_execution: true,
        });
        let compile_result = engine.compile_function_with_runtime_helpers(
            compiled.unit(),
            function_id,
            php_jit::JitCompileRequest::new(format!("function.{}", function.name))
                .with_function_name(function.name.clone())
                .with_ir_fingerprint(format!("{:016x}", cache_key.ir_fingerprint)),
            php_jit::JitRuntimeHelperAddresses {
                packed_array_len: jit_array_len_abi as *const () as usize,
                packed_array_fetch_int_slow: jit_array_fetch_int_slow_abi as *const () as usize,
                known_strlen: jit_strlen_known_abi as *const () as usize,
                known_count: jit_count_known_abi as *const () as usize,
                string_concat: jit_concat_string_string_fast as *const () as usize,
                property_load: jit_property_load_monomorphic_fast as *const () as usize,
                record_array_lookup: jit_record_array_lookup_abi as *const () as usize,
            },
        );
        match compile_result {
            Ok(result) if result.status == php_jit::JitCompileStatus::Compiled => {
                let Some(handle) = result.handle else {
                    self.record_jit_compile_failure_for_key(key);
                    self.record_counter_jit_bailout();
                    return None;
                };
                let descriptor = JitCompileDescriptor {
                    function_id: function_id.raw(),
                    function_name: function.name.clone(),
                    ir_fingerprint: format!("{:016x}", cache_key.ir_fingerprint),
                    code_bytes: result.stats.native_code_bytes,
                    compile_time_nanos: result.stats.native_compile_time_nanos,
                    target_isa: cache_key.target_isa.clone(),
                    abi_hash: cache_key.abi_hash,
                    config_hash: cache_key.config_hash,
                };
                {
                    let mut jit = self.jit.borrow_mut();
                    if let Some(entry) = jit.functions.get_mut(&key) {
                        entry.compiled = true;
                        entry.handle = Some(handle.clone());
                    }
                    jit.insert_compile_cache(cache_key, handle.clone(), runtime_layout_epoch);
                }
                self.record_counter_jit_compiled();
                self.record_counter_jit_compile_metadata(
                    result.stats.native_code_bytes,
                    result.stats.native_compile_time_nanos,
                );
                self.record_counter_jit_compile_descriptor(descriptor);
                self.maybe_write_cranelift_clif_dump(compiled, function_id);
                self.record_jit_compile_budget_spent(result.stats.native_compile_time_nanos);
                Some(handle)
            }
            Ok(_) | Err(_) => {
                self.record_jit_compile_failure_for_key(key);
                self.record_counter_jit_bailout();
                None
            }
        }
    }

    pub(super) fn try_execute_bytecode_entry(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> BytecodeEntryAttempt {
        match self.try_execute_dense_function_entry(
            compiled,
            compiled.unit().entry,
            FunctionCall::new(Vec::new(), Vec::new()),
            output,
            stack,
            state,
        ) {
            BytecodeFunctionAttempt::Executed(result, _) => BytecodeEntryAttempt::Executed(result),
            BytecodeFunctionAttempt::Unsupported(message, _) => {
                BytecodeEntryAttempt::Unsupported(message)
            }
        }
    }

    fn dense_execution_artifact_key(&self) -> DenseExecutionArtifactKey {
        DenseExecutionArtifactKey {
            mode: if self.options.execution_format.is_strict_bytecode() {
                DenseExecutionArtifactMode::Strict
            } else {
                DenseExecutionArtifactMode::Mixed
            },
            superinstructions: self.options.superinstructions.is_enabled(),
            profiled_layout: self.options.bytecode_layout.is_profiled(),
            layout_profile_entries: if self.options.bytecode_layout.is_profiled() {
                self.options
                    .bytecode_layout_profile
                    .as_ref()
                    .map(|profile| {
                        profile
                            .block_entries
                            .iter()
                            .map(|(key, value)| (key.clone(), *value))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            },
            dense_jump_threading: self.options.dense_jump_threading.is_enabled(),
        }
    }

    pub(super) fn get_or_build_dense_execution_plan(
        &self,
        compiled: &CompiledUnit,
    ) -> Result<Arc<DenseExecutionPlan>, String> {
        let key = DenseExecutionPlanThreadCacheKey {
            compiled_identity: compiled.cache_identity(),
            artifact: self.dense_execution_artifact_key(),
        };
        if let Some(plan) =
            DENSE_EXECUTION_PLAN_THREAD_CACHE.with(|cache| cache.borrow().get(&key).cloned())
        {
            self.record_counter_dense_execution_plan_cache_hit();
            self.record_counter_dense_execution_plan(plan.as_ref());
            return Ok(plan);
        }

        #[allow(clippy::arc_with_non_send_sync)] // plan sharing predates a Send-safe design
        let plan = Arc::new({
            let mut plan = self.build_dense_execution_plan(compiled)?;
            plan.call_shape_meta = dense_call_shape_meta_for_unit(compiled.unit());
            plan
        });
        DENSE_EXECUTION_PLAN_THREAD_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= DENSE_EXECUTION_PLAN_THREAD_CACHE_MAX {
                cache.clear();
            }
            cache.insert(key, Arc::clone(&plan));
        });
        self.record_counter_dense_execution_plan_cache_miss();
        self.record_counter_dense_execution_plan(plan.as_ref());
        Ok(plan)
    }

    fn build_dense_execution_plan(
        &self,
        compiled: &CompiledUnit,
    ) -> Result<DenseExecutionPlan, String> {
        self.record_counter_bytecode_lower_attempt();
        if !self.options.execution_format.is_strict_bytecode() {
            let mut plan = DenseBytecodeUnit::mixed_plan_from_ir(compiled.unit());
            if let Err(errors) = plan.unit.verify() {
                return Err(format!(
                    "E_PHP_VM_DENSE_BYTECODE_VERIFY: mixed dense bytecode verifier rejected unit with {} error(s)",
                    errors.len()
                ));
            }
            if self.options.superinstructions.is_enabled() {
                let report = plan.unit.select_superinstructions();
                self.record_counter_superinstruction_selection(&report);
                if let Err(errors) = plan.unit.verify() {
                    return Err(format!(
                        "E_PHP_VM_DENSE_SUPERINSTRUCTION_VERIFY: selected mixed dense bytecode failed verification with {} error(s)",
                        errors.len()
                    ));
                }
            }
            if self.options.bytecode_layout.is_profiled() {
                let _report = plan
                    .unit
                    .apply_profiled_layout(self.options.bytecode_layout_profile.as_ref());
                if let Err(errors) = plan.unit.verify() {
                    return Err(format!(
                        "E_PHP_VM_DENSE_LAYOUT_VERIFY: profiled mixed dense bytecode layout failed verification with {} error(s)",
                        errors.len()
                    ));
                }
            }
            if self.options.dense_jump_threading.is_enabled() && plan.unit.has_jump_trampolines() {
                // Verifier-bracketed with rollback: a threading result that
                // fails verification restores the pre-pass unit instead of
                // dropping the whole plan. The trampoline pre-scan keeps the
                // snapshot clone off the common no-trampoline path.
                let snapshot = plan.unit.clone();
                let report = plan.unit.thread_jump_chains();
                if report.threaded_edges > 0 && plan.unit.verify().is_err() {
                    plan.unit = snapshot;
                    self.record_counter_dense_jump_threading(&report, true);
                } else {
                    self.record_counter_dense_jump_threading(&report, false);
                }
            }
            self.record_counter_bytecode_lowered_families(&plan.unit);
            self.record_counter_bytecode_lower_success();
            return Ok(plan);
        }

        let mut dense = DenseBytecodeUnit::lower_from_ir(compiled.unit())
            .map_err(|error| format!("E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED: {}", error.message))?;
        if let Err(errors) = dense.verify() {
            return Err(format!(
                "E_PHP_VM_DENSE_BYTECODE_VERIFY: dense bytecode verifier rejected unit with {} error(s)",
                errors.len()
            ));
        }
        if self.options.superinstructions.is_enabled() {
            let report = dense.select_superinstructions();
            self.record_counter_superinstruction_selection(&report);
            if let Err(errors) = dense.verify() {
                return Err(format!(
                    "E_PHP_VM_DENSE_SUPERINSTRUCTION_VERIFY: selected dense bytecode failed verification with {} error(s)",
                    errors.len()
                ));
            }
        }
        if self.options.bytecode_layout.is_profiled() {
            let _report =
                dense.apply_profiled_layout(self.options.bytecode_layout_profile.as_ref());
            if let Err(errors) = dense.verify() {
                return Err(format!(
                    "E_PHP_VM_DENSE_LAYOUT_VERIFY: profiled dense bytecode layout failed verification with {} error(s)",
                    errors.len()
                ));
            }
        }
        if self.options.dense_jump_threading.is_enabled() && dense.has_jump_trampolines() {
            let snapshot = dense.clone();
            let report = dense.thread_jump_chains();
            if report.threaded_edges > 0 && dense.verify().is_err() {
                dense = snapshot;
                self.record_counter_dense_jump_threading(&report, true);
            } else {
                self.record_counter_dense_jump_threading(&report, false);
            }
        }
        self.record_counter_bytecode_lowered_families(&dense);
        self.record_counter_bytecode_lower_success();
        let functions = dense
            .functions
            .iter()
            .map(|_| DenseFunctionPlan::Dense)
            .collect();
        Ok(DenseExecutionPlan {
            unit: dense,
            functions,
            call_shape_meta: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_execute_dense_function_entry<'a>(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        call: FunctionCall<'a>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> BytecodeFunctionAttempt<'a> {
        let mut call = Some(call);
        if self.options.trace || self.options.trace_runtime {
            return BytecodeFunctionAttempt::Unsupported(
                "E_PHP_VM_DENSE_BYTECODE_TRACE_UNSUPPORTED: dense bytecode execution does not support tracing yet"
                    .to_string(),
                call.expect("call should be available before execution starts"),
            );
        }
        let Some(ir_function) = compiled.unit().functions.get(function_id.index()) else {
            return BytecodeFunctionAttempt::Unsupported(
                "E_PHP_VM_DENSE_BYTECODE_ENTRY: IR entry function is missing".to_string(),
                call.expect("call should be available before execution starts"),
            );
        };
        let plan = match self.get_or_build_dense_execution_plan(compiled) {
            Ok(plan) => plan,
            Err(message) => {
                return BytecodeFunctionAttempt::Unsupported(
                    message,
                    call.expect("call should be available before execution starts"),
                );
            }
        };
        match plan.function_plan(function_id.index()) {
            Some(DenseFunctionPlan::Dense) => {
                let Some(dense_function) = plan.unit.functions.get(function_id.index()) else {
                    return BytecodeFunctionAttempt::Unsupported(
                        "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense bytecode entry function is missing"
                            .to_string(),
                        call.expect("call should be available before execution starts"),
                    );
                };
                BytecodeFunctionAttempt::Executed(
                    Box::new(self.execute_bytecode_function(
                        DenseExecutionRequest {
                            compiled,
                            dense: &plan.unit,
                            plan: Some(plan.as_ref()),
                            dense_function,
                            ir_function,
                            function_id,
                            call: call.take().expect("call should be consumed exactly once"),
                        },
                        output,
                        stack,
                        state,
                    )),
                    BytecodeFunctionTier::Dense,
                )
            }
            Some(DenseFunctionPlan::RichFallback { reason }) => {
                self.record_counter_rich_fallback_function_executed(reason, &ir_function.name);
                BytecodeFunctionAttempt::Executed(
                    Box::new(self.execute_function(
                        compiled,
                        function_id,
                        call.take().expect("call should be consumed exactly once"),
                        output,
                        stack,
                        state,
                    )),
                    BytecodeFunctionTier::RichFallback(reason.clone()),
                )
            }
            None => BytecodeFunctionAttempt::Unsupported(
                "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense execution plan entry is missing".to_string(),
                call.expect("call should be available before execution starts"),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_execute_cached_dense_function_dispatch<'a>(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
        call: FunctionCall<'a>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> CachedDenseFunctionDispatch<'a> {
        if !self.options.execution_format.attempts_bytecode()
            || self.options.trace
            || self.options.trace_runtime
            || function.flags.is_generator
            || call.resume_continuation.is_some()
            || call.resume_fiber_continuation.is_some()
            || call.running_generator.is_some()
            || call.running_fiber.is_some()
        {
            return CachedDenseFunctionDispatch::Continue(call);
        }

        let plan = match self.get_or_build_dense_execution_plan(compiled) {
            Ok(plan) => plan,
            Err(message) => {
                let reason = dense_bytecode_unsupported_reason(&message);
                self.record_counter_bytecode_unsupported_reason(reason);
                if self.options.execution_format.is_strict_bytecode() {
                    return CachedDenseFunctionDispatch::Executed(Box::new(VmResult::unsupported(
                        output.clone(),
                        message,
                    )));
                }
                self.record_counter_bytecode_unsupported_fallback();
                self.record_counter_bytecode_auto_fallback_reason(reason);
                return CachedDenseFunctionDispatch::Continue(call);
            }
        };

        match plan.function_plan(function_id.index()) {
            Some(DenseFunctionPlan::Dense) => {
                let Some(dense_function) = plan.unit.functions.get(function_id.index()) else {
                    let message =
                        "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense bytecode function is missing"
                            .to_string();
                    if self.options.execution_format.is_strict_bytecode() {
                        return CachedDenseFunctionDispatch::Executed(Box::new(
                            VmResult::unsupported(output.clone(), message),
                        ));
                    }
                    self.record_counter_bytecode_unsupported_reason(
                        dense_bytecode_unsupported_reason(&message),
                    );
                    return CachedDenseFunctionDispatch::Continue(call);
                };
                CachedDenseFunctionDispatch::Executed(Box::new(self.execute_bytecode_function(
                    DenseExecutionRequest {
                        compiled,
                        dense: &plan.unit,
                        plan: Some(plan.as_ref()),
                        dense_function,
                        ir_function: function,
                        function_id,
                        call,
                    },
                    output,
                    stack,
                    state,
                )))
            }
            Some(DenseFunctionPlan::RichFallback { .. }) => {
                CachedDenseFunctionDispatch::Continue(call)
            }
            None => {
                let message =
                    "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense execution plan entry is missing"
                        .to_string();
                if self.options.execution_format.is_strict_bytecode() {
                    CachedDenseFunctionDispatch::Executed(Box::new(VmResult::unsupported(
                        output.clone(),
                        message,
                    )))
                } else {
                    self.record_counter_bytecode_unsupported_reason(
                        dense_bytecode_unsupported_reason(&message),
                    );
                    CachedDenseFunctionDispatch::Continue(call)
                }
            }
        }
    }
}
