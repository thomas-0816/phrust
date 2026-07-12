use super::prelude::*;
use crate::error::VmError;

pub(super) fn call_args_to_positional(
    function: &str,
    args: Vec<CallArgument>,
) -> Result<Vec<Value>, String> {
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        if let Some(name) = arg.name {
            return Err(format!(
                "E_PHP_VM_UNKNOWN_NAMED_ARG: function {function} has no builtin parameter ${name}"
            ));
        }
        values.push(arg.value);
    }
    Ok(values)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ArgumentBindingPolicy {
    pub(super) call_site_strict_types: bool,
}

pub(super) struct ArgumentBinder<'a, 'vm> {
    compiled: &'a CompiledUnit,
    function: &'a IrFunction,
    stack: &'a mut CallStack,
    state: &'a ExecutionState,
    typecheck: TypecheckFastPathContext<'vm>,
    policy: ArgumentBindingPolicy,
}

impl<'a, 'vm> ArgumentBinder<'a, 'vm> {
    pub(super) fn new(
        compiled: &'a CompiledUnit,
        function: &'a IrFunction,
        stack: &'a mut CallStack,
        state: &'a ExecutionState,
        typecheck: TypecheckFastPathContext<'vm>,
        policy: ArgumentBindingPolicy,
    ) -> Self {
        Self {
            compiled,
            function,
            stack,
            state,
            typecheck,
            policy,
        }
    }

