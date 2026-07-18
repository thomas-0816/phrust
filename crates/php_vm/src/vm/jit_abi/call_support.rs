use super::*;

pub(super) fn dereference_native_callable_value(mut value: Value) -> Value {
    // References are transparent when PHP resolves a callable, including the
    // target and method slots of a two-element callable array. Peel a bounded
    // chain because foreach and by-reference argument binding can wrap the
    // same callable more than once.
    for _ in 0..64 {
        match value {
            Value::Reference(reference) => value = reference.get(),
            value => return value,
        }
    }
    value
}

pub(super) fn stable_native_symbol_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub(super) fn native_catch_matches(
    context: &NativeExecutionContext<'_>,
    types: &[String],
    value: i64,
) -> bool {
    let class = context
        .decode(value)
        .ok()
        .and_then(super::super::native_exception_fields)
        .map(|(class, _, _)| class);
    class.is_some_and(|class| {
        let normalized = class.to_ascii_lowercase();
        types.iter().any(|type_| {
            type_.eq_ignore_ascii_case(&class)
                || type_.eq_ignore_ascii_case("Throwable")
                || (type_.eq_ignore_ascii_case("Exception") && normalized.ends_with("exception"))
                || (type_.eq_ignore_ascii_case("Error") && normalized.ends_with("error"))
        })
    })
}

pub(super) fn invoke_native_function(
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
) -> Result<i64, String> {
    invoke_native_function_with_metadata(context, function, arguments, None)
}

