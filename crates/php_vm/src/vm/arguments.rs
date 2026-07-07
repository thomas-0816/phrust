use super::prelude::*;
use crate::error::VmError;

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
    // hand the caller *raw* (uncoerced) by-value arguments plus a raw trace
    // snapshot; parameter type coercion, strict-types enforcement, and
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
    if elide_frame_args
        && args.len() == function.params.len()
        && args.iter().all(|arg| {
            arg.name.is_none()
                && !matches!(arg.value_kind, IrCallArgValueKind::ByRefLocationPlaceholder)
        })
        && function.params.iter().all(|param| {
            !param.variadic
                    && !param.by_ref
                    // Untyped params (with or without a default) kept the fast
                    // path before this change; typed params without a default
                    // are the R1 win added here. Both bind by-position and are
                    // coerced by the shared post-binding loop, so both are
                    // behavior-identical to the general path at exact arity
                    // (a supplied param never consults its default). Only
                    // typed-with-default stays on the general path, conservatively.
                    && (param.type_.is_none() || param.default.is_none())
        })
    {
        let mut prepared = Vec::with_capacity(function.params.len());
        let mut trace_args = Vec::with_capacity(function.params.len());
        for (arg, param) in args.into_iter().zip(function.params.iter()) {
            let value = match arg.value {
                Value::Reference(cell) => cell.get(),
                other => other,
            };
            trace_args.push(FrameTraceArgument {
                name: None,
                value: if param_is_sensitive(param) {
                    trace_value_for_param(&value, true)
                } else {
                    value.clone()
                },
            });
            prepared.push(PreparedArg {
                value,
                reference: None,
            });
        }
        return Ok(PreparedArguments {
            args: prepared,
            frame_args: Vec::new(),
            trace_args,
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
    let mut trace_args = Vec::new();
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
            let sensitive = param_is_sensitive(param);
            trace_args.extend(variadic_tail.iter().map(|arg| FrameTraceArgument {
                name: arg.key.clone(),
                value: trace_value_for_param(&arg.value, sensitive),
            }));
            prepared.push(PreparedArg {
                value: variadic_array(variadic_tail),
                reference: None,
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
            trace_args.push(FrameTraceArgument {
                name: None,
                value: if param_is_sensitive(param) {
                    trace_value_for_param(&value, true)
                } else if let Some(cell) = &reference {
                    // Traces observe later writes through by-ref parameters
                    // (matching the reference engine), and holding the cell
                    // instead of a value snapshot keeps the argument's
                    // copy-on-write handle unshared for the frame's lifetime.
                    Value::Reference(cell.clone())
                } else {
                    value.clone()
                },
            });
            if !elide_frame_args
                && highest_frame_param_index.is_some_and(|highest| index <= highest)
            {
                frame_args.push(value.clone());
            }
            prepared.push(PreparedArg { value, reference });
        } else if let Some(default) = &param.default {
            let value = inline_constant_value(default);
            trace_args.push(FrameTraceArgument {
                name: None,
                value: trace_value_for_param(&value, param_is_sensitive(param)),
            });
            if param.by_ref {
                let reference = ReferenceCell::new(value.clone());
                if !elide_frame_args
                    && highest_frame_param_index.is_some_and(|highest| index <= highest)
                {
                    frame_args.push(value.clone());
                }
                prepared.push(PreparedArg {
                    value,
                    reference: Some(reference),
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
    trace_args.extend(extra_positional.iter().map(|arg| FrameTraceArgument {
        name: None,
        value: arg.value.clone(),
    }));
    if !elide_frame_args {
        frame_args.extend(extra_positional.iter().map(|arg| arg.value.clone()));
    }
    prepared.extend(extra_positional);
    Ok(PreparedArguments {
        args: prepared,
        frame_args,
        trace_args,
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

fn argument_typecheck_error(function_name: &str, message: String) -> VmError {
    if let Some(message) = message.strip_prefix("E_PHP_VM_PARAM_TYPE_MISMATCH: ") {
        return VmError::fatal("E_PHP_VM_PARAM_TYPE_MISMATCH", "arguments", message)
            .with_context("function", function_name);
    }
    VmError::fatal("E_PHP_VM_ARGUMENT_TYPECHECK", "arguments", message)
        .with_context("function", function_name)
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