    pub(super) fn bind(
        &mut self,
        args: Vec<CallArgument>,
        allow_by_ref_value_warnings: bool,
        call_span: Option<php_ir::IrSpan>,
        by_ref_warning_callable_name: Option<&str>,
        elide_frame_args: bool,
    ) -> Result<PreparedArguments, VmError> {
        let compiled = self.compiled;
        let function = self.function;
        let stack = &mut *self.stack;
        let state = self.state;
        let typecheck = self.typecheck;
        let policy = self.policy;

        bind_arguments(
            compiled,
            function,
            args,
            stack,
            state,
            typecheck,
            policy,
            allow_by_ref_value_warnings,
            call_span,
            by_ref_warning_callable_name,
            elide_frame_args,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_arguments(
    compiled: &CompiledUnit,
    function: &IrFunction,
    args: Vec<CallArgument>,
    stack: &mut CallStack,
    state: &ExecutionState,
    typecheck: TypecheckFastPathContext<'_>,
    policy: ArgumentBindingPolicy,
    allow_by_ref_value_warnings: bool,
    call_span: Option<php_ir::IrSpan>,
    by_ref_warning_callable_name: Option<&str>,
    elide_frame_args: bool,
) -> Result<PreparedArguments, VmError> {
    ArgumentBinder::new(compiled, function, stack, state, typecheck, policy).bind(
        args,
        allow_by_ref_value_warnings,
        call_span,
        by_ref_warning_callable_name,
        elide_frame_args,
    )
}

/// The plain-positional, exact-arity, by-value call shape that both the
/// `bind_arguments` fast path and the dense executor's direct-to-locals fast
/// path recognize. Sharing one predicate keeps the two in lockstep so they can
/// never diverge on which calls skip the general machinery.
///
/// For this shape the general path and the fast path are behavior-identical:
/// both hand the caller *raw* (uncoerced) by-value arguments and let the shared
/// post-binding loop apply parameter-type coercion, strict-types enforcement,
/// and `TypeError` reporting via `coerce_or_check_param_type` at the identical
/// program point. Untyped params (with or without a default) and typed params
/// without a default both bind by position and are coerced there; exact arity
/// means a supplied param never consults its default, so typed-without-default
/// is identical to the general path here. Only typed-with-default stays on the
/// general path, conservatively.
///
/// The shape guarantees: no by-ref references are produced (all params are
/// by-value), no defaults are consulted (exact arity supplies every param), no
/// variadic expansion, and no named/unpacked arguments. Callers must still
/// require `elide_frame_args` before taking either fast path.
pub(super) fn is_direct_bind_fast_shape(function: &IrFunction, args: &[CallArgument]) -> bool {
    args.len() == function.params.len()
        && args.iter().all(|arg| {
            arg.name.is_none()
                && !matches!(arg.value_kind, IrCallArgValueKind::ByRefLocationPlaceholder)
        })
        && params_bind_direct(function)
}

/// The parameter-side half of [`is_direct_bind_fast_shape`]: every param is
/// by-value, non-variadic, and not typed-with-default. The dense call arm's
/// bare-value fast lane checks this against the *callee* before any argument
/// exists, pairing it with the dense-metadata equivalent of the argument-side
/// half; keep the two predicates in lockstep.
pub(super) fn params_bind_direct(function: &IrFunction) -> bool {
    function.params.iter().all(|param| {
        !param.variadic && !param.by_ref && (param.type_.is_none() || param.default.is_none())
    })
}

#[allow(clippy::too_many_arguments)]
fn bind_arguments(
    compiled: &CompiledUnit,
    function: &IrFunction,
    args: Vec<CallArgument>,
    stack: &mut CallStack,
    state: &ExecutionState,
    typecheck: TypecheckFastPathContext<'_>,
    policy: ArgumentBindingPolicy,
    allow_by_ref_value_warnings: bool,
    call_span: Option<php_ir::IrSpan>,
    by_ref_warning_callable_name: Option<&str>,
    elide_frame_args: bool,
) -> Result<PreparedArguments, VmError> {
    // Fast path: a plain positional call with exact arity to a function whose
    // parameters are all non-variadic and by-value (untyped, or typed without a
    // default — see the guard), with frame args elided. This is the
    // overwhelmingly common call shape; binding it directly skips the general
    // path's per-call `bound` vector, the required/variadic scans, and the
    // named/variadic/default machinery.
    //
    // It is behavior-identical to the general path for this shape. Both paths
    // hand the caller *raw* (uncoerced) by-value arguments; parameter type
    // coercion, strict-types enforcement, and
    // TypeError reporting are then applied uniformly by the shared post-binding
    // loop that calls `coerce_or_check_param_type` on every prepared argument
    // (see the two `prepare_arguments` call sites in `mod.rs`). The general
    // success path never coerces inside `bind_arguments` — it too produces raw
    // values and lets that same loop coerce them — so producing raw values here
    // for typed and untyped params alike feeds the identical values to the
    // identical type-check at the identical program point. Typed by-value params
    // therefore bind here exactly as they would through the general path.
    //
    // The remaining shape guarantees still hold: no by-ref references are
    // produced (all params are by-value), no defaults are consulted (exact
    // arity means every param is supplied), `frame_args` is empty (elided), and
    // `diagnostics` is empty (no by-ref value warnings). Values are moved rather
    // than re-cloned.
    if elide_frame_args && is_direct_bind_fast_shape(function, &args) {
        let mut prepared = Vec::with_capacity(function.params.len());
        for arg in args {
            let value = match arg.value {
                Value::Reference(cell) => cell.get(),
                other => other,
            };
            prepared.push(PreparedArg {
                value,
                reference: None,
                trace_holds_reference: false,
            });
        }
        return Ok(PreparedArguments {
            args: prepared,
            frame_args: Vec::new(),
            diagnostics: Vec::new(),
        });
    }

    let min = function
        .params
        .iter()
        .filter(|param| param.required)
        .count();
    let variadic_index = function.params.iter().position(|param| param.variadic);
    let max = variadic_index.unwrap_or(function.params.len());
    let mut bound: Vec<Option<CallArgument>> = (0..function.params.len()).map(|_| None).collect();
    let mut highest_frame_param_index: Option<usize> = None;
    let mut variadic_tail = Vec::new();
    let mut positional_index = 0usize;
    let mut saw_named = false;
    let mut supplied_count = 0usize;
    // Extra positional arguments to a non-variadic function are not an error in
    // PHP: they are ignored for parameter binding but remain visible to
    // func_get_args(), so keep them in the prepared list.
    let mut extra_positional: Vec<PreparedArg> = Vec::new();

    for arg in args {
        if let Some(name) = arg.name.clone() {
            saw_named = true;
            let Some(index) = function.params.iter().position(|param| param.name == name) else {
                if variadic_index.is_some() {
                    variadic_tail.push(VariadicTailArg {
                        key: Some(name),
                        value: arg.value,
                    });
                    supplied_count += 1;
                    continue;
                }
                return Err(VmError::fatal(
                    "E_PHP_VM_UNKNOWN_NAMED_ARG",
                    "arguments",
                    format!("Unknown named parameter ${name}"),
                )
                .with_context("function", &function.name)
                .with_context("parameter", name));
            };
            if function.params[index].variadic {
                variadic_tail.push(VariadicTailArg {
                    key: Some(name),
                    value: arg.value,
                });
                supplied_count += 1;
                continue;
            }
            if bound[index].is_some() {
                return Err(VmError::fatal(
                    "E_PHP_VM_DUPLICATE_NAMED_ARG",
                    "arguments",
                    format!("Named parameter ${name} overwrites previous argument"),
                )
                .with_context("function", &function.name)
                .with_context("parameter", name));
            }
            highest_frame_param_index =
                Some(highest_frame_param_index.map_or(index, |highest| highest.max(index)));
            bound[index] = Some(CallArgument {
                name: None,
                value: arg.value,
                value_kind: arg.value_kind,
                by_ref_local: arg.by_ref_local,
                by_ref_dim: arg.by_ref_dim,
                by_ref_property: arg.by_ref_property,
                by_ref_property_dim: arg.by_ref_property_dim,
            });
            supplied_count += 1;
            continue;
        }

        if saw_named {
            return Err(VmError::fatal(
                "E_PHP_VM_POSITIONAL_AFTER_NAMED_ARG",
                "arguments",
                format!(
                    "function {} cannot use positional argument after named argument",
                    function.name
                ),
            )
            .with_context("function", &function.name));
        }
        if variadic_index.is_some_and(|index| positional_index >= index) {
            variadic_tail.push(VariadicTailArg {
                key: None,
                value: arg.value,
            });
            positional_index += 1;
            supplied_count += 1;
            continue;
        }
        if positional_index >= max {
            extra_positional.push(PreparedArg {
                value: arg.value,
                reference: None,
                trace_holds_reference: false,
            });
            positional_index += 1;
            supplied_count += 1;
            continue;
        }
        if bound[positional_index].is_some() {
            let name = function.params[positional_index].name.clone();
            return Err(VmError::fatal(
                "E_PHP_VM_DUPLICATE_NAMED_ARG",
                "arguments",
                format!("Named parameter ${name} overwrites previous argument"),
            )
            .with_context("function", &function.name)
            .with_context("parameter", name));
        }
        highest_frame_param_index = Some(
            highest_frame_param_index
                .map_or(positional_index, |highest| highest.max(positional_index)),
        );
        bound[positional_index] = Some(CallArgument {
            name: None,
            value: arg.value,
            value_kind: arg.value_kind,
            by_ref_local: arg.by_ref_local,
            by_ref_dim: arg.by_ref_dim,
            by_ref_property: arg.by_ref_property,
            by_ref_property_dim: arg.by_ref_property_dim,
        });
        positional_index += 1;
        supplied_count += 1;
    }

    if supplied_count < min {
        precheck_bound_argument_types(
            compiled,
            function,
            &mut bound,
            state,
            typecheck,
            policy.call_site_strict_types,
            call_span,
        )
        .map_err(|message| argument_typecheck_error(&function.name, message))?;
        let requirement = if function.params.len() == min && variadic_index.is_none() {
            format!("exactly {min} expected")
        } else {
            format!("at least {min} expected")
        };
        let call_site = call_span
            .and_then(|span| source_span_file_line(compiled, span))
            .map(|(file, line)| format!(" in {file} on line {line}"))
            .unwrap_or_default();
        // The reference engine's message ends after the expectation; the
        // declaration site is the throwable's location and only the uncaught
        // fatal rendering appends it.
        return Err(VmError::fatal(
            "E_PHP_VM_TOO_FEW_ARGS",
            "arguments",
            format!(
                "Too few arguments to function {}(), {} passed{} and {}",
                function.name, supplied_count, call_site, requirement
            ),
        )
        .with_context("function", &function.name)
        .with_context("supplied_count", supplied_count)
        .with_context("minimum_count", min));
    }

    let mut prepared = Vec::with_capacity(function.params.len());
    let mut frame_args = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, param) in function.params.iter().enumerate() {
        if param.variadic {
            if !elide_frame_args {
                frame_args.extend(
                    variadic_tail
                        .iter()
                        .filter(|arg| arg.key.is_none())
                        .map(|arg| arg.value.clone()),
                );
            }
            prepared.push(PreparedArg {
                value: variadic_array(variadic_tail),
                reference: None,
                trace_holds_reference: false,
            });
            break;
        }
        if let Some(arg) = bound[index].take() {
            let value_reference = match &arg.value {
                Value::Reference(cell) => Some(cell.clone()),
                _ => None,
            };
            let mut value = value_reference
                .as_ref()
                .map(ReferenceCell::get)
                .unwrap_or_else(|| arg.value.clone());
            let reference = if param.by_ref {
                record_by_ref_arg_counters(typecheck, |counters| {
                    counters.by_ref_arg_location_binding_attempts += 1;
                });
                if let Some(reference) = value_reference {
                    record_by_ref_arg_counters(typecheck, |counters| {
                        counters.record_by_ref_arg_fallback("value_reference_argument");
                    });
                    Some(reference)
                } else if let Some(reference) = call_argument_reference_cell(
                    compiled, None, &arg, stack,
                )
                .map_err(|message| {
                    VmError::fatal("E_PHP_VM_BY_REF_BINDING", "arguments", message)
                        .with_context("function", &function.name)
                        .with_context("parameter", &param.name)
                })? {
                    record_by_ref_arg_location_binding(typecheck, &arg, &reference);
                    Some(reference)
                } else if allow_by_ref_value_warnings {
                    record_by_ref_arg_counters(typecheck, |counters| {
                        counters.record_by_ref_arg_fallback("value_given_warning");
                    });
                    diagnostics.push(by_ref_value_given_warning(
                        compiled,
                        function,
                        stack,
                        call_span,
                        index + 1,
                        &param.name,
                        by_ref_warning_callable_name,
                    ));
                    None
                } else {
                    record_by_ref_arg_counters(typecheck, |counters| {
                        counters.record_by_ref_arg_fallback("not_referenceable");
                    });
                    return Err(by_ref_not_referenceable_error(function, param, index));
                }
            } else {
                None
            };
            // A placeholder argument never materialized a caller value: its
            // observable value is whatever the bound location holds now.
            if matches!(arg.value_kind, IrCallArgValueKind::ByRefLocationPlaceholder)
                && let Some(cell) = &reference
            {
                value = cell.get();
            }
            // Traces observe later writes through by-ref parameters (matching
            // the reference engine): a supplied by-ref parameter's backtrace
            // entry holds the live cell rather than a value snapshot, which also
            // keeps the argument's copy-on-write handle unshared for the frame's
            // lifetime. `build_frame_trace_arguments` reconstructs that entry.
            let trace_holds_reference = reference.is_some();
            if !elide_frame_args
                && highest_frame_param_index.is_some_and(|highest| index <= highest)
            {
                frame_args.push(value.clone());
            }
            prepared.push(PreparedArg {
                value,
                reference,
                trace_holds_reference,
            });
        } else if let Some(default) = &param.default {
            let value = inline_constant_value(default);
            if param.by_ref {
                let reference = ReferenceCell::new(value.clone());
                if !elide_frame_args
                    && highest_frame_param_index.is_some_and(|highest| index <= highest)
                {
                    frame_args.push(value.clone());
                }
                // A by-ref parameter that falls back to its default keeps a
                // value snapshot in the trace (not the fresh cell), so
                // `trace_holds_reference` stays false.
                prepared.push(PreparedArg {
                    value,
                    reference: Some(reference),
                    trace_holds_reference: false,
                });
                continue;
            }
            if !elide_frame_args
                && highest_frame_param_index.is_some_and(|highest| index <= highest)
            {
                frame_args.push(value.clone());
            }
            prepared.push(PreparedArg {
                value,
                reference: None,
                trace_holds_reference: false,
            });
        } else if param.required {
            return Err(VmError::fatal(
                "E_PHP_VM_TOO_FEW_ARGS",
                "arguments",
                format!(
                    "function {} is missing argument ${}",
                    function.name, param.name
                ),
            )
            .with_context("function", &function.name)
            .with_context("parameter", &param.name));
        } else {
            return Err(VmError::fatal(
                "E_PHP_VM_UNSUPPORTED_DEFAULT_ARG",
                "arguments",
                format!(
                    "function {} parameter ${} has no folded default",
                    function.name, param.name
                ),
            )
            .with_context("function", &function.name)
            .with_context("parameter", &param.name));
        }
    }
    if !elide_frame_args {
        frame_args.extend(extra_positional.iter().map(|arg| arg.value.clone()));
    }
    prepared.extend(extra_positional);
    Ok(PreparedArguments {
        args: prepared,
        frame_args,
        diagnostics,
    })
}

struct VariadicTailArg {
    key: Option<String>,
    value: Value,
}

fn record_by_ref_arg_counters(
    typecheck: TypecheckFastPathContext<'_>,
    record: impl FnOnce(&mut VmCounters),
) {
    if let Some(counters) = typecheck.counters
        && let Some(counters) = counters.borrow_mut().as_mut()
    {
        record(counters);
    }
}

/// Attributes one by-ref binding that went through location metadata: whether
/// the caller still materialized the argument as a value register (pinning
/// array handles) and whether the bound cell's array is already shared, which
/// guarantees a copy-on-write separation on the callee's first write.
fn record_by_ref_arg_location_binding(
    typecheck: TypecheckFastPathContext<'_>,
    arg: &CallArgument,
    reference: &ReferenceCell,
) {
    record_by_ref_arg_counters(typecheck, |counters| {
        counters.by_ref_arg_location_bindings += 1;
        let materialized = !matches!(arg.value_kind, IrCallArgValueKind::ByRefLocationPlaceholder);
        if materialized {
            counters.by_ref_arg_value_materializations += 1;
            let kind = if arg.by_ref_local.is_some() {
                "local_value_materialized"
            } else if arg.by_ref_dim.is_some() {
                "dim_value_materialized"
            } else if arg.by_ref_property.is_some() {
                "property_value_materialized"
            } else {
                "property_dim_value_materialized"
            };
            counters.record_by_ref_arg_fallback(kind);
            if matches!(arg.value, Value::Array(_)) {
                counters.by_ref_arg_register_pins += 1;
            }
        }
        let shared_array = reference
            .try_with_value_mut(|value| matches!(value, Value::Array(array) if array.is_shared()))
            .unwrap_or(false);
        if shared_array {
            counters.by_ref_arg_cow_separations += 1;
        } else {
            counters.by_ref_arg_cow_separations_avoided += 1;
        }
    });
}

fn by_ref_not_referenceable_error(
    function: &IrFunction,
    param: &php_ir::function::IrParam,
    index: usize,
) -> VmError {
    VmError::fatal(
        "E_PHP_VM_BY_REF_ARG_NOT_REFERENCEABLE",
        "arguments",
        format!(
            "{}(): Argument #{} (${}) could not be passed by reference",
            function.name,
            index + 1,
            param.name
        ),
    )
    .with_context("function", &function.name)
    .with_context("parameter", &param.name)
    .with_context("position", index + 1)
}

fn argument_typecheck_error(function_name: &str, error: ParamTypecheckError) -> VmError {
    match error {
        ParamTypecheckError::Mismatch(message) => {
            VmError::fatal("E_PHP_VM_PARAM_TYPE_MISMATCH", "arguments", message)
                .with_context("function", function_name)
        }
        ParamTypecheckError::Runtime(message) => {
            VmError::fatal("E_PHP_VM_ARGUMENT_TYPECHECK", "arguments", message)
                .with_context("function", function_name)
        }
    }
}

pub(super) enum ParamTypecheckError {
    Mismatch(String),
    Runtime(String),
}

impl ParamTypecheckError {
    pub(super) fn render_message(self) -> String {
        match self {
            Self::Mismatch(message) => format!("E_PHP_VM_PARAM_TYPE_MISMATCH: {message}"),
            Self::Runtime(message) => message,
        }
    }
}

impl From<ParamTypecheckError> for String {
    fn from(error: ParamTypecheckError) -> Self {
        error.render_message()
    }
}

impl From<String> for ParamTypecheckError {
    fn from(message: String) -> Self {
        Self::Runtime(message)
    }
}

fn variadic_array(args: Vec<VariadicTailArg>) -> Value {
    let mut array = PhpArray::new();
    for arg in args {
        if let Some(key) = arg.key {
            array.insert(ArrayKey::String(PhpString::from(key.as_str())), arg.value);
        } else {
            array.append(arg.value);
        }
    }
    Value::Array(array)
}

fn precheck_bound_argument_types(
    compiled: &CompiledUnit,
    function: &IrFunction,
    bound: &mut [Option<CallArgument>],
    state: &ExecutionState,
    typecheck: TypecheckFastPathContext<'_>,
    call_site_strict_types: bool,
    call_span: Option<php_ir::IrSpan>,
) -> Result<(), ParamTypecheckError> {
    for (index, (param, arg)) in function.params.iter().zip(bound.iter_mut()).enumerate() {
        if param.by_ref || param.variadic {
            continue;
        }
        let Some(arg) = arg.as_mut() else {
            continue;
        };
        coerce_or_check_param_type(
            ParamTypecheckRequest {
                compiled,
                state,
                function,
                param,
                arg_index: index,
                fast_path: typecheck,
                strict_types: call_site_strict_types,
                call_span,
            },
            &mut arg.value,
        )?;
    }
    Ok(())
}

pub(super) fn call_argument_reference_cell(
    compiled: &CompiledUnit,
    state: Option<&ExecutionState>,
    arg: &CallArgument,
    stack: &mut CallStack,
) -> Result<Option<ReferenceCell>, String> {
    let _source = layout_source::enter(layout_source::BY_REF_ARGUMENT_BINDING);
    if let Some(local) = arg.by_ref_local {
        let frame = stack.current_mut().ok_or("no active frame")?;
        return Ok(frame.locals.ensure_reference_cell(local).map(Some)?);
    }
    if let Some(target) = &arg.by_ref_dim {
        return ensure_dim_reference_cell(stack, target.local, &target.dims).map(Some);
    }
    if let Some(target) = &arg.by_ref_property {
        return ensure_property_reference_cell(
            compiled,
            state,
            stack,
            &target.object,
            &target.property,
        )
        .map(Some);
    }
    if let Some(target) = &arg.by_ref_property_dim {
        return ensure_property_dim_reference_cell(
            compiled,
            state,
            stack,
            &target.object,
            &target.property,
            &target.dims,
        )
        .map(Some);
    }
    Ok(None)
}

fn by_ref_value_given_warning(
    compiled: &CompiledUnit,
    function: &IrFunction,
    stack: &CallStack,
    call_span: Option<php_ir::IrSpan>,
    position: usize,
    param_name: &str,
    callable_name: Option<&str>,
) -> RuntimeDiagnostic {
    let source_span = runtime_source_span(compiled, call_span.unwrap_or(function.span));
    let callable = if let Some(callable_name) = callable_name {
        callable_name.to_owned()
    } else if function.flags.is_closure {
        let file = source_span.file.as_deref().unwrap_or("<unknown>");
        format!("{{closure:{file}:{}}}", source_span.start)
    } else {
        function.name.clone()
    };
    RuntimeDiagnostic::new(
        "E_PHP_VM_BY_REF_ARG_VALUE_GIVEN_WARNING",
        RuntimeSeverity::Warning,
        format!(
            "{callable}(): Argument #{position} (${param_name}) must be passed by reference, value given"
        ),
        source_span,
        stack_trace(compiled, stack),
        Some(php_runtime::api::PhpReferenceClassification::Warning),
    )
}

#[derive(Clone, Copy)]
pub(super) struct TypecheckFastPathContext<'a> {
    enabled: bool,
    counters: Option<&'a RefCell<Option<VmCounters>>>,
}

impl<'a> TypecheckFastPathContext<'a> {
    pub(super) fn new(enabled: bool, counters: Option<&'a RefCell<Option<VmCounters>>>) -> Self {
        Self { enabled, counters }
    }

