//! Callable dispatch and dynamic call execution.

use super::builtin_intrinsics::try_execute_simple_literal_pcre_builtin;
use super::prelude::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedArg {
    pub(super) value: Value,
    pub(super) reference: Option<ReferenceCell>,
    /// When true, this argument's backtrace entry must hold the live by-ref
    /// cell so the trace observes later writes through the parameter (matching
    /// the reference engine). Set only for a *supplied* by-ref parameter that
    /// bound a cell; a by-ref parameter that fell back to its default keeps a
    /// value snapshot instead, so it stays false.
    pub(super) trace_holds_reference: bool,
}

pub(super) struct PreparedArguments {
    pub(super) args: Vec<PreparedArg>,
    pub(super) frame_args: Vec<Value>,
    pub(super) diagnostics: Vec<RuntimeDiagnostic>,
}

pub(super) struct FunctionCall<'a> {
    pub(super) args: Vec<CallArgument>,
    /// R1.2 fast lane: bare positional argument values for an exact-arity
    /// plain-positional call to a known simple callee, pre-validated by the
    /// dense call arm. When non-empty, `args` is empty and the executor's
    /// direct-bind loop consumes these values straight into the frame locals
    /// — no `CallArgument` construction, no by-ref bookkeeping per argument.
    /// Values are already effective (references dereferenced at read).
    pub(super) positional_values: Vec<Value>,
    pub(super) captures: Vec<ClosureCaptureValue>,
    pub(super) call_span: Option<php_ir::IrSpan>,
    pub(super) call_site_strict_types: Option<bool>,
    pub(super) error_context_compiled: Option<CompiledUnit>,
    pub(super) allow_by_ref_value_warnings: bool,
    pub(super) by_ref_warning_callable_name: Option<String>,
    pub(super) this_value: Option<ObjectRef>,
    pub(super) scope_class: Option<Arc<str>>,
    pub(super) called_class: Option<Arc<str>>,
    pub(super) declaring_class: Option<Arc<str>>,
    pub(super) shared_top_level_locals: Option<&'a mut HashMap<String, Slot>>,
    pub(super) shared_top_level_bind_missing_globals: bool,
    pub(super) running_generator: Option<GeneratorRef>,
    pub(super) resume_continuation: Option<GeneratorContinuation>,
    pub(super) resume_input: Option<GeneratorResumeInput>,
    pub(super) running_fiber: Option<FiberRef>,
    pub(super) resume_fiber_continuation: Option<FiberContinuation>,
    pub(super) resume_fiber_input: Option<FiberResumeInput>,
}

impl FunctionCall<'_> {
    pub(super) fn new(args: Vec<CallArgument>, captures: Vec<ClosureCaptureValue>) -> Self {
        Self {
            args,
            positional_values: Vec::new(),
            captures,
            call_span: None,
            call_site_strict_types: None,
            error_context_compiled: None,
            allow_by_ref_value_warnings: false,
            by_ref_warning_callable_name: None,
            this_value: None,
            scope_class: None,
            called_class: None,
            declaring_class: None,
            shared_top_level_locals: None,
            shared_top_level_bind_missing_globals: false,
            running_generator: None,
            resume_continuation: None,
            resume_input: None,
            running_fiber: None,
            resume_fiber_continuation: None,
            resume_fiber_input: None,
        }
    }

    pub(super) fn with_call_span(mut self, span: php_ir::IrSpan) -> Self {
        self.call_span = Some(span);
        self
    }

    pub(super) fn with_positional_values(mut self, values: Vec<Value>) -> Self {
        debug_assert!(self.args.is_empty());
        self.positional_values = values;
        self
    }

    /// PHP-visible call arity across both argument representations.
    pub(super) fn arg_count(&self) -> usize {
        if self.positional_values.is_empty() {
            self.args.len()
        } else {
            self.positional_values.len()
        }
    }

    pub(super) fn with_optional_call_span(mut self, span: Option<php_ir::IrSpan>) -> Self {
        self.call_span = span;
        self
    }

    pub(super) fn with_call_site_strict_types(mut self, strict_types: bool) -> Self {
        self.call_site_strict_types = Some(strict_types);
        self
    }

    pub(super) fn argument_binding_policy(
        &self,
        fallback_compiled: &CompiledUnit,
    ) -> arguments::ArgumentBindingPolicy {
        // A span's FileId is only meaningful inside the unit that produced
        // it. Resolve per-file strictness against the caller unit when the
        // call carries one; otherwise trust the explicit call-site flag. The
        // fallback-unit span resolution stays last: it is only correct for
        // intra-unit calls, where the binder unit and the span's unit agree.
        let strict_types = self
            .error_context_compiled
            .as_ref()
            .zip(self.call_span)
            .map(|(caller, span)| caller.unit().strict_types_for_span(span))
            .or(self.call_site_strict_types)
            .or_else(|| {
                self.call_span
                    .map(|span| fallback_compiled.unit().strict_types_for_span(span))
            })
            .unwrap_or(fallback_compiled.unit().strict_types);
        arguments::ArgumentBindingPolicy {
            call_site_strict_types: strict_types,
        }
    }

    pub(super) fn with_error_context(mut self, compiled: CompiledUnit) -> Self {
        self.error_context_compiled = Some(compiled);
        self
    }

    pub(super) fn with_by_ref_value_warnings(mut self) -> Self {
        self.allow_by_ref_value_warnings = true;
        self
    }

    pub(super) fn with_optional_by_ref_warning_callable_name(
        mut self,
        name: Option<String>,
    ) -> Self {
        self.by_ref_warning_callable_name = name;
        self
    }

    pub(super) fn running_generator(mut self, generator: GeneratorRef) -> Self {
        self.running_generator = Some(generator);
        self
    }

    pub(super) fn resume_generator(
        mut self,
        continuation: GeneratorContinuation,
        input: GeneratorResumeInput,
    ) -> Self {
        self.resume_continuation = Some(continuation);
        self.resume_input = Some(input);
        self
    }

    pub(super) fn running_fiber(mut self, fiber: FiberRef) -> Self {
        self.running_fiber = Some(fiber);
        self
    }

    pub(super) fn inherit_fiber_context(mut self, fiber: &Option<FiberRef>) -> Self {
        self.running_fiber = fiber.clone();
        self
    }

    pub(super) fn resume_fiber(
        mut self,
        fiber: FiberRef,
        continuation: FiberContinuation,
        input: FiberResumeInput,
    ) -> Self {
        self.running_fiber = Some(fiber);
        self.resume_fiber_continuation = Some(continuation);
        self.resume_fiber_input = Some(input);
        self
    }

    pub(super) fn with_this(mut self, this_value: ObjectRef) -> Self {
        self.this_value = Some(this_value);
        self
    }

    pub(super) fn with_class_context(
        mut self,
        scope_class: impl Into<String>,
        called_class: impl Into<String>,
        declaring_class: impl Into<String>,
    ) -> Self {
        self.scope_class = Some(Arc::from(normalize_class_name(&scope_class.into())));
        self.called_class = Some(Arc::from(display_class_name(&called_class.into())));
        self.declaring_class = Some(Arc::from(normalize_class_name(&declaring_class.into())));
        self
    }

    /// Class-context fast path: the handles are already in the exact
    /// normalized/display form `with_class_context` would produce, so
    /// attaching them is three refcount bumps instead of three fresh
    /// normalizing allocations.
    pub(super) fn with_class_context_handles(
        mut self,
        scope_class: Arc<str>,
        called_class: Arc<str>,
        declaring_class: Arc<str>,
    ) -> Self {
        debug_assert_eq!(normalize_class_name(&scope_class), *scope_class);
        debug_assert_eq!(display_class_name(&called_class), *called_class);
        debug_assert_eq!(normalize_class_name(&declaring_class), *declaring_class);
        self.scope_class = Some(scope_class);
        self.called_class = Some(called_class);
        self.declaring_class = Some(declaring_class);
        self
    }
}

