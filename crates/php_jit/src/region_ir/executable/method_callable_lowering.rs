fn lower_stable_method_callable_call(
    unit: &IrUnit,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    dst: RegId,
    callable: &KnownMethodCallableArray,
    args: &[IrCallArg],
) -> Option<(RegionInstructionKind, Option<RegionOperand>)> {
    if callable.length != 2 {
        return None;
    }
    let method = callable.method.as_deref()?;
    let trailing_unpack = matches!(
        args,
        [argument] if argument.unpack && argument.name.is_none()
    );
    match callable.target.as_ref()? {
        KnownMethodCallableTarget::Static { class_name } => {
            let name = format!("{class_name}::{method}");
            if trailing_unpack {
                let local = find_direct_static_method(unit, class_name, method)
                    .and_then(|function| unit.functions.get(function.index()));
                let external = local.is_none().then(|| {
                    published_external_named_callable_signature(external_function_signatures, &name)
                });
                let direct = local.is_some_and(|function| {
                    !function.flags.is_generator
                        && !function.returns_by_ref
                        && stable_unpack_callback_parameters_are_direct(&function.params)
                }) || external.flatten().is_some_and(|signature| {
                    !signature.requires_non_reference_trampoline
                        && !signature.returns_by_reference
                        && signature.native_arity as usize == signature.native_params.len()
                        && stable_unpack_callback_parameters_are_direct(&signature.native_params)
                });
                if !direct {
                    return None;
                }
            } else {
                stable_named_array_callback(
                    unit,
                    external_function_signatures,
                    &name,
                    args.len(),
                    true,
                )?;
            }
            Some((
                lower_stable_named_callable(unit, external_function_signatures, dst, name, args).0,
                None,
            ))
        }
        KnownMethodCallableTarget::Instance {
            receiver,
            class_name,
        } => {
            if !trailing_unpack {
                let callback = stable_method_array_callback(
                    unit,
                    external_function_signatures,
                    callable,
                    args.len(),
                    true,
                )?;
                let receiver = callback.receiver?;
                let receiver_operand = match receiver {
                    RegionOperand::Register(register) => Operand::Register(register),
                    RegionOperand::Local(local) => Operand::Local(local),
                    RegionOperand::Constant(_)
                    | RegionOperand::LinkedConstant { .. }
                    | RegionOperand::I64(_) => return None,
                };
                let call = if let Some(function) = callback.function {
                    lower_direct_method_call(unit, dst, function, receiver_operand, args)
                } else {
                    let signature = published_external_method_signature(
                        external_function_signatures,
                        class_name,
                        method,
                    )?;
                    lower_direct_external_method_call(
                        unit,
                        RegionCallResult::Register(dst),
                        signature,
                        Some(receiver_operand),
                        args,
                    )?
                };
                return Some((call, Some(receiver)));
            }

            let receiver_operand = match receiver {
                RegionOperand::Register(register) => Operand::Register(*register),
                RegionOperand::Local(local) => Operand::Local(*local),
                RegionOperand::Constant(_)
                | RegionOperand::LinkedConstant { .. }
                | RegionOperand::I64(_) => return None,
            };
            if let Some(function) = stable_instance_method_function(unit, class_name, method) {
                let target = unit.functions.get(function.index())?;
                if !stable_unpack_callback_parameters_are_direct(&target.params) {
                    return None;
                }
                return Some((
                    lower_direct_method_call(unit, dst, function, receiver_operand, args),
                    Some(*receiver),
                ));
            }

            let signature = published_external_method_signature(
                external_function_signatures,
                class_name,
                method,
            )?;
            if signature.requires_non_reference_trampoline
                || signature.returns_by_reference
                || signature.native_arity as usize
                    != signature.native_params.len().saturating_add(1)
                || !stable_unpack_callback_parameters_are_direct(&signature.native_params)
            {
                return None;
            }
            let mut operands = vec![Some(*receiver)];
            operands.extend(lower_call_operands(unit, args));
            Some((
                RegionInstructionKind::NativeCall(RegionNativeCall {
                    result: RegionCallResult::Register(dst),
                    target: RegionCallTarget::Function {
                        name: signature.name.clone(),
                        function: None,
                    },
                    args: args.to_vec(),
                    argument_operand_offset: 1,
                    operands,
                    direct_arity: None,
                    variadic: false,
                    returns_by_reference: false,
                    caller_strict_types: unit.strict_types,
                }),
                Some(*receiver),
            ))
        }
    }
}
