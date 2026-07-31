#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OptimizingCallbackBindingPlan {
    fixed_parameter_count: usize,
    provided_argument_count: usize,
    variadic: bool,
}

fn optimizing_callback_binding_plan(
    target: OptimizingCompiledCallTarget<'_>,
    hidden_argument_count: usize,
    provided_argument_count: usize,
    direct_reference_argument_count: usize,
) -> Option<OptimizingCallbackBindingPlan> {
    let fixed_parameter_count = target
        .params
        .iter()
        .position(|parameter| parameter.variadic)
        .unwrap_or(target.params.len());
    let variadic = fixed_parameter_count < target.params.len();
    let valid_variadic_shape = !variadic
        || fixed_parameter_count + 1 == target.params.len()
            && target.params[fixed_parameter_count].variadic;
    let reference_parameters_are_direct = target
        .params
        .iter()
        .enumerate()
        .all(|(index, parameter)| !parameter.by_ref || index < direct_reference_argument_count);
    let reference_trampoline_is_direct = target.reference_only_trampoline
        && reference_parameters_are_direct
        && (!target.returns_by_reference
            || target.params.iter().all(|parameter| !parameter.by_ref));
    if !valid_variadic_shape
        || target.arity != hidden_argument_count.saturating_add(target.params.len())
        || !reference_parameters_are_direct
        || target.requires_trampoline
            && !reference_trampoline_is_direct
        || provided_argument_count > fixed_parameter_count && !variadic
        || target
            .params
            .iter()
            .take(fixed_parameter_count)
            .skip(provided_argument_count)
            .any(|parameter| {
                parameter.required
                    || parameter.default.is_none()
                    || !parameter
                        .default
                        .as_ref()
                        .is_some_and(optimizing_callback_default_is_direct)
            })
        || target
            .params
            .iter()
            .take(provided_argument_count.min(fixed_parameter_count))
            .chain(
                target
                    .params
                    .get(fixed_parameter_count)
                    .filter(|parameter| {
                        parameter.variadic && provided_argument_count > fixed_parameter_count
                    }),
            )
            .filter_map(|parameter| parameter.type_.as_ref())
            .any(|type_| !optimizing_type_has_direct_guard(type_))
    {
        return None;
    }
    Some(OptimizingCallbackBindingPlan {
        fixed_parameter_count,
        provided_argument_count,
        variadic,
    })
}

fn optimizing_callback_default_is_direct(default: &IrConstant) -> bool {
    matches!(
        default,
        IrConstant::Null
            | IrConstant::Bool(_)
            | IrConstant::Int(_)
            | IrConstant::Float(_)
            | IrConstant::String(_)
            | IrConstant::StringBytes(_)
    )
}