/// Function-invariant frame-shape properties derived from a single body scan.
/// Classifying a call frame otherwise re-scans the whole callee body on every
/// call; these flags are memoized per (unit, function) so repeated calls reuse
/// the scan result. Field semantics mirror `function_has_try_or_finally`,
/// `function_may_hold_destructor_sensitive_value`, and
/// `method_body_has_inline_blocker`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FrameShapeFlags {
    pub(super) has_try_or_finally: bool,
    pub(super) may_hold_destructor_sensitive_value: bool,
    pub(super) has_inline_blocker: bool,
}

/// Precomputes the function-invariant call-shape facts for every function in
/// a unit. Runs once per built execution plan; the per-call dispatch path
/// indexes the result instead of consulting hashed memo caches.
pub(super) fn dense_call_shape_meta_for_unit(
    unit: &php_ir::module::IrUnit,
) -> Vec<DenseCallShapeMeta> {
    unit.functions
        .iter()
        .map(|function| DenseCallShapeMeta {
            has_try_or_finally: function_has_try_or_finally(function),
            may_hold_destructor_sensitive_value: function_may_hold_destructor_sensitive_value(
                function,
            ),
            has_inline_blocker: method_body_has_inline_blocker(function),
            elide_frame_args: !function_body_observes_argument_vector(function),
            params_bind_direct: arguments::params_bind_direct(function),
        })
        .collect()
}

pub(super) fn prepared_function_facts(
    compiled: &CompiledUnit,
    function_id: FunctionId,
    function: &IrFunction,
) -> PreparedFunctionFacts {
    compiled.prepared_function_facts(function_id, || PreparedFunctionFacts {
        observes_argument_vector: function_body_observes_argument_vector(function),
        has_try_or_finally: function_has_try_or_finally(function),
        may_hold_destructor_sensitive_value: function_may_hold_destructor_sensitive_value(function),
        has_inline_blocker: method_body_has_inline_blocker(function),
    })
}

pub(super) fn frame_reuse_call_shape_blocked_reason(
    function: &IrFunction,
    call: &FunctionCall<'_>,
    shape: FrameShapeFlags,
    reuse_class_context: bool,
) -> Option<&'static str> {
    if function.flags.is_generator {
        return Some("generator");
    }
    if call.running_generator.is_some() || call.resume_continuation.is_some() {
        return Some("generator_continuation");
    }
    if call.running_fiber.is_some() || call.resume_fiber_continuation.is_some() {
        return Some("fiber_continuation");
    }
    if function.returns_by_ref {
        return Some("by_ref_return");
    }
    if function.params.iter().any(|param| param.by_ref) {
        return Some("by_ref_param");
    }
    if function.flags.is_closure || !call.captures.is_empty() || !function.captures.is_empty() {
        return Some(
            if call.captures.is_empty() && function.captures.is_empty() {
                "closure"
            } else {
                "closure_capture"
            },
        );
    }
    // Runtime lever R4: class-context calls (methods/constructors/static calls,
    // or any call carrying `$this`/scope/called/declaring class) are reuse-blocked
    // by default. With `reuse_class_context` on, they become reuse-eligible only
    // if they clear every *other* guard below (shared-top-level-locals,
    // try/finally, destructor-sensitive body) and the by-ref-argument guard the
    // caller ORs in afterwards. The reuse/reset path fully resets `$this` and all
    // class-context frame state (`reset_with_activation_context` overwrites
    // scope/called/declaring class and re-zeroes every local/register, so the
    // `$this` local is dropped and re-initialized per call), and teardown drops
    // the prior occupant's values at the same `pop_recycle` point as a fresh frame.
    let has_class_context = call.this_value.is_some()
        || call.scope_class.is_some()
        || call.called_class.is_some()
        || call.declaring_class.is_some()
        || function.flags.is_method;
    if has_class_context && !reuse_class_context {
        return Some("class_context");
    }
    if call.shared_top_level_locals.is_some() {
        return Some("shared_top_level_locals");
    }
    if shape.has_try_or_finally {
        return Some("try_finally");
    }
    if shape.may_hold_destructor_sensitive_value {
        return Some("destructor_sensitive_value");
    }
    None
}

pub(super) fn frame_reuse_prepared_args_blocked_reason(
    prepared_args: &[PreparedArg],
) -> Option<&'static str> {
    prepared_args
        .iter()
        .any(|arg| arg.reference.is_some())
        .then_some("by_ref_argument")
}