pub(super) fn invoke_native_function_with_metadata(
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> Result<i64, String> {
    invoke_native_function_with_metadata_strict(
        context,
        function,
        arguments,
        metadata,
        context.unit.strict_types,
    )
}

pub(super) fn invoke_native_function_with_metadata_strict(
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    strict: bool,
) -> Result<i64, String> {
    let target = context
        .unit
        .functions
        .get(function.index())
        .cloned()
        .ok_or_else(|| format!("native function {} is missing", function.raw()))?;
    let _depth_guard = enter_native_call(&target.name)?;
    let instance_method = context.unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == function)
            .map(|method| !method.flags.is_static)
    });
    let leading = target.captures.len()
        + usize::from(instance_method.unwrap_or(false))
        + usize::from(native_function_has_implicit_closure_this(&target));
    if arguments.len() < leading {
        return Err(format!(
            "{}() is missing its native receiver/capture arguments",
            target.name
        ));
    }
    // The callee frame owns its implicit receiver and captured locals just as
    // it owns ordinary bound arguments. The trampoline's input operands are
    // only borrowed, so materialize independent handles before passing the
    // leading frame slots to native code. Reusing the caller's handle lets
    // callee frame cleanup release a still-live caller local and the recycled
    // value-table slot can then decode as an unrelated value.
    let mut bound = arguments[..leading]
        .iter()
        .map(|argument| {
            context
                .decode(*argument)
                .and_then(|value| context.encode(value))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let raw_supplied = &arguments[leading..];
    let mut supplied = Vec::<(Option<String>, i64)>::new();
    if let Some(metadata) = metadata {
        for (argument, value) in metadata.iter().zip(raw_supplied) {
            if argument.unpack {
                let Value::Array(array) = context.decode(*value)? else {
                    return Err("Only arrays and Traversables can be unpacked".to_owned());
                };
                supplied.extend(
                    array
                        .iter()
                        .map(|(key, value)| {
                            let name = match key {
                                php_runtime::api::ArrayKey::Int(_) => None,
                                php_runtime::api::ArrayKey::String(name) => {
                                    Some(name.to_string_lossy())
                                }
                            };
                            context.encode(value.clone()).map(|value| (name, value))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
            } else {
                supplied.push((argument.name.clone(), *value));
            }
        }
    } else {
        supplied.extend(raw_supplied.iter().copied().map(|value| (None, value)));
    }
    let variadic_index = target
        .params
        .iter()
        .position(|parameter| parameter.variadic);
    let fixed_count = variadic_index.unwrap_or(target.params.len());
    let mut assigned = vec![None; fixed_count];
    let mut variadic = php_runtime::api::PhpArray::new();
    let mut visible_extra = Vec::new();
    let mut positional = 0usize;
    let mut saw_named = false;
    for (name, value) in &supplied {
        if let Some(name) = name {
            saw_named = true;
            if let Some(index) = target.params[..fixed_count]
                .iter()
                .position(|parameter| parameter.name.eq_ignore_ascii_case(name))
            {
                if assigned[index].replace(*value).is_some() {
                    return Err(format!(
                        "Named parameter ${name} overwrites previous argument"
                    ));
                }
            } else if variadic_index.is_some() {
                variadic.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        name.as_bytes().to_vec(),
                    )),
                    context.decode(*value)?,
                );
            } else {
                return Err(format!("E_PHP_THROW:Error:Unknown named parameter ${name}"));
            }
        } else {
            if saw_named {
                return Err("Cannot use positional argument after named argument".to_owned());
            }
            while positional < fixed_count && assigned[positional].is_some() {
                positional += 1;
            }
            if positional < fixed_count {
                assigned[positional] = Some(*value);
                positional += 1;
            } else if variadic_index.is_some() {
                variadic.append(context.decode(*value)?);
                visible_extra.push(*value);
            } else {
                visible_extra.push(*value);
            }
        }
    }
    for (index, parameter) in target.params.iter().enumerate() {
        if parameter.variadic {
            let mut values = php_runtime::api::PhpArray::new();
            for (key, value) in variadic.iter() {
                let value = parameter.type_.as_ref().map_or_else(
                    || value.clone(),
                    |type_| native_coerce_call_argument(value.clone(), type_, strict),
                );
                values.insert(key.clone(), value);
            }
            bound.push(context.encode(Value::Array(values))?);
        } else if let Some(argument) = assigned[index] {
            if parameter.by_ref {
                match context.decode(argument)? {
                    Value::Reference(reference) => {
                        if matches!(reference.get(), Value::Uninitialized) {
                            reference.set(Value::Null);
                        }
                    }
                    _ => {
                        return Err(format!(
                            "E_PHP_THROW:Error:{}(): Argument #{} (${}) could not be passed by reference",
                            target.name,
                            index + 1,
                            parameter.name
                        ));
                    }
                }
                if let Some(type_) = &parameter.type_
                    && let Value::Reference(reference) = context.decode(argument)?
                {
                    let value = native_coerce_call_argument(reference.get(), type_, strict);
                    if !(native_value_matches_ir_type_in_context(context, &value, type_)
                        || matches!(type_, php_ir::IrReturnType::Callable)
                            && native_value_is_callable(context, &value))
                    {
                        return Err(format!(
                            "E_PHP_THROW:TypeError:{}(): Argument #{} (${}) must be of type {}, {} given",
                            target.name,
                            index + 1,
                            parameter.name,
                            native_ir_type_name(type_),
                            native_value_type_name(&value)
                        ));
                    }
                    reference.set(value);
                }
                bound.push(argument);
            } else {
                let mut value = match context.decode(argument)? {
                    Value::Reference(reference) => reference.get(),
                    value => value,
                };
                if let Some(type_) = &parameter.type_ {
                    value = native_coerce_call_argument(value, type_, strict);
                    if !(native_value_matches_ir_type_in_context(context, &value, type_)
                        || matches!(type_, php_ir::IrReturnType::Callable)
                            && native_value_is_callable(context, &value))
                    {
                        return Err(format!(
                            "E_PHP_THROW:TypeError:{}(): Argument #{} (${}) must be of type {}, {} given",
                            target.name,
                            index + 1,
                            parameter.name,
                            native_ir_type_name(type_),
                            native_value_type_name(&value)
                        ));
                    }
                }
                bound.push(context.encode(value)?);
            }
        } else if let Some(default) = &parameter.default {
            let value = native_runtime_constant_value(context, default)?;
            let value = if parameter.by_ref {
                Value::Reference(php_runtime::api::ReferenceCell::new(value))
            } else {
                value
            };
            bound.push(context.encode(value)?);
        } else {
            return Err(format!("Too few arguments to function {}()", target.name));
        }
    }
    let visible_arguments = assigned
        .iter()
        .flatten()
        .chain(&visible_extra)
        .map(|value| context.decode(*value))
        .collect::<Result<Vec<_>, _>>()?;
    context.push_call_arguments(visible_arguments.clone());
    let result = invoke_native_method_with_trace_arguments(
        context,
        function,
        &bound,
        Some(&visible_arguments),
    );
    context.pop_call_arguments();
    result
}

pub(super) fn materialize_native_property_reference_arguments(
    context: &mut NativeExecutionContext<'_>,
    arguments: &mut [php_jit::JitNativeCallArgument],
    encoded: &mut [i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> Result<(), String> {
    let Some(metadata) = metadata else {
        return Ok(());
    };
    for (index, ((argument, encoded), call_argument)) in arguments
        .iter_mut()
        .zip(encoded.iter_mut())
        .zip(metadata)
        .enumerate()
    {
        if argument.flags.0 & php_jit::JitNativeArgFlags::BY_REFERENCE.0 == 0
            || matches!(context.decode(*encoded)?, Value::Reference(_))
        {
            continue;
        }
        let Some(target) = &call_argument.by_ref_property else {
            continue;
        };
        if argument.property_receiver == 0 {
            return Err(format!(
                "native call argument {} is missing its property receiver",
                index + 1
            ));
        }
        let mut receiver = context.decode(argument.property_receiver)?;
        for _ in 0..16 {
            let Value::Reference(reference) = receiver else {
                break;
            };
            receiver = reference.get();
        }
        let Value::Object(object) = receiver else {
            return Err(format!(
                "native call argument {} property receiver is not an object",
                index + 1
            ));
        };
        let reference = match object.get_property(&target.property) {
            Some(Value::Reference(reference)) => reference,
            Some(value) => {
                let reference = php_runtime::api::ReferenceCell::new(value);
                object.set_property(target.property.clone(), Value::Reference(reference.clone()));
                reference
            }
            None => {
                let reference = php_runtime::api::ReferenceCell::new(Value::Null);
                object.set_property(target.property.clone(), Value::Reference(reference.clone()));
                reference
            }
        };
        let reference = context.encode(Value::Reference(reference))?;
        argument.value.payload = reference as u64;
        *encoded = reference;
    }
    Ok(())
}

pub(super) fn invoke_native_named_callable(
    context: &mut NativeExecutionContext<'_>,
    name: &str,
    arguments: &[i64],
    instruction: &php_ir::Instruction,
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> Result<i64, String> {
    if let Some(function) = context.function_id(name) {
        if native_function_is_generator(context, function) {
            let arguments = arguments
                .iter()
                .map(|value| context.decode(*value))
                .collect::<Result<Vec<_>, _>>()?;
            return context.encode(Value::Generator(php_runtime::api::GeneratorRef::new(
                function.raw(),
                arguments,
            )));
        }
        return invoke_native_function_with_metadata_strict(
            context,
            function,
            arguments,
            metadata,
            context.unit.strict_types_for_span(instruction.span),
        );
    }
    if let Some(function) = context.external_function(name) {
        return invoke_native_external_function_with_metadata(
            context,
            function,
            arguments,
            metadata,
            None,
            context.unit.strict_types_for_span(instruction.span),
        );
    }
    let builtin_name = if php_std::arginfo::function_metadata_indexed(name).is_some() {
        name
    } else {
        name.rsplit('\\').next().unwrap_or(name)
    };
    let expanded = bind_native_builtin_arguments(context, builtin_name, arguments, metadata)?;
    execute_native_builtin(context, builtin_name, &expanded, instruction, None)
}

pub(super) fn expand_native_unpack_arguments(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> Result<Vec<i64>, String> {
    let Some(metadata) = metadata else {
        return Ok(arguments.to_vec());
    };
    let mut expanded = Vec::new();
    for (argument, value) in metadata.iter().zip(arguments) {
        if argument.unpack {
            let Value::Array(array) = context.decode(*value)? else {
                return Err("Only arrays and Traversables can be unpacked".to_owned());
            };
            for (_, value) in array.iter() {
                expanded.push(context.encode(value.clone())?);
            }
        } else {
            expanded.push(*value);
        }
    }
    Ok(expanded)
}

pub(super) fn native_builtin_default_value(
    context: &NativeExecutionContext<'_>,
    expression: &str,
) -> Result<Value, String> {
    let expression = expression.trim();
    match expression {
        "null" => Ok(Value::Null),
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        "PHP_INT_MAX" => Ok(Value::Int(i64::MAX)),
        _ if expression.starts_with('"') && expression.ends_with('"') => {
            let inner = &expression[1..expression.len().saturating_sub(1)];
            let mut bytes = Vec::new();
            let mut escaped = false;
            for byte in inner.bytes() {
                if escaped {
                    bytes.push(match byte {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'v' => 0x0b,
                        b'0' => 0,
                        byte => byte,
                    });
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else {
                    bytes.push(byte);
                }
            }
            Ok(Value::String(PhpString::from_bytes(bytes)))
        }
        _ if expression.contains('|') => {
            let mut value = 0i64;
            for name in expression.split('|').map(str::trim) {
                let constant = context.lookup_constant(name)?;
                let Value::Int(constant) = constant else {
                    return Err(format!("builtin default constant {name} is not an int"));
                };
                value |= constant;
            }
            Ok(Value::Int(value))
        }
        _ => context.lookup_constant(expression).or_else(|_| {
            expression
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| format!("unsupported builtin default expression {expression}"))
        }),
    }
}

pub(super) fn bind_native_builtin_arguments(
    context: &mut NativeExecutionContext<'_>,
    name: &str,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> Result<Vec<i64>, String> {
    if native_builtins::native_builtin_is_unavailable_target_function(name)
        || (name.eq_ignore_ascii_case("print")
            && metadata
                .is_some_and(|arguments| arguments.iter().any(|argument| argument.name.is_some())))
    {
        return Err(format!(
            "E_PHP_THROW:Error:Call to undefined function {name}()"
        ));
    }
    let Some(call_metadata) = metadata else {
        return Ok(arguments.to_vec());
    };
    if !call_metadata.iter().any(|argument| argument.name.is_some()) {
        return expand_native_unpack_arguments(context, arguments, metadata);
    }
    let function = php_std::arginfo::function_metadata_indexed(name)
        .ok_or_else(|| format!("builtin {name} has no argument metadata"))?;
    let mut assigned = vec![None; function.params.len()];
    let mut positional = 0usize;
    for (argument, value) in call_metadata.iter().zip(arguments) {
        if argument.unpack {
            return Err("named builtin argument unpacking is not supported".to_owned());
        }
        if let Some(name) = &argument.name {
            let index = function
                .params
                .iter()
                .position(|parameter| parameter.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("E_PHP_THROW:Error:Unknown named parameter ${name}"))?;
            assigned[index] = Some(*value);
        } else {
            while positional < assigned.len() && assigned[positional].is_some() {
                positional += 1;
            }
            if positional < assigned.len() {
                assigned[positional] = Some(*value);
                positional += 1;
            }
        }
    }
    let last = assigned.iter().rposition(Option::is_some).unwrap_or(0);
    let mut bound = Vec::with_capacity(last.saturating_add(1));
    for (index, parameter) in function.params.iter().enumerate().take(last + 1) {
        if let Some(value) = assigned[index] {
            bound.push(value);
        } else if let Some(default) = parameter.default_value {
            let value = native_builtin_default_value(context, default)?;
            bound.push(context.encode(value)?);
        } else {
            return Err(format!("Missing required argument ${}", parameter.name));
        }
    }
    Ok(bound)
}

pub(super) fn invoke_native_bound_method(
    context: &mut NativeExecutionContext<'_>,
    target: &php_runtime::api::CallableMethodTarget,
    method: &str,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    strict: bool,
    caller_function: Option<u32>,
) -> Result<i64, String> {
    let (class_name, receiver) = match target {
        php_runtime::api::CallableMethodTarget::Object(object) => (
            object.class_name(),
            Some(context.encode(Value::Object(object.clone()))?),
        ),
        php_runtime::api::CallableMethodTarget::Class(class) => (class.clone(), None),
    };
    let entry = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalize_class_name(&class_name))
        .and_then(|class| {
            class
                .methods
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(method))
        })
        .cloned();
    let mut call_arguments = Vec::with_capacity(arguments.len() + usize::from(receiver.is_some()));
    call_arguments.extend(receiver);
    call_arguments.extend_from_slice(arguments);
    if let Some(entry) = entry {
        let access_error = caller_function.and_then(|caller_function| {
            native_method_access_error(context, entry.function, caller_function, false)
        });
        if let Some(error) = access_error {
            return Err(format!("E_PHP_THROW:Error:{error}"));
        }
        if caller_function.is_none() && (entry.flags.is_private || entry.flags.is_protected) {
            return Err(format!(
                "E_PHP_THROW:Error:Call to {} method {class_name}::{method}() from global scope",
                if entry.flags.is_private {
                    "private"
                } else {
                    "protected"
                }
            ));
        }
        let function = entry.function;
        if native_function_is_generator(context, function) {
            let arguments = call_arguments
                .iter()
                .map(|value| context.decode(*value))
                .collect::<Result<Vec<_>, _>>()?;
            return context.encode(Value::Generator(php_runtime::api::GeneratorRef::new(
                function.raw(),
                arguments,
            )));
        }
        let pushed_called_class = entry.flags.is_static;
        if pushed_called_class {
            context.called_classes.push(class_name.clone());
        }
        let result = invoke_native_function_with_metadata_strict(
            context,
            function,
            &call_arguments,
            metadata,
            strict,
        );
        if pushed_called_class {
            context.called_classes.pop();
        }
        return result;
    }
    if let Some((function, _)) = native_external_method(context, &class_name, method) {
        return invoke_native_external_function_with_metadata(
            context,
            function,
            &call_arguments,
            metadata,
            Some(class_name),
            strict,
        );
    }
    Err(format!("Call to undefined method {class_name}::{method}()"))
}

pub(super) fn execute_native_dynamic_callable(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    encoded: &[i64],
    caller_function: Option<u32>,
) -> Option<Result<i64, String>> {
    if !matches!(
        instruction.kind,
        php_ir::InstructionKind::CallCallable { .. }
            | php_ir::InstructionKind::CallClosure { .. }
            | php_ir::InstructionKind::Pipe { .. }
    ) {
        return None;
    }
    let Some((callee, arguments)) = encoded.split_first() else {
        return Some(Err("callable operand is missing".to_owned()));
    };
    let callee = match context.decode(*callee) {
        Ok(value) => dereference_native_callable_value(value),
        Err(error) => return Some(Err(error)),
    };
    let metadata = match &instruction.kind {
        php_ir::InstructionKind::CallCallable { args, .. }
        | php_ir::InstructionKind::CallClosure { args, .. } => Some(args.as_slice()),
        php_ir::InstructionKind::Pipe { .. } => None,
        _ => None,
    };
    let result = (|| -> Result<i64, String> {
        match callee {
            Value::Callable(callable) => match callable.as_ref() {
                php_runtime::api::CallableValue::UserFunction { name }
                | php_runtime::api::CallableValue::InternalBuiltin { name } => {
                    invoke_native_named_callable(context, name, arguments, instruction, metadata)
                }
                php_runtime::api::CallableValue::Closure(closure) => {
                    let function = php_ir::FunctionId::new(closure.function);
                    let has_implicit_this = closure
                        .context
                        .owner_unit
                        .and_then(|unit| context.dynamic_units.get(unit))
                        .map(|package| package.compiled.unit())
                        .unwrap_or(&*context.unit)
                        .functions
                        .get(function.index())
                        .is_some_and(native_function_has_implicit_closure_this);
                    let mut closure_arguments = Vec::with_capacity(
                        usize::from(has_implicit_this) + closure.captures.len() + arguments.len(),
                    );
                    if has_implicit_this {
                        closure_arguments.push(
                            context.encode(
                                closure
                                    .bound_this
                                    .as_ref()
                                    .map_or(Value::Null, |object| Value::Object(object.clone())),
                            )?,
                        );
                    }
                    closure_arguments.extend(
                        closure
                            .captures
                            .iter()
                            .map(|capture| {
                                if capture.name.eq_ignore_ascii_case("this")
                                    && let Some(object) = &closure.bound_this
                                {
                                    context.encode(Value::Object(object.clone()))
                                } else if let Some(reference) = capture.reference() {
                                    context.encode(Value::Reference(reference))
                                } else {
                                    context.encode(capture.value().cloned().unwrap_or(Value::Null))
                                }
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    );
                    closure_arguments.extend_from_slice(arguments);
                    let pushed_scope = closure.context.scope_class.is_some();
                    if let Some(scope_class) = &closure.context.scope_class {
                        context.lexical_scope_classes.push(scope_class.to_string());
                    }
                    let generator_owner_is_current =
                        closure.context.owner_unit.is_none_or(|unit| {
                            context
                                .dynamic_units
                                .get(unit)
                                .is_some_and(|package| package.compiled.ptr_eq(&context.compiled))
                        });
                    if generator_owner_is_current && native_function_is_generator(context, function)
                    {
                        let arguments = closure_arguments
                            .iter()
                            .map(|value| context.decode(*value))
                            .collect::<Result<Vec<_>, _>>()?;
                        let result = context.encode(Value::Generator(
                            php_runtime::api::GeneratorRef::new(function.raw(), arguments),
                        ));
                        if pushed_scope {
                            context.lexical_scope_classes.pop();
                        }
                        return result;
                    }
                    if let Some(unit) = closure.context.owner_unit {
                        let result = invoke_native_external_function_with_metadata(
                            context,
                            NativeDynamicFunction { unit, function },
                            &closure_arguments,
                            metadata,
                            closure
                                .context
                                .called_class
                                .as_ref()
                                .map(|class| class.to_string()),
                            context.unit.strict_types_for_span(instruction.span),
                        );
                        if pushed_scope {
                            context.lexical_scope_classes.pop();
                        }
                        return result;
                    }
                    let pushed_called_class = closure.context.called_class.is_some();
                    if let Some(called_class) = &closure.context.called_class {
                        context.called_classes.push(called_class.to_string());
                    }
                    let result = invoke_native_function_with_metadata_strict(
                        context,
                        function,
                        &closure_arguments,
                        metadata,
                        context.unit.strict_types_for_span(instruction.span),
                    );
                    if pushed_called_class {
                        context.called_classes.pop();
                    }
                    if pushed_scope {
                        context.lexical_scope_classes.pop();
                    }
                    result
                }
                php_runtime::api::CallableValue::BoundMethod { target, method, .. } => {
                    invoke_native_bound_method(
                        context,
                        target,
                        method,
                        arguments,
                        metadata,
                        context.unit.strict_types_for_span(instruction.span),
                        caller_function,
                    )
                }
                php_runtime::api::CallableValue::MethodPlaceholder { target }
                | php_runtime::api::CallableValue::UnresolvedDynamic { target } => {
                    Err(format!("{target} is not callable"))
                }
            },
            Value::String(name) => {
                let name = name.to_string_lossy();
                if let Some((class, method)) = name.split_once("::") {
                    invoke_native_bound_method(
                        context,
                        &php_runtime::api::CallableMethodTarget::Class(class.to_owned()),
                        method,
                        arguments,
                        metadata,
                        context.unit.strict_types_for_span(instruction.span),
                        caller_function,
                    )
                } else {
                    invoke_native_named_callable(context, &name, arguments, instruction, metadata)
                }
            }
            Value::Object(object) => invoke_native_bound_method(
                context,
                &php_runtime::api::CallableMethodTarget::Object(object),
                "__invoke",
                arguments,
                metadata,
                context.unit.strict_types_for_span(instruction.span),
                caller_function,
            ),
            Value::Array(array) => {
                let target = array
                    .get(&php_runtime::api::ArrayKey::Int(0))
                    .cloned()
                    .map(dereference_native_callable_value)
                    .ok_or_else(|| "callable array target is missing".to_owned())?;
                let method = array
                    .get(&php_runtime::api::ArrayKey::Int(1))
                    .cloned()
                    .map(dereference_native_callable_value)
                    .ok_or_else(|| "callable array method is missing".to_owned())?;
                let Value::String(method) = method else {
                    return Err("callable array method must be a string".to_owned());
                };
                let target = match target {
                    Value::Object(object) => php_runtime::api::CallableMethodTarget::Object(object),
                    Value::String(class) => {
                        php_runtime::api::CallableMethodTarget::Class(class.to_string_lossy())
                    }
                    value => {
                        return Err(format!(
                            "callable array target must be object or class-string, {} given",
                            native_value_type_name(&value)
                        ));
                    }
                };
                invoke_native_bound_method(
                    context,
                    &target,
                    &method.to_string_lossy(),
                    arguments,
                    metadata,
                    context.unit.strict_types_for_span(instruction.span),
                    caller_function,
                )
            }
            value => Err(format!(
                "{} is not callable",
                native_value_type_name(&value)
            )),
        }
    })();
    Some(result)
}

pub(super) fn execute_native_dynamic_constructor(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    encoded: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::DynamicNewObject { args, .. } = &instruction.kind else {
        return None;
    };
    let Some((class_name, arguments)) = encoded.split_first() else {
        return Some(Err("dynamic class operand is missing".to_owned()));
    };
    let class_name = match context.decode(*class_name) {
        Ok(Value::Reference(reference)) => reference.get(),
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
    };
    let Value::String(class_name) = class_name else {
        return Some(Err(format!(
            "Class name must be a valid object or a string, {} given",
            native_value_type_name(&class_name)
        )));
    };
    let class_name = class_name.to_string_lossy();
    let result = (|| -> Result<i64, String> {
        if arguments.len() != args.len() {
            return Err(format!(
                "dynamic constructor argument metadata mismatch: expected {}, received {}",
                args.len(),
                arguments.len()
            ));
        }
        native_autoload_class(context, &class_name, instruction)?;
        if let Some(result) = construct_native_internal_class(context, &class_name, arguments) {
            return result;
        }
        if native_external_class_exists(context, &class_name) {
            return create_native_external_object(context, &class_name, arguments, instruction);
        }

        let class = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(&class_name))
            .cloned()
            .ok_or_else(|| format!("E_PHP_VM_UNKNOWN_CLASS: Class {class_name} not found"))?;
        if class.flags.is_abstract
            || class.flags.is_interface
            || class.flags.is_trait
            || class.flags.is_enum
        {
            return Err(format!(
                "Cannot instantiate {} {}",
                class_name, class.display_name
            ));
        }
        native_prepare_runtime_class_constants(context, None, &class, instruction)?;
        let runtime_class = native_runtime_class(context, &class)?;
        let object = php_runtime::api::ObjectRef::new_with_display_name(
            &runtime_class,
            class.display_name.clone(),
        );
        let receiver = context.encode(Value::Object(object))?;
        if let Some(constructor) = native_method_in_hierarchy(context, &class.name, "__construct") {
            let mut constructor_arguments = Vec::with_capacity(arguments.len() + 1);
            constructor_arguments.push(receiver);
            constructor_arguments.extend_from_slice(arguments);
            let mut metadata = Vec::with_capacity(args.len() + 1);
            metadata.push(php_ir::instruction::IrCallArg {
                name: None,
                value: php_ir::Operand::Register(php_ir::RegId::new(0)),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            });
            metadata.extend(args.iter().cloned());
            let _ = invoke_native_function_with_metadata_strict(
                context,
                constructor,
                &constructor_arguments,
                Some(&metadata),
                context.unit.strict_types_for_span(instruction.span),
            )?;
        }
        Ok(receiver)
    })();
    Some(result)
}

pub(super) fn execute_native_generator_method(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    encoded: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallMethod { method, .. } = &instruction.kind else {
        return None;
    };
    let receiver = encoded.first()?;
    let generator = match context.decode(*receiver) {
        Ok(Value::Generator(generator)) => generator,
        Ok(_) => return None,
        Err(error) => return Some(Err(error)),
    };
    let result = (|| -> Result<i64, String> {
        let iterator = context.generator_iterator(generator.clone())?;
        let ensure_started = |context: &mut NativeExecutionContext<'_>| {
            if generator.state() == php_runtime::api::GeneratorState::Created {
                context.iterator_next(iterator).map(|_| ())
            } else {
                Ok(())
            }
        };
        match method.to_ascii_lowercase().as_str() {
            "rewind" => {
                ensure_started(context)?;
                if !context.generator_can_rewind(iterator) {
                    return Err(
                        "E_PHP_THROW:Exception:Cannot rewind a generator that was already run"
                            .to_owned(),
                    );
                }
                context.encode(Value::Null)
            }
            "valid" => {
                ensure_started(context)?;
                context.encode(Value::Bool(
                    generator.state() == php_runtime::api::GeneratorState::Suspended,
                ))
            }
            "current" => {
                ensure_started(context)?;
                context.encode(generator.current_value().unwrap_or(Value::Null))
            }
            "key" => {
                ensure_started(context)?;
                context.encode(generator.current_key().unwrap_or(Value::Null))
            }
            "next" => {
                ensure_started(context)?;
                if generator.state() == php_runtime::api::GeneratorState::Suspended {
                    let _ = context.iterator_next(iterator)?;
                }
                context.encode(Value::Null)
            }
            "send" => {
                ensure_started(context)?;
                let value = encoded
                    .get(1)
                    .copied()
                    .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
                let next = context.generator_resume(
                    iterator,
                    php_jit::JitNativeResumeInputKind::VALUE,
                    value,
                )?;
                context.encode(next.map_or(Value::Null, |(_, value)| value))
            }
            "throw" => {
                ensure_started(context)?;
                let value = encoded
                    .get(1)
                    .copied()
                    .ok_or_else(|| "Generator::throw() expects an exception".to_owned())?;
                let next = context.generator_resume(
                    iterator,
                    php_jit::JitNativeResumeInputKind::THROW,
                    value,
                )?;
                context.encode(next.map_or(Value::Null, |(_, value)| value))
            }
            "getreturn" => {
                if generator.state() != php_runtime::api::GeneratorState::Closed {
                    return Err(
                        "Cannot get return value of a generator that hasn't returned".to_owned(),
                    );
                }
                context.encode(generator.return_value().unwrap_or(Value::Null))
            }
            _ => Err(format!("Call to undefined method Generator::{method}()")),
        }
    })();
    Some(result)
}

pub(super) fn finish_native_fiber_outcome(
    context: &mut NativeExecutionContext<'_>,
    fiber: &php_runtime::api::FiberRef,
    handle: php_jit::JitFunctionHandle,
    arguments: Vec<i64>,
    outcome: php_jit::JitI64InvokeOutcome,
) -> Result<i64, String> {
    match outcome {
        php_jit::JitI64InvokeOutcome::Returned(value)
        | php_jit::JitI64InvokeOutcome::SideExit {
            status: 1 | 2,
            value,
            ..
        } => {
            fiber.terminate(Some(context.decode(value)?));
            context.fiber_executions.remove(&fiber.id());
            context.encode(Value::Null)
        }
        php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            state,
        } if status == php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32 => {
            fiber.set_state(php_runtime::api::FiberState::Suspended);
            context.fiber_executions.insert(
                fiber.id(),
                NativeFiberExecution {
                    handle,
                    arguments,
                    state,
                    nested: context.pending_nested_fiber_execution.take().map(Box::new),
                },
            );
            context.encode(context.decode(value)?)
        }
        php_jit::JitI64InvokeOutcome::SideExit { status, value, .. }
            if status == php_jit::JitCallStatus::THROW.0 as i32 =>
        {
            fiber.set_state(php_runtime::api::FiberState::Errored);
            let (class, message, _) = context
                .decode(value)
                .ok()
                .and_then(super::super::native_exception_fields)
                .unwrap_or_else(|| {
                    (
                        "Error".to_owned(),
                        "unknown native exception".to_owned(),
                        "<unknown>".to_owned(),
                    )
                });
            Err(format!("E_PHP_THROW:{class}:{message}"))
        }
        php_jit::JitI64InvokeOutcome::SideExit { status, .. } => {
            fiber.set_state(php_runtime::api::FiberState::Errored);
            Err(format!("native fiber returned status {status}"))
        }
    }
}

pub(super) fn execute_native_fiber_suspend(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    encoded: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallStaticMethod {
        class_name, method, ..
    } = &instruction.kind
    else {
        return None;
    };
    if !class_name.eq_ignore_ascii_case("Fiber") || !method.eq_ignore_ascii_case("suspend") {
        return None;
    }
    if context.active_fiber.is_none() {
        return Some(Err(
            "E_PHP_THROW:FiberError:Cannot suspend outside of a fiber".to_owned(),
        ));
    }
    context.pending_fiber_suspension_value = Some(
        encoded
            .first()
            .copied()
            .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX)),
    );
    Some(Err("E_PHP_SUSPEND_FIBER".to_owned()))
}

pub(super) fn execute_native_fiber_method(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    encoded: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallMethod { method, .. } = &instruction.kind else {
        return None;
    };
    let receiver = encoded.first()?;
    let fiber = match context.decode(*receiver) {
        Ok(Value::Fiber(fiber)) => fiber,
        Ok(_) => return None,
        Err(error) => return Some(Err(error)),
    };
    let result = (|| -> Result<i64, String> {
        match method.to_ascii_lowercase().as_str() {
            "isstarted" => context.encode(Value::Bool(
                fiber.state() != php_runtime::api::FiberState::NotStarted,
            )),
            "issuspended" => context.encode(Value::Bool(
                fiber.state() == php_runtime::api::FiberState::Suspended,
            )),
            "isrunning" => context.encode(Value::Bool(
                fiber.state() == php_runtime::api::FiberState::Running,
            )),
            "isterminated" => context.encode(Value::Bool(
                fiber.state() == php_runtime::api::FiberState::Terminated,
            )),
            "getreturn" => {
                if fiber.state() != php_runtime::api::FiberState::Terminated {
                    let state = if fiber.state() == php_runtime::api::FiberState::NotStarted {
                        "been started"
                    } else {
                        "returned"
                    };
                    return Err(format!(
                        "E_PHP_THROW:FiberError:Cannot get fiber return value: The fiber has not {state}"
                    ));
                }
                context.encode(fiber.return_value().unwrap_or(Value::Null))
            }
            "start" => {
                if fiber.state() != php_runtime::api::FiberState::NotStarted {
                    return Err(
                        "E_PHP_THROW:FiberError:Cannot start a fiber that has already been started"
                            .to_owned(),
                    );
                }
                let Value::Callable(callable) = fiber.callable() else {
                    return Err(
                        "Fiber::__construct(): Argument #1 ($callback) must be of type callable"
                            .to_owned(),
                    );
                };
                let php_runtime::api::CallableValue::Closure(closure) = callable.as_ref() else {
                    return Err("Fiber callback must resolve to a native closure".to_owned());
                };
                let function = php_ir::FunctionId::new(closure.function);
                let handle = ensure_native_entry(context, function)?;
                let mut arguments = closure
                    .captures
                    .iter()
                    .map(|capture| {
                        if let Some(reference) = capture.reference() {
                            context.encode(Value::Reference(reference))
                        } else {
                            context.encode(capture.value().cloned().unwrap_or(Value::Null))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                arguments.extend_from_slice(&encoded[1..]);
                fiber.set_state(php_runtime::api::FiberState::Running);
                let previous_fiber = context.active_fiber.replace(fiber.id());
                let outcome = handle
                    .invoke_i64_with_deopt(&arguments, php_jit::JIT_RUNTIME_ABI_HASH)
                    .map_err(|error| format!("native fiber invocation failed: {error:?}"))?;
                context.active_fiber = previous_fiber;
                finish_native_fiber_outcome(context, &fiber, handle, arguments, outcome)
            }
            "resume" | "throw" => {
                if fiber.state() != php_runtime::api::FiberState::Suspended {
                    return Err(
                        "E_PHP_THROW:FiberError:Cannot resume a fiber that is not suspended"
                            .to_owned(),
                    );
                }
                let mut execution = context
                    .fiber_executions
                    .remove(&fiber.id())
                    .ok_or_else(|| "native fiber suspension state is missing".to_owned())?;
                let value = encoded
                    .get(1)
                    .copied()
                    .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
                fiber.set_state(php_runtime::api::FiberState::Running);
                let kind = if method.eq_ignore_ascii_case("throw") {
                    php_jit::JitNativeResumeInputKind::THROW
                } else {
                    php_jit::JitNativeResumeInputKind::VALUE
                };
                if let Some(mut nested) = execution.nested.take() {
                    let previous_fiber = context.active_fiber.replace(fiber.id());
                    let nested_outcome = nested
                        .handle
                        .invoke_i64_suspension_resume_with_native_unwind(
                            &nested.arguments,
                            &nested.state,
                            kind,
                            value,
                            php_jit::JIT_RUNTIME_ABI_HASH,
                            |types, value| native_catch_matches(context, types, value),
                        )
                        .map_err(|error| format!("native nested fiber resume failed: {error:?}"))?;
                    context.active_fiber = previous_fiber;
                    match nested_outcome {
                        php_jit::JitI64InvokeOutcome::Returned(value)
                        | php_jit::JitI64InvokeOutcome::SideExit {
                            status: 1 | 2,
                            value,
                            ..
                        } => {
                            context.completed_nested_fiber_call = Some((
                                execution.state.function_id,
                                execution.state.continuation_id,
                                value,
                            ));
                            let previous_fiber = context.active_fiber.replace(fiber.id());
                            let outcome = execution
                                .handle
                                .invoke_i64_same_artifact_transition(
                                    &execution.state,
                                    php_jit::JIT_RUNTIME_ABI_HASH,
                                )
                                .map_err(|error| {
                                    format!("native fiber caller resume failed: {error:?}")
                                })?;
                            context.active_fiber = previous_fiber;
                            return finish_native_fiber_outcome(
                                context,
                                &fiber,
                                execution.handle,
                                execution.arguments,
                                outcome,
                            );
                        }
                        php_jit::JitI64InvokeOutcome::SideExit {
                            status,
                            value,
                            state,
                        } if status == php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32 => {
                            nested.state = state;
                            execution.nested = Some(nested);
                            context.fiber_executions.insert(fiber.id(), execution);
                            fiber.set_state(php_runtime::api::FiberState::Suspended);
                            return context.encode(context.decode(value)?);
                        }
                        php_jit::JitI64InvokeOutcome::SideExit { status, .. } => {
                            return Err(format!("native nested fiber returned status {status}"));
                        }
                    }
                }
                let previous_fiber = context.active_fiber.replace(fiber.id());
                let outcome = execution
                    .handle
                    .invoke_i64_suspension_resume_with_native_unwind(
                        &execution.arguments,
                        &execution.state,
                        kind,
                        value,
                        php_jit::JIT_RUNTIME_ABI_HASH,
                        |types, value| native_catch_matches(context, types, value),
                    )
                    .map_err(|error| format!("native fiber resume failed: {error:?}"))?;
                context.active_fiber = previous_fiber;
                finish_native_fiber_outcome(
                    context,
                    &fiber,
                    execution.handle,
                    execution.arguments,
                    outcome,
                )
            }
            _ => Err(format!("Call to undefined method Fiber::{method}()")),
        }
    })();
    Some(result)
}

pub(super) fn invoke_native_callable_value(
    context: &mut NativeExecutionContext<'_>,
    callable: Value,
    arguments: &[Value],
    source: &php_ir::Instruction,
    metadata: Option<Vec<php_ir::instruction::IrCallArg>>,
) -> Result<i64, String> {
    invoke_native_callable_value_from(context, callable, arguments, source, metadata, None)
}

pub(super) fn invoke_native_callable_value_from(
    context: &mut NativeExecutionContext<'_>,
    callable: Value,
    arguments: &[Value],
    source: &php_ir::Instruction,
    metadata: Option<Vec<php_ir::instruction::IrCallArg>>,
    caller_function: Option<u32>,
) -> Result<i64, String> {
    let mut encoded = Vec::with_capacity(arguments.len() + 1);
    encoded.push(context.encode(callable)?);
    for argument in arguments {
        encoded.push(context.encode(argument.clone())?);
    }
    let metadata = metadata.unwrap_or_else(|| {
        arguments
            .iter()
            .map(|_| php_ir::instruction::IrCallArg {
                name: None,
                value: php_ir::Operand::Register(php_ir::RegId::new(0)),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            })
            .collect()
    });
    let call = php_ir::Instruction {
        id: source.id,
        span: source.span,
        kind: php_ir::InstructionKind::CallCallable {
            dst: php_ir::RegId::new(0),
            callee: php_ir::Operand::Register(php_ir::RegId::new(0)),
            args: metadata,
        },
    };
    execute_native_dynamic_callable(context, &call, &encoded, caller_function)
        .unwrap_or_else(|| Err("dynamic callable dispatch was not selected".to_owned()))
}
