use super::*;

pub(super) fn prepare_arguments(
    compiled: &CompiledUnit,
    function: &IrFunction,
    args: Vec<CallArgument>,
    stack: &mut CallStack,
    state: &ExecutionState,
    typecheck: TypecheckFastPathContext<'_>,
    allow_by_ref_value_warnings: bool,
    call_span: Option<php_ir::IrSpan>,
    by_ref_warning_callable_name: Option<&str>,
) -> Result<PreparedArguments, String> {
    let min = function
        .params
        .iter()
        .filter(|param| param.required)
        .count();
    let variadic_index = function.params.iter().position(|param| param.variadic);
    let max = variadic_index.unwrap_or(function.params.len());
    let mut bound: Vec<Option<CallArgument>> = (0..function.params.len()).map(|_| None).collect();
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
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_NAMED_ARG: Unknown named parameter ${name}"
                ));
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
                return Err(format!(
                    "E_PHP_VM_DUPLICATE_NAMED_ARG: function {} argument ${name} was already provided",
                    function.name
                ));
            }
            bound[index] = Some(CallArgument {
                name: None,
                value: arg.value,
                value_kind: arg.value_kind,
                by_ref_local: arg.by_ref_local,
                by_ref_dim: arg.by_ref_dim,
                by_ref_property: arg.by_ref_property,
            });
            supplied_count += 1;
            continue;
        }

        if saw_named {
            return Err(format!(
                "E_PHP_VM_POSITIONAL_AFTER_NAMED_ARG: function {} cannot use positional argument after named argument",
                function.name
            ));
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
            return Err(format!(
                "E_PHP_VM_DUPLICATE_NAMED_ARG: function {} argument ${} was already provided",
                function.name, function.params[positional_index].name
            ));
        }
        bound[positional_index] = Some(CallArgument {
            name: None,
            value: arg.value,
            value_kind: arg.value_kind,
            by_ref_local: arg.by_ref_local,
            by_ref_dim: arg.by_ref_dim,
            by_ref_property: arg.by_ref_property,
        });
        positional_index += 1;
        supplied_count += 1;
    }

    if supplied_count < min {
        precheck_bound_argument_types(compiled, function, &mut bound, state, typecheck, call_span)?;
        let requirement = if function.params.len() == min && variadic_index.is_none() {
            format!("exactly {min} expected")
        } else {
            format!("at least {min} expected")
        };
        let call_site = call_span
            .and_then(|span| source_span_file_line(compiled, span))
            .map(|(file, line)| format!(" in {file} on line {line}"))
            .unwrap_or_default();
        let declaration_site = source_span_file_line(compiled, function.span)
            .map(|(file, line)| format!(" in {file}:{line}"))
            .unwrap_or_default();
        return Err(format!(
            "E_PHP_VM_TOO_FEW_ARGS: Too few arguments to function {}(), {} passed{} and {}{}",
            function.name, supplied_count, call_site, requirement, declaration_site
        ));
    }

    let mut prepared = Vec::with_capacity(function.params.len());
    let mut trace_args = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, param) in function.params.iter().enumerate() {
        if param.variadic {
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
            let reference = if param.by_ref {
                if let Some(reference) = call_argument_reference_cell(compiled, &arg, stack)? {
                    Some(reference)
                } else if allow_by_ref_value_warnings {
                    diagnostics.push(by_ref_value_given_warning(
                        compiled,
                        function,
                        stack,
                        index + 1,
                        &param.name,
                        by_ref_warning_callable_name,
                    ));
                    None
                } else {
                    return Err(by_ref_not_referenceable_error(function, param, index));
                }
            } else {
                None
            };
            trace_args.push(FrameTraceArgument {
                name: None,
                value: trace_value_for_param(&arg.value, param_is_sensitive(param)),
            });
            prepared.push(PreparedArg {
                value: arg.value,
                reference,
            });
        } else if let Some(default) = &param.default {
            if param.by_ref {
                return Err(by_ref_not_referenceable_error(function, param, index));
            }
            let value = inline_constant_value(default);
            trace_args.push(FrameTraceArgument {
                name: None,
                value: trace_value_for_param(&value, param_is_sensitive(param)),
            });
            prepared.push(PreparedArg {
                value,
                reference: None,
            });
        } else if param.required {
            return Err(format!(
                "E_PHP_VM_TOO_FEW_ARGS: function {} is missing argument ${}",
                function.name, param.name
            ));
        } else {
            return Err(format!(
                "E_PHP_VM_UNSUPPORTED_DEFAULT_ARG: function {} parameter ${} has no folded default",
                function.name, param.name
            ));
        }
    }
    trace_args.extend(extra_positional.iter().map(|arg| FrameTraceArgument {
        name: None,
        value: arg.value.clone(),
    }));
    prepared.extend(extra_positional);
    Ok(PreparedArguments {
        args: prepared,
        trace_args,
        diagnostics,
    })
}

struct VariadicTailArg {
    key: Option<String>,
    value: Value,
}

fn by_ref_not_referenceable_error(
    function: &IrFunction,
    param: &php_ir::function::IrParam,
    index: usize,
) -> String {
    format!(
        "E_PHP_VM_BY_REF_ARG_NOT_REFERENCEABLE: {}(): Argument #{} (${}) could not be passed by reference",
        function.name,
        index + 1,
        param.name
    )
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