pub(super) fn call_frame_layout_class(
    function: &IrFunction,
    call: &FunctionCall<'_>,
    shape: FrameShapeFlags,
) -> &'static str {
    if function.flags.is_generator
        || call.running_generator.is_some()
        || call.resume_continuation.is_some()
    {
        return "generator_frame";
    }
    if call.running_fiber.is_some() || call.resume_fiber_continuation.is_some() {
        return "fiber_frame";
    }
    if call.shared_top_level_locals.is_some() || function.flags.is_top_level {
        return "include_eval_frame";
    }
    if function.flags.is_closure || !call.captures.is_empty() || !function.captures.is_empty() {
        return "closure_frame";
    }
    if call.args.iter().any(|arg| arg.name.is_some())
        || function.params.iter().any(|param| param.variadic)
    {
        return "variadic_named_argument_frame";
    }
    if call.by_ref_warning_callable_name.is_some() {
        return "dynamic_reflection_call_frame";
    }
    if call.this_value.is_some()
        || call.scope_class.is_some()
        || call.called_class.is_some()
        || call.declaring_class.is_some()
        || function.flags.is_method
    {
        return "known_method_frame";
    }
    if function_is_specialized_tiny_leaf_candidate(function, call.arg_count(), shape) {
        return "tiny_leaf_frame";
    }
    "known_function_frame"
}

pub(super) fn function_is_specialized_tiny_leaf_candidate(
    function: &IrFunction,
    supplied_arg_count: usize,
    shape: FrameShapeFlags,
) -> bool {
    !function.flags.is_top_level
        && !function.flags.is_method
        && !function.flags.is_closure
        && !function.flags.is_generator
        && !function.returns_by_ref
        && function.return_type.is_none()
        && function.captures.is_empty()
        && function.params.len() == supplied_arg_count
        && function
            .params
            .iter()
            .all(|param| !param.by_ref && !param.variadic && param.type_.is_none())
        && !shape.has_try_or_finally
        && !shape.may_hold_destructor_sensitive_value
        && !shape.has_inline_blocker
}

pub(super) fn specialized_call_frame_fallback_reason(
    layout: &str,
    frame_reuse_blocked_reason: Option<&'static str>,
    has_by_ref_arg: bool,
) -> Option<&'static str> {
    if layout == "tiny_leaf_frame" && frame_reuse_blocked_reason.is_none() {
        return None;
    }
    match layout {
        "known_method_frame" => Some("class_context"),
        "closure_frame" => Some("closure"),
        "variadic_named_argument_frame" => Some("named_or_variadic"),
        "generator_frame" => Some("generator"),
        "fiber_frame" => Some("fiber"),
        "include_eval_frame" => Some("include_eval"),
        "dynamic_reflection_call_frame" => Some("dynamic_reflection"),
        "known_function_frame" | "tiny_leaf_frame" => frame_reuse_blocked_reason
            .or_else(|| has_by_ref_arg.then_some("by_ref_argument"))
            .or(Some("not_tiny_leaf")),
        _ => frame_reuse_blocked_reason
            .or_else(|| has_by_ref_arg.then_some("by_ref_argument"))
            .or(Some("unsupported_layout")),
    }
}

pub(super) fn function_has_try_or_finally(function: &IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::EnterTry { .. }
                    | InstructionKind::LeaveTry
                    | InstructionKind::EndFinally { .. }
            )
        })
    })
}