    fn disabled(self) -> Self {
        Self {
            enabled: false,
            counters: self.counters,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ParamTypecheckRequest<'a> {
    pub(super) compiled: &'a CompiledUnit,
    pub(super) state: &'a ExecutionState,
    pub(super) function: &'a IrFunction,
    pub(super) param: &'a IrParam,
    pub(super) arg_index: usize,
    pub(super) fast_path: TypecheckFastPathContext<'a>,
    pub(super) strict_types: bool,
    pub(super) call_span: Option<php_ir::IrSpan>,
}

pub(super) fn param_is_sensitive(param: &IrParam) -> bool {
    param.attributes.iter().any(|attribute| {
        attribute_name_matches_sensitive(attribute.resolved_name.as_deref())
            || attribute_name_matches_sensitive(attribute.fallback_name.as_deref())
            || attribute_name_matches_sensitive(Some(&attribute.name))
    })
}

fn attribute_name_matches_sensitive(name: Option<&str>) -> bool {
    name.and_then(|name| name.rsplit('\\').next())
        .is_some_and(|name| name.eq_ignore_ascii_case("SensitiveParameter"))
}

pub(super) fn trace_value_for_param(value: &Value, sensitive: bool) -> Value {
    if sensitive {
        sensitive_parameter_value()
    } else {
        value.clone()
    }
}

pub(super) fn sensitive_parameter_value() -> Value {
    Value::Object(ObjectRef::new_with_display_name(
        &RuntimeClassEntry {
            name: normalize_class_name("SensitiveParameterValue").into(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: RuntimeClassFlags::default(),
        },
        "SensitiveParameterValue",
    ))
}

/// PHP's `zend_zval_type_name`-style name used in `TypeError` messages: the
/// class name for objects, otherwise the scalar type name.
pub(super) fn type_error_value_name(value: &Value) -> String {
    match value {
        Value::Reference(cell) => type_error_value_name(&cell.get()),
        Value::Object(object) => object.display_name(),
        other => value_type_name(other).to_owned(),
    }
}

/// Builds PHP's argument `TypeError` message, e.g.
/// `Foo::bar(): Argument #1 ($baz) must be of type int, string given`.
fn param_type_mismatch_message(
    request: ParamTypecheckRequest<'_>,
    value: &Value,
    runtime_type: &RuntimeType,
    include_parameter_name: bool,
) -> String {
    let ParamTypecheckRequest {
        compiled,
        function,
        param,
        arg_index,
        call_span,
        ..
    } = request;
    let parameter_name = if include_parameter_name {
        format!(" (${})", param.name)
    } else {
        String::new()
    };
    let mut message = format!(
        "{}(): Argument #{}{} must be of type {}, {} given",
        function.name,
        arg_index + 1,
        parameter_name,
        runtime_type_error_name(compiled, runtime_type),
        type_error_value_name(value),
    );
    if let Some((file, line)) = call_span.and_then(|span| source_span_file_line(compiled, span)) {
        message.push_str(&format!(", called in {file} on line {line}"));
    }
    message
}

fn runtime_type_error_name(compiled: &CompiledUnit, runtime_type: &RuntimeType) -> String {
    match runtime_type {
        RuntimeType::Class { name, display_name } => {
            let normalized = normalize_class_name(name);
            class_display_name(compiled, &normalized)
                .or_else(|| display_name.clone())
                .unwrap_or_else(|| name.clone())
        }
        RuntimeType::Nullable { inner } => format!("?{}", runtime_type_error_name(compiled, inner)),
        RuntimeType::Union { members } => {
            let mut names = members
                .iter()
                .map(|member| runtime_type_error_name(compiled, member))
                .collect::<Vec<_>>();
            if names
                .iter()
                .all(|name| php_builtin_union_display_rank(name).is_some())
            {
                names.sort_by_key(|name| php_builtin_union_display_rank(name).unwrap_or(u8::MAX));
            }
            names.join("|")
        }
        RuntimeType::Intersection { members } => members
            .iter()
            .map(|member| runtime_type_error_name(compiled, member))
            .collect::<Vec<_>>()
            .join("&"),
        RuntimeType::Dnf { clauses } => clauses
            .iter()
            .map(|clause| match clause {
                RuntimeType::Intersection { .. } => {
                    format!("({})", runtime_type_error_name(compiled, clause))
                }
                _ => runtime_type_error_name(compiled, clause),
            })
            .collect::<Vec<_>>()
            .join("|"),
        other => runtime_type_name(other),
    }
}

fn php_builtin_union_display_rank(name: &str) -> Option<u8> {
    match name {
        "string" => Some(10),
        "int" => Some(20),
        "float" => Some(30),
        "bool" => Some(40),
        "false" => Some(50),
        "true" => Some(51),
        "array" => Some(60),
        "object" => Some(70),
        "callable" => Some(80),
        "iterable" => Some(90),
        "null" => Some(255),
        _ => None,
    }
}

pub(super) fn coerce_or_check_param_type(
    request: ParamTypecheckRequest<'_>,
    value: &mut Value,
) -> Result<(), ParamTypecheckError> {
    let ParamTypecheckRequest {
        compiled,
        state,
        param,
        fast_path,
        strict_types,
        ..
    } = request;
    let Some(runtime_type) = ir_runtime_type(param.type_.as_ref()) else {
        return Ok(());
    };
    if param.variadic {
        return coerce_or_check_variadic_param_type(request, value, &runtime_type);
    }
    if !strict_types && let Some(coerced) = coerce_value_to_runtime_type(value, &runtime_type) {
        *value = coerced;
        return Ok(());
    }
    materialize_int_to_float_runtime_type(value, &runtime_type);
    if vm_value_matches_runtime_type(compiled, Some(state), value, &runtime_type, fast_path)
        .map_err(ParamTypecheckError::Runtime)?
    {
        Ok(())
    } else {
        Err(ParamTypecheckError::Mismatch(param_type_mismatch_message(
            request,
            value,
            &runtime_type,
            true,
        )))
    }
}

fn coerce_or_check_variadic_param_type(
    request: ParamTypecheckRequest<'_>,
    value: &mut Value,
    runtime_type: &RuntimeType,
) -> Result<(), ParamTypecheckError> {
    let ParamTypecheckRequest {
        compiled,
        state,
        fast_path,
        strict_types,
        ..
    } = request;
    let Value::Array(array) = value else {
        return Ok(());
    };
    let entries = array
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    for (offset, (key, mut element)) in entries.into_iter().enumerate() {
        if !strict_types && let Some(coerced) = coerce_value_to_runtime_type(&element, runtime_type)
        {
            element = coerced;
        } else if !vm_value_matches_runtime_type(
            compiled,
            Some(state),
            &element,
            runtime_type,
            fast_path,
        )
        .map_err(ParamTypecheckError::Runtime)?
        {
            return Err(ParamTypecheckError::Mismatch(param_type_mismatch_message(
                ParamTypecheckRequest {
                    arg_index: request.arg_index + offset,
                    ..request
                },
                &element,
                runtime_type,
                false,
            )));
        }
        materialize_int_to_float_runtime_type(&mut element, runtime_type);
        if let Some(mut slot) = array.get_mut(&key) {
            *slot = element;
        }
    }
    Ok(())
}

/// Builds the closure value for `MakeClosure`; both the rich arm and the
/// dense opcode call this so binding context and debug info stay identical.
pub(super) fn coerce_return_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    function: &IrFunction,
    value: Option<Value>,
    typecheck: TypecheckFastPathContext<'_>,
) -> Result<Option<Value>, String> {
    // A generator's declared return type constrains the generator object the
    // call produces, never the body's `return` value: the reference engine
    // feeds that value to getReturn() without any type check.
    if function.flags.is_generator {
        return Ok(value);
    }
    let Some(return_type) = ir_runtime_type(function.return_type.as_ref()) else {
        return Ok(value);
    };
    if matches!(return_type, RuntimeType::Void) {
        return match value {
            None => Ok(None),
            Some(value) => Err(format!(
                "E_PHP_VM_RETURN_TYPE_MISMATCH: function {} returned {}, expected void",
                function.name,
                value_type_name(&value)
            )),
        };
    };

    let mut value = value.unwrap_or(Value::Null);
    if !compiled.unit().strict_types
        && !function.returns_by_ref
        && let Some(coerced) = coerce_value_to_runtime_type(&value, &return_type)
    {
        value = coerced;
    }
    materialize_int_to_float_runtime_type(&mut value, &return_type);
    if vm_value_matches_runtime_type(compiled, Some(state), &value, &return_type, typecheck)? {
        Ok(Some(value))
    } else {
        if function.name.ends_with("::__toString") && matches!(return_type, RuntimeType::String) {
            return Err(format!(
                "E_PHP_VM_TOSTRING_RETURN_TYPE: {}(): Return value must be of type string, {} returned",
                function.name,
                value_type_name(&value)
            ));
        }
        Err(format!(
            "E_PHP_VM_RETURN_TYPE_MISMATCH: function {} returned {}, expected {}",
            function.name,
            value_type_name(&value),
            runtime_type_name(&return_type)
        ))
    }
}

pub(super) fn check_property_type(
    compiled: &CompiledUnit,
    state: Option<&ExecutionState>,
    class_name: &str,
    property: &str,
    runtime_type: &Option<RuntimeType>,
    value: &Value,
    typecheck: TypecheckFastPathContext<'_>,
) -> Result<(), String> {
    let Some(runtime_type) = runtime_type else {
        return Ok(());
    };
    if vm_value_matches_runtime_type(compiled, state, value, runtime_type, typecheck)? {
        Ok(())
    } else {
        Err(format!(
            "E_PHP_VM_PROPERTY_TYPE_MISMATCH: Cannot assign {} to property {class_name}::${property} of type {}",
            type_error_value_name(value),
            runtime_type_name(runtime_type)
        ))
    }
}

fn vm_value_matches_runtime_type(
    compiled: &CompiledUnit,
    state: Option<&ExecutionState>,
    value: &Value,
    runtime_type: &RuntimeType,
    typecheck: TypecheckFastPathContext<'_>,
) -> Result<bool, String> {
    if let Value::Reference(cell) = value {
        return vm_value_matches_runtime_type(
            compiled,
            state,
            &cell.get(),
            runtime_type,
            typecheck,
        );
    }
    if typecheck.enabled {
        if typecheck_fast_path_match(value, runtime_type) {
            record_typecheck_fast_path(typecheck, true);
            return Ok(true);
        }
        record_typecheck_fast_path(typecheck, false);
    }
    let fallback_typecheck = typecheck.disabled();
    Ok(match runtime_type {
        RuntimeType::Callable => state.map_or_else(
            || value_matches_runtime_type(value, runtime_type),
            |state| value_is_callable(compiled, state, value, false),
        ),
        RuntimeType::Class { name, .. } => match state {
            Some(state) => object_instanceof_in_state(compiled, state, value, name)?,
            None => object_instanceof(compiled, value, name)?,
        },
        RuntimeType::Nullable { inner } => {
            matches!(value, Value::Null)
                || vm_value_matches_runtime_type(compiled, state, value, inner, fallback_typecheck)?
        }
        RuntimeType::Union { members } => {
            for member in members {
                if vm_value_matches_runtime_type(
                    compiled,
                    state,
                    value,
                    member,
                    fallback_typecheck,
                )? {
                    return Ok(true);
                }
            }
            false
        }
        RuntimeType::Intersection { members } => {
            for member in members {
                if !vm_value_matches_runtime_type(
                    compiled,
                    state,
                    value,
                    member,
                    fallback_typecheck,
                )? {
                    return Ok(false);
                }
            }
            true
        }
        RuntimeType::Dnf { clauses } => {
            for clause in clauses {
                if vm_value_matches_runtime_type(
                    compiled,
                    state,
                    value,
                    clause,
                    fallback_typecheck,
                )? {
                    return Ok(true);
                }
            }
            false
        }
        _ => value_matches_runtime_type(value, runtime_type),
    })
}

fn typecheck_fast_path_match(value: &Value, runtime_type: &RuntimeType) -> bool {
    match runtime_type {
        RuntimeType::Bool => matches!(value, Value::Bool(_)),
        RuntimeType::Int => matches!(value, Value::Int(_)),
        RuntimeType::Float => matches!(value, Value::Float(_) | Value::Int(_)),
        RuntimeType::String => matches!(value, Value::String(_)),
        RuntimeType::Array => matches!(value, Value::Array(_)),
        RuntimeType::Callable => matches!(value, Value::Callable(_)),
        RuntimeType::Object => {
            matches!(
                value,
                Value::Object(_) | Value::Callable(_) | Value::Fiber(_) | Value::Generator(_)
            )
        }
        RuntimeType::Class { name, .. } => {
            matches!(
                value,
                Value::Object(object) if object.class_name().eq_ignore_ascii_case(name)
            ) || matches!(
                value,
                Value::Fiber(_) if name.eq_ignore_ascii_case("Fiber")
            ) || matches!(
                value,
                Value::Generator(_) if name.eq_ignore_ascii_case("Generator")
            ) || (matches!(value, Value::Callable(_)) && name.eq_ignore_ascii_case("Closure"))
        }
        RuntimeType::Nullable { inner } => {
            matches!(value, Value::Null) || typecheck_fast_path_match(value, inner)
        }
        _ => false,
    }
}

fn record_typecheck_fast_path(typecheck: TypecheckFastPathContext<'_>, hit: bool) {
    if let Some(counters) = typecheck.counters
        && let Some(counters) = counters.borrow_mut().as_mut()
    {
        counters.record_typecheck_fast_path(hit);
    }
}

pub(super) fn ir_runtime_type(return_type: Option<&IrReturnType>) -> Option<RuntimeType> {
    Some(match return_type? {
        IrReturnType::Int => RuntimeType::Int,
        IrReturnType::Float => RuntimeType::Float,
        IrReturnType::String => RuntimeType::String,
        IrReturnType::Array => RuntimeType::Array,
        IrReturnType::Callable => RuntimeType::Callable,
        IrReturnType::Iterable => RuntimeType::Iterable,
        IrReturnType::Object => RuntimeType::Object,
        IrReturnType::Bool => RuntimeType::Bool,
        IrReturnType::Null => RuntimeType::Null,
        IrReturnType::Void => RuntimeType::Void,
        IrReturnType::Mixed => RuntimeType::Mixed,
        IrReturnType::Never => RuntimeType::Never,
        IrReturnType::False => RuntimeType::False,
        IrReturnType::True => RuntimeType::True,
        IrReturnType::Class { name, display_name } => RuntimeType::Class {
            name: name.clone(),
            display_name: display_name.clone(),
        },
        IrReturnType::Nullable { inner } => RuntimeType::Nullable {
            inner: Box::new(ir_runtime_type(Some(inner))?),
        },
        IrReturnType::Union { members } => RuntimeType::Union {
            members: members
                .iter()
                .map(|member| ir_runtime_type(Some(member)))
                .collect::<Option<Vec<_>>>()?,
        },
        IrReturnType::Intersection { members } => RuntimeType::Intersection {
            members: members
                .iter()
                .map(|member| ir_runtime_type(Some(member)))
                .collect::<Option<Vec<_>>>()?,
        },
        IrReturnType::Dnf { members } => RuntimeType::Dnf {
            clauses: members
                .iter()
                .map(|member| ir_runtime_type(Some(member)))
                .collect::<Option<Vec<_>>>()?,
        },
    })
}

fn coerce_value_to_runtime_type(value: &Value, runtime_type: &RuntimeType) -> Option<Value> {
    if matches!((runtime_type, value), (RuntimeType::Float, Value::Int(_))) {
        return to_float(value).ok().map(Value::float);
    }
    if value_matches_runtime_type(value, runtime_type) {
        return Some(value.clone());
    }
    if !matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_)
    ) {
        return None;
    }
    match runtime_type {
        RuntimeType::Nullable { inner } if matches!(value, Value::Null) => Some(Value::Null),
        RuntimeType::Nullable { inner } => coerce_value_to_runtime_type(value, inner),
        RuntimeType::Union { members } => members
            .iter()
            .find_map(|member| coerce_value_to_runtime_type(value, member)),
        RuntimeType::Int if matches!(value, Value::String(_)) => {
            to_number(value).ok().map(|number| match number {
                NumericValue::Int(value) => Value::Int(value),
                NumericValue::Float(value) => Value::Int(value as i64),
            })
        }
        RuntimeType::Int => to_int(value).ok().map(Value::Int),
        RuntimeType::Float if matches!(value, Value::String(_)) => to_number(value)
            .ok()
            .map(|number| Value::float(number.as_f64())),
        RuntimeType::Float => to_float(value).ok().map(Value::float),
        RuntimeType::String => to_string(value).ok().map(Value::String),
        RuntimeType::Bool => to_bool(value).ok().map(Value::Bool),
        _ => None,
    }
}

fn materialize_int_to_float_runtime_type(value: &mut Value, runtime_type: &RuntimeType) {
    if matches!(runtime_type, RuntimeType::Float)
        && matches!(value, Value::Int(_))
        && let Ok(float) = to_float(value)
    {
        *value = Value::float(float);
    }
}

pub(super) fn call_args_from_owned_php_array(array: PhpArray) -> Vec<CallArgument> {
    array
        .into_pairs()
        .into_iter()
        .map(|(key, value)| {
            let mut arg = CallArgument::positional(value);
            if let ArrayKey::String(name) = key {
                arg.name = Some(name.to_string_lossy());
            }
            arg
        })
        .collect()
}

pub(super) fn call_args_from_php_array(array: &PhpArray) -> Vec<CallArgument> {
    array
        .iter()
        .map(|(key, value)| {
            let mut arg = CallArgument::positional(value.clone());
            if let ArrayKey::String(name) = key {
                arg.name = Some(name.to_string_lossy());
            }
            arg
        })
        .collect()
}