pub(super) fn function_may_hold_destructor_sensitive_value(function: &IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::NewObject { .. } | InstructionKind::DynamicNewObject { .. }
            )
        })
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CallArgument {
    pub(super) name: Option<String>,
    pub(super) value: Value,
    pub(super) value_kind: IrCallArgValueKind,
    pub(super) by_ref_local: Option<LocalId>,
    pub(super) by_ref_dim: Option<CallDimTarget>,
    pub(super) by_ref_property: Option<CallPropertyTarget>,
    pub(super) by_ref_property_dim: Option<CallPropertyDimTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CallDimTarget {
    pub(super) local: LocalId,
    pub(super) dims: Vec<ArrayKey>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CallPropertyTarget {
    pub(super) object: ObjectRef,
    pub(super) property: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CallPropertyDimTarget {
    pub(super) object: ObjectRef,
    pub(super) property: String,
    pub(super) dims: Vec<ArrayKey>,
}

impl CallArgument {
    pub(super) fn positional(value: Value) -> Self {
        Self {
            name: None,
            value,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: None,
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        }
    }
}

pub(super) fn function_call_shape(args: &[CallArgument]) -> FunctionCallShape {
    FunctionCallShape {
        arity: args.len().try_into().unwrap_or(u32::MAX),
        named_arguments: args
            .iter()
            .filter_map(|arg| arg.name.clone())
            .collect::<Vec<_>>(),
        by_ref_arguments: CallReferenceMask::from_flags(
            args.iter().map(call_argument_has_by_ref_metadata),
        ),
    }
}

pub(super) fn method_call_shape(args: &[CallArgument]) -> MethodCallShape {
    MethodCallShape {
        arity: args.len().try_into().unwrap_or(u32::MAX),
        named_arguments: args
            .iter()
            .filter_map(|arg| arg.name.clone())
            .collect::<Vec<_>>(),
        by_ref_arguments: CallReferenceMask::from_flags(
            args.iter().map(call_argument_has_by_ref_metadata),
        ),
    }
}

pub(super) fn dense_call_has_by_ref_argument(args: &[CallArgument]) -> bool {
    args.iter().any(call_argument_has_by_ref_metadata)
}

pub(super) fn call_argument_has_by_ref_metadata(arg: &CallArgument) -> bool {
    arg.by_ref_local.is_some()
        || arg.by_ref_dim.is_some()
        || arg.by_ref_property.is_some()
        || arg.by_ref_property_dim.is_some()
}

pub(super) fn function_call_builtin_metadata(
    target: &FunctionCallCacheTarget,
) -> Option<FunctionCallBuiltinMetadata> {
    let FunctionCallCacheTarget::Builtin { kind, name } = target else {
        return None;
    };
    Some(FunctionCallBuiltinMetadata {
        implementation_id: format!("{kind:?}:{name}"),
        version: 1,
    })
}

pub(super) fn function_call_target_is_builtin(target: &FunctionCallCacheTarget) -> bool {
    matches!(target, FunctionCallCacheTarget::Builtin { .. })
}

impl Vm {
    /// Dispatches a runtime callable value (callable string, closure,
    /// invokable, callable array) through the shared function-call target
    /// helpers. Both the rich `CallCallable` arm and the dense
    /// `CallCallable` opcode call this, so their semantics cannot diverge:
    /// plain function-name strings take the function-call inline cache,
    /// everything else routes through the generic callable dispatcher.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_callable_value_call(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        values: Vec<CallArgument>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        running_fiber: &Option<FiberRef>,
    ) -> VmResult {
        match &callee {
            Value::String(name) => {
                let display_name = name.to_string_lossy();
                if display_name.contains("::") {
                    self.call_callable_with_call_span(
                        compiled, callee, values, call_span, output, stack, state,
                    )
                } else {
                    let lowered_name = normalize_function_name(&display_name);
                    let interned_name = PhpString::intern(lowered_name.as_bytes());
                    let epoch = state.lookup_epoch();
                    let call_shape = function_call_shape(&values);
                    let target = self
                        .lookup_function_call_inline_cache(
                            compiled,
                            function_id,
                            block_id,
                            instruction_id,
                            &interned_name,
                            epoch,
                            &call_shape,
                        )
                        .or_else(|| {
                            let resolved =
                                self.resolve_function_call_target(compiled, state, &lowered_name)?;
                            if self.options.inline_caches.enabled()
                                && function_call_target_is_builtin(&resolved)
                            {
                                self.record_counter_builtin_call_ic(false);
                            }
                            self.install_function_call_inline_cache(
                                compiled,
                                function_id,
                                block_id,
                                instruction_id,
                                &interned_name,
                                epoch,
                                call_shape.clone(),
                                resolved.clone(),
                            );
                            Some(resolved)
                        });
                    if let Some(target) = target {
                        self.execute_function_call_target(
                            compiled,
                            target,
                            values,
                            Some((
                                compiled_unit_cache_key(compiled),
                                function_id,
                                block_id,
                                instruction_id,
                            )),
                            call_span,
                            output,
                            stack,
                            state,
                            running_fiber,
                        )
                    } else {
                        let diagnostic = undefined_function(
                            &display_name,
                            RuntimeSourceSpan::default(),
                            stack_trace(compiled, stack),
                        );
                        VmResult::runtime_error_with_diagnostic(
                            output.clone(),
                            diagnostic.message().to_owned(),
                            diagnostic,
                        )
                    }
                }
            }
            _ => self.call_callable_with_call_span(
                compiled, callee, values, call_span, output, stack, state,
            ),
        }
    }

    pub(super) fn call_callable(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        self.call_callable_inner(
            compiled, callee, args, None, output, stack, state, false, None,
        )
    }

    pub(super) fn call_callable_with_call_span(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        self.call_callable_inner(
            compiled, callee, args, call_span, output, stack, state, false, None,
        )
    }

    pub(super) fn call_callable_with_by_ref_value_warnings(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        self.call_callable_inner(
            compiled, callee, args, None, output, stack, state, true, None,
        )
    }

    pub(super) fn call_callable_inner(
        &self,
        compiled: &CompiledUnit,
        callee: Value,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        allow_by_ref_value_warnings: bool,
        by_ref_warning_callable_name: Option<String>,
    ) -> VmResult {
        match callee {
            Value::Callable(callable) => match *callable {
            CallableValue::UserFunction { name } => {
                let make_call = |args, captures| {
                    let call = FunctionCall::new(args, captures)
                        .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                        .with_optional_call_span(call_span);
                    if allow_by_ref_value_warnings {
                        call.with_by_ref_value_warnings()
                    } else {
                        call
                    }
                    .with_optional_by_ref_warning_callable_name(
                        by_ref_warning_callable_name.clone(),
                    )
                };
                if let Some(function) = compiled.lookup_function(&name) {
                    self.execute_function(
                        compiled,
                        function,
                        make_call(args, Vec::new()),
                        output,
                        stack,
                        state,
                    )
                } else if let Some((owner, function)) = dynamic_function_in_state(state, &name) {
                    self.execute_function(
                        &owner,
                        function,
                        make_call(args, Vec::new()),
                        output,
                        stack,
                        state,
                    )
                } else {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_UNRESOLVED_CALLABLE: function {name} is not defined"),
                    )
                }
            }
            CallableValue::Closure(payload) => {
                let mut call = FunctionCall::new(args, payload.captures)
                    .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                    .with_optional_call_span(call_span)
                    .with_error_context(compiled.clone());
                let closure_owner = closure_owner_for_function(
                    compiled,
                    state,
                    payload.function,
                    payload.debug.as_deref(),
                    payload.context.owner_unit,
                );
                if let Some(bound_this) = payload.bound_this
                    && closure_function_has_this_local(&closure_owner, payload.function)
                {
                    call = call.with_this(bound_this);
                }
                if let Some(scope_class) = payload.context.scope_class {
                    call = call.with_class_context_handles(
                        scope_class.clone(),
                        payload
                            .context
                            .called_class
                            .unwrap_or_else(|| scope_class.clone()),
                        payload
                            .context
                            .declaring_class
                            .unwrap_or_else(|| scope_class.clone()),
                    );
                } else if let Some(this_value) = call.this_value.as_ref() {
                    let handles = self.class_name_handles(&this_value.display_name_handle());
                    call = call.with_class_context_handles(
                        handles.normalized.clone(),
                        handles.display,
                        handles.normalized,
                    );
                }
                let call = if allow_by_ref_value_warnings {
                    call.with_by_ref_value_warnings()
                } else {
                    call
                }
                .with_optional_by_ref_warning_callable_name(
                    by_ref_warning_callable_name.clone(),
                );
                self.execute_function(
                    &closure_owner,
                    FunctionId::new(payload.function),
                    call,
                    output,
                    stack,
                    state,
                )
            }
            CallableValue::InternalBuiltin { name } => {
                if is_array_callback_builtin_name(&name) {
                    return self.call_array_callback_builtin(
                        compiled, &name, args, call_span, output, stack, state,
                    );
                }
                if is_array_sort_builtin_name(&name) {
                    return self.call_array_sort_builtin(compiled, &name, args, output, stack, state);
                }
                if is_autoload_builtin_name(&name) || is_symbol_introspection_builtin_name(&name) {
                    return self.call_autoload_builtin(
                        compiled, &name, args, None, call_span, output, stack, state,
                    );
                }
                if is_config_builtin_name(&name) {
                    return self.call_config_builtin(
                        compiled, &name, args, call_span, output, stack, state,
                    );
                }
                if is_error_handling_builtin_name(&name) {
                    return self.call_error_handling_builtin(
                        compiled, &name, args, output, stack, state,
                    );
                }
                if is_output_buffering_builtin_name(&name) {
                    return self.call_output_buffering_builtin(
                        compiled, &name, args, output, stack,
                    );
                }
                if is_environment_builtin_name(&name) {
                    return self.call_environment_builtin(
                        compiled, &name, args, output, stack, state,
                    );
                }
                if is_process_builtin_name(&name) {
                    return self.call_process_builtin(compiled, &name, args, output, stack);
                }
                if is_pcre_callback_builtin_name(&name) {
                    return self.call_pcre_callback_builtin(
                        compiled, &name, args, call_span, output, stack, state,
                    );
                }
                if let Some(result) = self.try_execute_preg_match_start_offset_ascii_call_fast(
                    &name, &args, compiled, stack, state,
                ) {
                    return result;
                }
                let values = match call_builtin_args_to_positional(
                    self, compiled, &name, args, call_span, output, stack, state,
                ) {
                    Ok(values) => values,
                    Err(InternalBuiltinArgError::Message(message)) => {
                        return self.runtime_error(output, compiled, stack, message);
                    }
                    Err(InternalBuiltinArgError::Fatal(result)) => return *result,
                };
                if let Some(result) = self.try_execute_serialization_builtin(
                    compiled, &name, &values, call_span, output, stack, state,
                ) {
                    return result;
                }
                self.execute_internal_registry_builtin(
                    &name,
                    values,
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
            }
            CallableValue::BoundMethod {
                target,
                method,
                scope,
            } => self.call_bound_method_callable(
                compiled, target, &method, scope, args, call_span, output, stack, state,
            ),
            CallableValue::MethodPlaceholder { target } => self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_UNSUPPORTED_METHOD_CALLABLE: method callable {target} is not implemented"
                ),
            ),
            CallableValue::UnresolvedDynamic { target } => self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNRESOLVED_CALLABLE: callable {target} could not be resolved"),
            ),
            },
            Value::String(name) => self.call_named_callable(
                compiled,
                &name.to_string_lossy(),
                args,
                call_span,
                output,
                stack,
                state,
                allow_by_ref_value_warnings,
                by_ref_warning_callable_name.clone(),
            ),
            Value::Array(array) => {
                self.call_array_callable(
                    compiled,
                    &array,
                    args,
                    call_span,
                    output,
                    stack,
                    state,
                    allow_by_ref_value_warnings,
                )
            }
            Value::Object(object) => {
                self.call_object_callable(compiled, object, args, call_span, output, stack, state)
            }
            other => self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_PIPE_RHS_NOT_CALLABLE: {} is not callable",
                    value_type_name(&other)
                ),
            ),
        }
    }

    pub(super) fn call_fiber_callable(
        &self,
        compiled: &CompiledUnit,
        fiber: FiberRef,
        callee: Value,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        match callee {
            Value::Callable(callable) => match *callable {
                CallableValue::UserFunction { name } => {
                    if let Some(function) = compiled.lookup_function(&name) {
                        self.execute_function(
                            compiled,
                            function,
                            FunctionCall::new(args, Vec::new())
                                .with_call_site_strict_types(compiled.unit().strict_types)
                                .running_fiber(fiber),
                            output,
                            stack,
                            state,
                        )
                    } else if let Some((owner, function)) = dynamic_function_in_state(state, &name)
                    {
                        self.execute_function(
                            &owner,
                            function,
                            FunctionCall::new(args, Vec::new())
                                .with_call_site_strict_types(compiled.unit().strict_types)
                                .running_fiber(fiber),
                            output,
                            stack,
                            state,
                        )
                    } else {
                        self.runtime_error(
                            output,
                            compiled,
                            stack,
                            format!("E_PHP_VM_UNRESOLVED_CALLABLE: function {name} is not defined"),
                        )
                    }
                }
                CallableValue::Closure(payload) => {
                    let mut call = FunctionCall::new(args, payload.captures)
                        .with_call_site_strict_types(compiled.unit().strict_types)
                        .running_fiber(fiber)
                        .with_error_context(compiled.clone());
                    let closure_owner = closure_owner_for_function(
                        compiled,
                        state,
                        payload.function,
                        payload.debug.as_deref(),
                        payload.context.owner_unit,
                    );
                    if let Some(bound_this) = payload.bound_this
                        && closure_function_has_this_local(&closure_owner, payload.function)
                    {
                        call = call.with_this(bound_this);
                    }
                    if let Some(scope_class) = payload.context.scope_class {
                        call = call.with_class_context_handles(
                            scope_class.clone(),
                            payload
                                .context
                                .called_class
                                .unwrap_or_else(|| scope_class.clone()),
                            payload
                                .context
                                .declaring_class
                                .unwrap_or_else(|| scope_class.clone()),
                        );
                    } else if let Some(this_value) = call.this_value.as_ref() {
                        let scope_class = this_value.display_name();
                        call = call.with_class_context(
                            scope_class.clone(),
                            scope_class.clone(),
                            scope_class,
                        );
                    }
                    self.execute_function(
                        &closure_owner,
                        FunctionId::new(payload.function),
                        call,
                        output,
                        stack,
                        state,
                    )
                }
                other_callable => self.call_callable(
                    compiled,
                    Value::Callable(Box::new(other_callable)),
                    args,
                    output,
                    stack,
                    state,
                ),
            },
            other => self.call_callable(compiled, other, args, output, stack, state),
        }
    }

    pub(super) fn call_named_callable(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        allow_by_ref_value_warnings: bool,
        by_ref_warning_callable_name: Option<String>,
    ) -> VmResult {
        if let Some((class_name, method)) = name.split_once("::") {
            return self.call_static_method_callable(
                compiled,
                class_name,
                method,
                args,
                call_span,
                output,
                stack,
                state,
                allow_by_ref_value_warnings,
                by_ref_warning_callable_name,
            );
        }
        let normalized = name.to_ascii_lowercase();
        let make_call = |args| {
            let call = FunctionCall::new(args, Vec::new())
                .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                .with_optional_call_span(call_span);
            if allow_by_ref_value_warnings {
                call.with_by_ref_value_warnings()
            } else {
                call
            }
            .with_optional_by_ref_warning_callable_name(by_ref_warning_callable_name.clone())
        };
        if let Some(function) = compiled.lookup_function(&normalized) {
            return self.execute_function(
                compiled,
                function,
                make_call(args),
                output,
                stack,
                state,
            );
        }
        if let Some((owner, function)) = dynamic_function_in_state(state, &normalized) {
            return self.execute_function(&owner, function, make_call(args), output, stack, state);
        }
        if is_autoload_builtin_name(&normalized)
            || is_symbol_introspection_builtin_name(&normalized)
        {
            return self.call_autoload_builtin(
                compiled,
                &normalized,
                args,
                None,
                call_span,
                output,
                stack,
                state,
            );
        }
        if is_config_builtin_name(&normalized) {
            return self.call_config_builtin(
                compiled,
                &normalized,
                args,
                call_span,
                output,
                stack,
                state,
            );
        }
        if is_error_handling_builtin_name(&normalized) {
            return self.call_error_handling_builtin(
                compiled,
                &normalized,
                args,
                output,
                stack,
                state,
            );
        }
        if is_output_buffering_builtin_name(&normalized) {
            return self.call_output_buffering_builtin(compiled, &normalized, args, output, stack);
        }
        if is_environment_builtin_name(&normalized) {
            return self.call_environment_builtin(
                compiled,
                &normalized,
                args,
                output,
                stack,
                state,
            );
        }
        if is_process_builtin_name(&normalized) {
            return self.call_process_builtin(compiled, &normalized, args, output, stack);
        }
        if is_pcre_callback_builtin_name(&normalized) {
            return self.call_pcre_callback_builtin(
                compiled,
                &normalized,
                args,
                call_span,
                output,
                stack,
                state,
            );
        }
        if is_filter_callback_builtin_name(&normalized) {
            return self.call_filter_callback_builtin(
                compiled,
                &normalized,
                args,
                call_span,
                output,
                stack,
                state,
            );
        }
        if is_array_callback_builtin_name(&normalized) {
            return self.call_array_callback_builtin(
                compiled,
                &normalized,
                args,
                call_span,
                output,
                stack,
                state,
            );
        }
        if is_array_sort_builtin_name(&normalized) {
            return self.call_array_sort_builtin(compiled, &normalized, args, output, stack, state);
        }
        if let Some(result) = self.try_execute_preg_match_start_offset_ascii_call_fast(
            &normalized,
            &args,
            compiled,
            stack,
            state,
        ) {
            return result;
        }
        if BuiltinRegistry::new().contains(&normalized) {
            let values = match call_builtin_args_to_positional(
                self,
                compiled,
                &normalized,
                args,
                None,
                output,
                stack,
                state,
            ) {
                Ok(values) => values,
                Err(InternalBuiltinArgError::Message(message)) => {
                    return self.runtime_error(output, compiled, stack, message);
                }
                Err(InternalBuiltinArgError::Fatal(result)) => return *result,
            };
            if let Some(result) = self.try_execute_serialization_builtin(
                compiled,
                &normalized,
                &values,
                call_span,
                output,
                stack,
                state,
            ) {
                return result;
            }
            if let Some(result) =
                try_execute_simple_literal_pcre_builtin(&normalized, &values, state)
            {
                return result;
            }
            return self.execute_internal_registry_builtin(
                &normalized,
                values,
                call_span,
                output,
                stack,
                state,
                compiled,
            );
        }
        self.runtime_error(
            output,
            compiled,
            stack,
            format!("E_PHP_VM_UNRESOLVED_CALLABLE: function {name} is not defined"),
        )
    }

    pub(super) fn resolve_function_call_target(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        name: &str,
    ) -> Option<FunctionCallCacheTarget> {
        if !name.contains('\\')
            && let Some(target) = builtin_function_call_target(name)
        {
            return Some(target);
        }
        if let Some(function) = compiled.lookup_function(name) {
            return Some(FunctionCallCacheTarget::CurrentUnit {
                unit_identity: compiled.cache_identity(),
                function,
            });
        }
        if let Some((unit_index, function)) = dynamic_function_target_in_state(state, name) {
            return Some(FunctionCallCacheTarget::DynamicUnit {
                unit_index,
                unit_identity: dynamic_unit_identity(state, unit_index),
                function,
            });
        }

        if let Some(fallback_name) = namespaced_function_global_fallback(name) {
            if let Some(function) = compiled.lookup_function(fallback_name) {
                return Some(FunctionCallCacheTarget::CurrentUnit {
                    unit_identity: compiled.cache_identity(),
                    function,
                });
            }
            if let Some((unit_index, function)) =
                dynamic_function_target_in_state(state, fallback_name)
            {
                return Some(FunctionCallCacheTarget::DynamicUnit {
                    unit_index,
                    unit_identity: dynamic_unit_identity(state, unit_index),
                    function,
                });
            }
            if let Some(target) = builtin_function_call_target(fallback_name) {
                return Some(target);
            }
        }

        if let Some(target) = builtin_function_call_target(name) {
            return Some(target);
        }
        None
    }

    pub(super) fn execute_function_call_target(
        &self,
        compiled: &CompiledUnit,
        target: FunctionCallCacheTarget,
        args: Vec<CallArgument>,
        call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        running_fiber: &Option<FiberRef>,
    ) -> VmResult {
        match target {
            FunctionCallCacheTarget::CurrentUnit {
                unit_identity,
                function,
            } => {
                if unit_identity != compiled.cache_identity() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_INLINE_CACHE_STALE_CURRENT_UNIT: cached unit identity changed",
                    );
                }
                self.execute_function(
                    compiled,
                    function,
                    FunctionCall::new(args, Vec::new())
                        .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                        .inherit_fiber_context(running_fiber)
                        .with_optional_call_span(call_span),
                    output,
                    stack,
                    state,
                )
            }
            FunctionCallCacheTarget::DynamicUnit {
                unit_index,
                unit_identity,
                function,
            } => {
                let Some(owner) =
                    resolve_dynamic_unit_by_identity(state, unit_index, unit_identity)
                else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_INLINE_CACHE_STALE_DYNAMIC_UNIT: dynamic unit {unit_index} is unavailable"
                        ),
                    );
                };
                self.execute_function(
                    &owner,
                    function,
                    FunctionCall::new(args, Vec::new())
                        .with_call_site_strict_types(call_site_strictness(compiled, call_span))
                        .inherit_fiber_context(running_fiber)
                        .with_optional_call_span(call_span),
                    output,
                    stack,
                    state,
                )
            }
            FunctionCallCacheTarget::Builtin { kind, name } => {
                self.profile_builtin_call(&name, || match kind {
                    FunctionCallBuiltinKind::AutoloadOrSymbolIntrospection => self
                        .call_autoload_builtin(
                            compiled, &name, args, call_site, call_span, output, stack, state,
                        ),
                    FunctionCallBuiltinKind::Config => self.call_config_builtin(
                        compiled, &name, args, call_span, output, stack, state,
                    ),
                    FunctionCallBuiltinKind::ErrorHandling => self
                        .call_error_handling_builtin(compiled, &name, args, output, stack, state),
                    FunctionCallBuiltinKind::OutputBuffering => {
                        self.call_output_buffering_builtin(compiled, &name, args, output, stack)
                    }
                    FunctionCallBuiltinKind::Environment => {
                        self.call_environment_builtin(compiled, &name, args, output, stack, state)
                    }
                    FunctionCallBuiltinKind::Process => {
                        self.call_process_builtin(compiled, &name, args, output, stack)
                    }
                    FunctionCallBuiltinKind::PcreCallback => self.call_pcre_callback_builtin(
                        compiled, &name, args, call_span, output, stack, state,
                    ),
                    FunctionCallBuiltinKind::FilterCallback => self.call_filter_callback_builtin(
                        compiled, &name, args, call_span, output, stack, state,
                    ),
                    FunctionCallBuiltinKind::ArrayCallback => self.call_array_callback_builtin(
                        compiled, &name, args, call_span, output, stack, state,
                    ),
                    FunctionCallBuiltinKind::ArraySort => {
                        self.call_array_sort_builtin(compiled, &name, args, output, stack, state)
                    }
                    FunctionCallBuiltinKind::InternalRegistry => {
                        if let Some(result) = self
                            .try_execute_preg_match_start_offset_ascii_call_fast(
                                &name, &args, compiled, stack, state,
                            )
                        {
                            return result;
                        }
                        let values = match call_builtin_args_to_positional(
                            self, compiled, &name, args, call_span, output, stack, state,
                        ) {
                            Ok(values) => values,
                            Err(InternalBuiltinArgError::Message(message)) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                            Err(InternalBuiltinArgError::Fatal(result)) => return *result,
                        };
                        if let Some(result) = self.try_execute_serialization_builtin(
                            compiled, &name, &values, call_span, output, stack, state,
                        ) {
                            return result;
                        }
                        if let Some(result) =
                            try_execute_simple_literal_pcre_builtin(&name, &values, state)
                        {
                            return result;
                        }
                        self.execute_internal_registry_builtin(
                            &name, values, call_span, output, stack, state, compiled,
                        )
                    }
                })
            }
        }
    }

    pub(super) fn execute_function_with_dense_plan(
        &self,
        compiled: &CompiledUnit,
        owner: &CompiledUnit,
        plan: Option<&DenseExecutionPlan>,
        function: FunctionId,
        call: FunctionCall<'_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        // Copy-and-patch native leaf tier for method-path calls: dense method
        // dispatch executes bodies directly (bypassing `execute_function`), so
        // without this hook a recognized `$this` accessor leaf would never
        // engage on the default engine. Same placement contract as the hook in
        // `execute_function`: before dense dispatch, fall through on `None`.
        // The leaf is compiled against `owner` — the unit whose IR owns the
        // function — exactly like the dense body below.
        #[cfg(feature = "jit-copy-patch")]
        if let Some(ir_function) = owner.unit().functions.get(function.index())
            && let Some(result) = self.try_execute_copy_patch_leaf(
                owner,
                function,
                ir_function,
                &call,
                output,
                stack,
                state,
            )
        {
            return result;
        }
        if let Some(plan) = plan {
            self.record_counter_dense_method_dispatch_attempt();
            // Bodies defined in another unit (an include) execute through
            // that unit's memoized plan; every warmed include already has
            // one in the thread cache, so cross-unit methods stop dropping
            // whole bodies to the rich interpreter.
            let owner_plan_arc;
            let (unit, active_plan) = if owner.ptr_eq(compiled) {
                (compiled, plan)
            } else {
                match self.get_or_build_dense_execution_plan(owner) {
                    Ok(owner_plan) => {
                        owner_plan_arc = owner_plan;
                        (owner, owner_plan_arc.as_ref())
                    }
                    Err(_) => {
                        self.record_counter_dense_method_dispatch_fallback(
                            "owner_plan_unavailable",
                        );
                        return self.execute_function(owner, function, call, output, stack, state);
                    }
                }
            };
            let fallback_reason = if call.resume_continuation.is_some()
                || call.resume_fiber_continuation.is_some()
                || call.running_generator.is_some()
                || call.running_fiber.is_some()
            {
                Some("generator_or_fiber_context")
            } else {
                match active_plan.function_plan(function.index()) {
                    Some(DenseFunctionPlan::Dense) => {
                        if let (Some(dense_function), Some(ir_function)) = (
                            active_plan.unit.functions.get(function.index()),
                            unit.unit().functions.get(function.index()),
                        ) {
                            self.record_counter_dense_method_dispatch_hit();
                            // Record the request-profile boundary here too:
                            // the dense path bypasses `execute_function`, so
                            // without this a densely executed function/method
                            // would silently vanish from the profiler's
                            // per-name attribution.
                            let profile_boundary = self.request_profile_boundary_start();
                            let function_profile = profile_boundary
                                .is_some()
                                .then(|| (ir_function.name.clone(), ir_function.flags.is_method));
                            let result = self.execute_bytecode_function(
                                DenseExecutionRequest {
                                    compiled: unit,
                                    dense: &active_plan.unit,
                                    plan: Some(active_plan),
                                    dense_function,
                                    ir_function,
                                    function_id: function,
                                    call,
                                },
                                output,
                                stack,
                                state,
                            );
                            if let Some((name, is_method)) = function_profile {
                                self.record_counter_function_profile(
                                    &name,
                                    is_method,
                                    profile_boundary,
                                );
                            }
                            return result;
                        }
                        Some("dense_body_missing")
                    }
                    Some(DenseFunctionPlan::RichFallback { reason }) => Some(reason.as_str()),
                    None => Some("plan_missing"),
                }
            };
            if let Some(reason) = fallback_reason {
                self.record_counter_dense_method_dispatch_fallback(reason);
            }
        }
        self.execute_function(owner, function, call, output, stack, state)
    }

    pub(super) fn call_array_callable(
        &self,
        compiled: &CompiledUnit,
        array: &PhpArray,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        allow_by_ref_value_warnings: bool,
    ) -> VmResult {
        if array.len() != 2 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_INVALID_CALLABLE_ARRAY: callable arrays must contain exactly target and method",
            );
        }
        let (Some(target), Some(method)) =
            (array.get(&ArrayKey::Int(0)), array.get(&ArrayKey::Int(1)))
        else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_INVALID_CALLABLE_ARRAY: callable arrays must contain exactly target and method",
            );
        };
        let Some(method) = callable_string_ref(method) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_INVALID_CALLABLE_ARRAY: callable array method must be string",
            );
        };
        match callable_resolve_reference(target.clone()) {
            Value::Object(object) => {
                self.call_object_method_callable(
                    compiled, object, &method, args, call_span, output, stack, state,
                )
            }
            Value::Callable(callable) if method.eq_ignore_ascii_case("__invoke") => {
                self.call_callable_inner(
                    compiled,
                    Value::Callable(callable),
                    args,
                    call_span,
                    output,
                    stack,
                    state,
                    allow_by_ref_value_warnings,
                    Some("Closure::__invoke".to_owned()),
                )
            }
            Value::String(class_name) => self.call_static_method_callable(
                compiled,
                &class_name.to_string_lossy(),
                &method,
                args,
                call_span,
                output,
                stack,
                state,
                allow_by_ref_value_warnings,
                None,
            ),
            other => self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_INVALID_CALLABLE_ARRAY: callable array target must be object or class string, got {}",
                    value_type_name(&other)
                ),
            ),
        }
    }

    pub(super) fn call_closure_call_method(
        &self,
        compiled: &CompiledUnit,
        callable: CallableValue,
        mut args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        span: php_ir::IrSpan,
    ) -> VmResult {
        if args.is_empty() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_TOO_FEW_ARGS: Closure::call expects at least 1 argument, 0 given",
            );
        }
        let new_this = callable_resolve_reference(args.remove(0).value);
        let Value::Object(new_this) = new_this else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_PARAM_TYPE_MISMATCH: Closure::call(): Argument #1 ($newThis) must be of type object, {} given",
                    value_type_name(&new_this)
                ),
            );
        };
        match callable {
            CallableValue::BoundMethod {
                target: CallableMethodTarget::Object(object),
                method,
                scope,
            } => {
                let compatible = class_is_a_in_state(
                    compiled,
                    state,
                    &new_this.class_name(),
                    &object.class_name(),
                )
                .unwrap_or(false);
                if !compatible {
                    if let Err(result) = self.emit_closure_call_bind_warning(
                        compiled,
                        output,
                        stack,
                        state,
                        &object.class_name(),
                        &method,
                        &new_this.class_name(),
                        span,
                    ) {
                        return result;
                    }
                    return VmResult::success_no_output(Some(Value::Null));
                }
                self.call_bound_object_method_callable(
                    compiled,
                    new_this,
                    &method,
                    scope,
                    args,
                    Some(span),
                    output,
                    stack,
                    state,
                )
            }
            callable @ CallableValue::Closure(_) => {
                if is_std_class_object(&new_this) {
                    if let Err(result) = self.emit_closure_internal_scope_bind_warning(
                        compiled,
                        output,
                        stack,
                        state,
                        &new_this.class_name(),
                        span,
                    ) {
                        return result;
                    }
                    return VmResult::success_no_output(Some(Value::Null));
                }
                self.call_callable_inner(
                    compiled,
                    bind_closure_callable_value(callable, Some(new_this)),
                    args,
                    Some(span),
                    output,
                    stack,
                    state,
                    false,
                    Some("Closure::call".to_owned()),
                )
            }
            other => self.call_callable_inner(
                compiled,
                Value::Callable(Box::new(other)),
                args,
                Some(span),
                output,
                stack,
                state,
                false,
                Some("Closure::call".to_owned()),
            ),
        }
    }

    pub(super) fn call_closure_bind_to_method(
        &self,
        compiled: &CompiledUnit,
        callable: CallableValue,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        span: php_ir::IrSpan,
    ) -> VmResult {
        let mut values = match call_args_to_positional("Closure::bindTo", args) {
            Ok(values) => values,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if values.is_empty() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_TOO_FEW_ARGS: Closure::bindTo expects at least 1 argument, 0 given",
            );
        }
        if values.len() > 2 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_TOO_MANY_ARGS: Closure::bindTo expects at most 2 arguments, {} given",
                    values.len()
                ),
            );
        }
        if let Some(scope) = values.get(1) {
            match callable_resolve_reference(scope.clone()) {
                Value::Null | Value::String(_) | Value::Object(_) => {}
                other => {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_PARAM_TYPE_MISMATCH: Closure::bindTo(): Argument #2 ($newScope) must be of type object|string|null, {} given",
                            value_type_name(&other)
                        ),
                    );
                }
            }
        }
        let new_this = callable_resolve_reference(values.remove(0));
        let bound_this = match new_this {
            Value::Null => {
                if callable_closure_should_warn_unbind_this(&callable) {
                    if let Err(result) =
                        self.emit_closure_unbind_this_warning(compiled, output, stack, state, span)
                    {
                        return result;
                    }
                    return VmResult::success_no_output(Some(Value::Null));
                }
                None
            }
            Value::Object(object) => Some(object),
            other => {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_PARAM_TYPE_MISMATCH: Closure::bindTo(): Argument #1 ($newThis) must be of type ?object, {} given",
                        value_type_name(&other)
                    ),
                );
            }
        };
        let value = bind_closure_callable_value(callable, bound_this);
        VmResult::success_no_output(Some(value))
    }

    pub(super) fn emit_closure_internal_scope_bind_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        class_name: &str,
        span: php_ir::IrSpan,
    ) -> Result<(), VmResult> {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_CLOSURE_INTERNAL_SCOPE_BIND_WARNING",
            RuntimeSeverity::Warning,
            format!(
                "Cannot bind closure to scope of internal class {}, this will be an error in PHP 9",
                callable_class_display_name(compiled, state, class_name)
            ),
            runtime_source_span(compiled, span),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Warning),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Warning,
                php_runtime::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn emit_closure_unbind_this_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        span: php_ir::IrSpan,
    ) -> Result<(), VmResult> {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_CLOSURE_UNBIND_THIS_WARNING",
            RuntimeSeverity::Warning,
            "Cannot unbind $this of closure using $this, this will be an error in PHP 9",
            runtime_source_span(compiled, span),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Warning),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Warning,
                php_runtime::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn emit_closure_call_bind_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        declaring_class: &str,
        method: &str,
        target_class: &str,
        span: php_ir::IrSpan,
    ) -> Result<(), VmResult> {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_CLOSURE_CALL_BIND_WARNING",
            RuntimeSeverity::Warning,
            format!(
                "Cannot bind method {}::{}() to object of class {}, this will be an error in PHP 9",
                callable_class_display_name(compiled, state, declaring_class),
                method,
                callable_class_display_name(compiled, state, target_class)
            ),
            runtime_source_span(compiled, span),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Warning),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Warning,
                php_runtime::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }
}
