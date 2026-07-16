use super::*;

fn call_dispatch_helper_id(instruction: &php_ir::Instruction) -> &'static str {
    match &instruction.kind {
        php_ir::InstructionKind::CallFunction { .. }
        | php_ir::InstructionKind::BindReferenceFromCall { .. } => "call_function",
        php_ir::InstructionKind::CallMethod { .. }
        | php_ir::InstructionKind::BindReferenceFromMethodCall { .. } => "call_method",
        php_ir::InstructionKind::CallStaticMethod { .. } => "call_static_method",
        php_ir::InstructionKind::CallCallable { .. }
        | php_ir::InstructionKind::CallClosure { .. }
        | php_ir::InstructionKind::Pipe { .. } => "call_callable",
        php_ir::InstructionKind::NewObject { .. } => "call_constructor",
        _ => "call_runtime_intrinsic",
    }
}

fn mark_native_function_argument_references(
    arguments: &mut [php_jit::JitNativeCallArgument],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    parameters: &[php_ir::IrParam],
) {
    let variadic_index = parameters.iter().position(|parameter| parameter.variadic);
    let fixed_count = variadic_index.unwrap_or(parameters.len());
    let mut positional = 0usize;
    let mut assigned = vec![false; fixed_count];

    for (index, argument) in arguments.iter_mut().enumerate() {
        let call_argument = metadata.and_then(|metadata| metadata.get(index));
        let parameter = call_argument
            .and_then(|argument| argument.name.as_deref())
            .and_then(|name| {
                parameters[..fixed_count]
                    .iter()
                    .position(|parameter| parameter.name.eq_ignore_ascii_case(name))
            })
            .or_else(|| {
                while positional < fixed_count && assigned[positional] {
                    positional += 1;
                }
                if positional < fixed_count {
                    let index = positional;
                    assigned[index] = true;
                    positional += 1;
                    Some(index)
                } else {
                    variadic_index
                }
            })
            .and_then(|index| parameters.get(index));
        let requires_reference = !call_argument.is_some_and(|argument| argument.unpack)
            && parameter.is_some_and(|parameter| parameter.by_ref);
        if requires_reference {
            argument.flags.0 |= php_jit::JitNativeArgFlags::BY_REFERENCE.0;
        } else {
            argument.flags.0 &= !php_jit::JitNativeArgFlags::BY_REFERENCE.0;
        }
    }
}

/// Typed native call trampoline entry. Target compilation and lookup are
/// requested explicitly; this boundary has no alternate executor entry.
// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_call_dispatch_abi(
    _vm_context: u64,
    frame: *mut php_jit::JitNativeCallFrame,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    if frame.is_null() || out.is_null() {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: The generated caller owns both records for this synchronous
        // call and the pointers were checked for null above.
        let frame = unsafe { &mut *frame };
        if frame.abi_version != php_jit::JIT_RUNTIME_ABI_VERSION
            || frame.struct_size as usize != std::mem::size_of::<php_jit::JitNativeCallFrame>()
            || (frame.argument_count != 0 && frame.arguments == 0)
            || (frame.local_count != 0 && frame.local_slots == 0)
        {
            return (
                php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
                php_jit::JitCallStatus::ABI_MISMATCH,
                None,
            );
        }
        let mut empty_arguments = [];
        let arguments: &mut [php_jit::JitNativeCallArgument] = if frame.argument_count == 0 {
            &mut empty_arguments
        } else {
            // SAFETY: ABI validation above proves a non-null caller-owned
            // argument table with `argument_count` live entries.
            unsafe {
                std::slice::from_raw_parts_mut(
                    frame.arguments as *mut php_jit::JitNativeCallArgument,
                    frame.argument_count as usize,
                )
            }
        };
        let encoded = arguments
            .iter()
            .map(|argument| argument.value.payload as i64)
            .collect::<Vec<_>>();
        let local_values = if frame.local_count == 0 {
            Vec::new()
        } else {
            // SAFETY: ABI validation above proves a non-null caller-owned
            // local table with `local_count` live entries.
            unsafe {
                std::slice::from_raw_parts(
                    frame.local_slots as *const php_jit::JitAbiSlot,
                    frame.local_count as usize,
                )
            }
            .iter()
            .map(|slot| slot.payload as i64)
            .collect::<Vec<_>>()
        };
        let outcome = with_native_context(|context| {
            let mut telemetry = context.runtime_telemetry.borrow_mut();
            telemetry.counters.native_call_dynamic =
                telemetry.counters.native_call_dynamic.saturating_add(1);
            drop(telemetry);
            let instruction = context
                .instruction_for_source(
                    frame.function_id,
                    frame.source_block_id,
                    frame.source_instruction_id,
                )
                .cloned();
            let Some(instruction) = instruction else {
                return Err(format!(
                    "E_PHP_VM_UNRESOLVED_CALLABLE: native call site is unavailable at function={} block={} instruction={}",
                    frame.function_id, frame.source_block_id, frame.source_instruction_id,
                ));
            };
            let instruction_kind = format!("{:?}", instruction.kind);
            let helper_id = call_dispatch_helper_id(&instruction);
            if context.options.collect_counters {
                context.enter_runtime_helper(helper_id);
            }
            let outcome = (|| {
            let completed_nested_fiber_matches = context
                .completed_nested_fiber_call
                .as_ref()
                .is_some_and(|(function, continuation, _)| {
                    *function == frame.function_id && *continuation == frame.continuation_id
                });
            if completed_nested_fiber_matches
                && let Some((_, _, value)) = context.completed_nested_fiber_call.take()
            {
                return Ok(value);
            }
            if let Some(result) =
                execute_native_static_property(context, &instruction, &encoded, frame.function_id)
            {
                return result;
            }
            if let Some(result) = execute_native_fiber_suspend(context, &instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_instanceof(context, &instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_resolve_callable(context, &instruction) {
                return result;
            }
            if let Some(result) = execute_native_acquire_callable(context, &instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_bind_global(context, &instruction) {
                return result;
            }
            if matches!(
                instruction.kind,
                php_ir::InstructionKind::CallCallable { .. }
                    | php_ir::InstructionKind::CallClosure { .. }
                    | php_ir::InstructionKind::Pipe { .. }
            ) && frame.target.function_id != u32::MAX
            {
                let function = php_ir::FunctionId::new(frame.target.function_id);
                if native_function_is_generator(context, function) {
                    let arguments = encoded
                        .iter()
                        .map(|value| context.decode(*value))
                        .collect::<Result<Vec<_>, _>>()?;
                    return context.encode(Value::Generator(php_runtime::api::GeneratorRef::new(
                        function.raw(),
                        arguments,
                    )));
                }
                let metadata = match &instruction.kind {
                    php_ir::InstructionKind::CallCallable { args, .. }
                    | php_ir::InstructionKind::CallClosure { args, .. } => Some(args.as_slice()),
                    php_ir::InstructionKind::Pipe { .. } => None,
                    _ => None,
                };
                return invoke_native_function_with_metadata_strict(
                    context,
                    function,
                    &encoded,
                    metadata,
                    context.unit.strict_types_for_span(instruction.span),
                );
            }
            if let Some(result) =
                execute_native_dynamic_constructor(context, &instruction, &encoded)
            {
                return result;
            }
            if frame.target.function_id == u32::MAX
                && frame.target.kind != php_jit::JitNativeCallKind::FUNCTION
                && let Some(result) =
                    execute_native_dynamic_callable(
                        context,
                        &instruction,
                        &encoded,
                        Some(frame.function_id),
                    )
            {
                return result;
            }
            if let Some(result) = execute_native_property_instruction(
                context,
                &instruction,
                &encoded,
                frame.function_id,
            ) {
                return result;
            }
            if let Some(result) =
                execute_native_class_constant(context, &instruction, frame.function_id)
            {
                return result;
            }
            if let Some(result) = execute_native_internal_class(context, &instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_array_object(context, &instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_enum_static_method(context, &instruction, &encoded)
            {
                return result;
            }
            if let Some(result) = execute_native_generator_method(context, &instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_fiber_method(context, &instruction, &encoded) {
                return result;
            }
            if let php_ir::InstructionKind::NewObject {
                display_class_name, ..
            } = &instruction.kind
                && display_class_name.eq_ignore_ascii_case("Fiber")
            {
                let callback = encoded
                    .first()
                    .copied()
                    .ok_or_else(|| "Fiber::__construct() expects a callback".to_owned())?;
                let callback = context.decode(callback)?;
                if !matches!(callback, Value::Callable(_)) {
                    return Err(
                        "Fiber::__construct(): Argument #1 ($callback) must be of type callable"
                            .to_owned(),
                    );
                }
                return context.encode(Value::Fiber(php_runtime::api::FiberRef::new(callback)));
            }
            if let php_ir::InstructionKind::NewObject {
                display_class_name, ..
            } = &instruction.kind
                // A known constructor call carries the already allocated
                // receiver as argument zero. Let the unified native-call path
                // below invoke that constructor; allocating and returning a
                // second object here would skip the constructor entirely.
                && frame.target.function_id == u32::MAX
            {
                let display_class_name = native_resolve_scoped_class_name(
                    context,
                    display_class_name,
                    frame.function_id,
                )?;
                let normalized = normalize_class_name(&display_class_name);
                if let Some(class) = context
                    .unit
                    .classes
                    .iter()
                    .find(|class| class.name == normalized)
                    .cloned()
                {
                    let mut parent = class.parent.clone();
                    let mut throwable_parent = false;
                    let mut visited = std::collections::BTreeSet::new();
                    while let Some(name) = parent.take() {
                        let name = normalize_class_name(&name);
                        if !visited.insert(name.clone()) {
                            break;
                        }
                        if (name.ends_with("exception") || name.ends_with("error"))
                            && (php_std::ExtensionRegistry::standard_library()
                                .enabled_class(&name)
                                .is_some()
                                || matches!(
                                    name.as_str(),
                                    "exception"
                                        | "error"
                                        | "typeerror"
                                        | "valueerror"
                                        | "argumentcounterror"
                                        | "fibererror"
                                ))
                        {
                            throwable_parent = true;
                            break;
                        }
                        parent = context
                            .unit
                            .classes
                            .iter()
                            .find(|candidate| candidate.name == name)
                            .and_then(|candidate| candidate.parent.clone())
                            .or_else(|| {
                                native_external_class(context, &name)
                                    .and_then(|(_, candidate)| candidate.parent)
                            });
                    }
                    if throwable_parent {
                        let message = encoded
                            .first()
                            .map(|message| context.decode(*message))
                            .transpose()?
                            .map(native_string)
                            .transpose()?
                            .map_or_else(String::new, |message| {
                                String::from_utf8_lossy(&message).into_owned()
                            });
                        return encode_native_throwable_at(
                            context,
                            &class.display_name,
                            &message,
                            instruction.span,
                        );
                    }
                    native_prepare_runtime_class_constants(
                        context,
                        None,
                        &class,
                        &instruction,
                    )?;
                    let runtime_class = native_runtime_class(context, &class)?;
                    let object = php_runtime::api::ObjectRef::new_with_display_name(
                        &runtime_class,
                        class.display_name,
                    );
                    return context.encode(Value::Object(object));
                }
                if native_external_class(context, &display_class_name).is_none()
                    && context.autoload_in_progress.insert(normalized.clone())
                {
                    let callbacks = context.autoload_callbacks.clone();
                    for callback in callbacks {
                        invoke_native_callable_value(
                            context,
                            callback,
                            &[Value::String(PhpString::from_bytes(
                                display_class_name.as_bytes().to_vec(),
                            ))],
                            &instruction,
                            None,
                        )?;
                        if native_external_class(context, &display_class_name).is_some() {
                            break;
                        }
                    }
                    context.autoload_in_progress.remove(&normalized);
                }
                if native_external_class(context, &display_class_name).is_some() {
                    if let Some(parent) = native_external_class(context, &display_class_name)
                        .and_then(|(_, class)| class.parent_display_name.or(class.parent))
                    {
                        native_autoload_class(context, &parent, &instruction)?;
                    }
                    return create_native_external_object(
                        context,
                        &display_class_name,
                        &encoded,
                        &instruction,
                    );
                }
            }
            if let php_ir::InstructionKind::CallMethod { method, .. } = &instruction.kind
                && (method.eq_ignore_ascii_case("getMessage")
                    || method.eq_ignore_ascii_case("getTrace")
                    || method.eq_ignore_ascii_case("getCode")
                    || method.eq_ignore_ascii_case("getPrevious"))
                && let Some(receiver) = encoded.first()
            {
                let decoded = context.decode(*receiver)?;
                let name = if method.eq_ignore_ascii_case("getTrace") {
                    "trace"
                } else if method.eq_ignore_ascii_case("getCode") {
                    "code"
                } else if method.eq_ignore_ascii_case("getPrevious") {
                    "previous"
                } else {
                    "message"
                };
                let fallback = if method.eq_ignore_ascii_case("getTrace") {
                    Value::Array(php_runtime::api::PhpArray::new())
                } else if method.eq_ignore_ascii_case("getCode") {
                    Value::Int(0)
                } else {
                    Value::Null
                };
                let value = match decoded {
                    Value::Array(exception) => {
                        let key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                            name.as_bytes().to_vec(),
                        ));
                        exception.get(&key).cloned().unwrap_or(fallback)
                    }
                    Value::Object(exception) => {
                        exception.get_property(name).unwrap_or(fallback)
                    }
                    value => {
                        return Err(format!(
                            "Call to a member function {method}() on {}",
                            native_value_type_name(&value)
                        ));
                    }
                };
                return context.encode(value);
            }
            if let php_ir::InstructionKind::CallMethod { method, args, .. } = &instruction.kind
                && let Some(receiver) = encoded.first()
            {
                let receiver_value = match context.decode(*receiver).map_err(|error| {
                    format!("{method}() native receiver could not be decoded: {error}")
                })? {
                    Value::Reference(reference) => reference.get(),
                    value => value,
                };
                let Value::Object(object) = receiver_value else {
                    return Err(format!(
                        "Call to a member function {method}() on {}",
                        native_value_type_name(&receiver_value)
                    ));
                };
                let class_name = object.class_name();
                let function = native_calling_class(context, frame.function_id)
                    .and_then(|class| {
                        class
                            .methods
                            .iter()
                            .find(|entry| {
                                entry.name.eq_ignore_ascii_case(method) && entry.flags.is_private
                            })
                            .map(|entry| entry.function)
                    })
                    .or_else(|| native_method_in_hierarchy(context, &class_name, method));
                if let Some(function) = function {
                    emit_native_deprecated_call(context, function, &instruction);
                    if let Some(error) =
                        native_method_access_error(context, function, frame.function_id, false)
                    {
                        if let Some(magic) =
                            native_method_in_hierarchy(context, &class_name, "__call")
                        {
                            let method_name = context.encode(Value::String(
                                PhpString::from_bytes(method.as_bytes().to_vec()),
                            ))?;
                            let call_arguments =
                                encode_native_call_arguments_array(context, &encoded[1..])?;
                            return invoke_native_function(
                                context,
                                magic,
                                &[*receiver, method_name, call_arguments],
                            );
                        }
                        return Err(format!("E_PHP_THROW:Error:{error}"));
                    }
                    let is_static_method = context.unit.classes.iter().any(|class| {
                        class
                            .methods
                            .iter()
                            .any(|entry| entry.function == function && entry.flags.is_static)
                    });
                    let call_arguments = if is_static_method {
                        &encoded[1..]
                    } else {
                        &encoded
                    };
                    if native_function_is_generator(context, function) {
                        let arguments = call_arguments
                            .iter()
                            .map(|value| context.decode(*value))
                            .collect::<Result<Vec<_>, _>>()?;
                        return context.encode(Value::Generator(
                            php_runtime::api::GeneratorRef::new(function.raw(), arguments),
                        ));
                    }
                    if is_static_method {
                        context.called_classes.push(class_name.clone());
                    }
                    let result = invoke_native_function_with_metadata_strict(
                        context,
                        function,
                        call_arguments,
                        Some(args),
                        context.unit.strict_types_for_span(instruction.span),
                    );
                    if is_static_method {
                        context.called_classes.pop();
                    }
                    return result;
                }
                if let Some((function, entry)) =
                    native_external_method(context, &class_name, method)
                {
                    if let Some(error) = native_external_method_access_error(
                        context,
                        function,
                        frame.function_id,
                        false,
                    ) {
                        return Err(format!("E_PHP_THROW:Error:{error}"));
                    }
                    let call_arguments = if entry.flags.is_static {
                        &encoded[1..]
                    } else {
                        &encoded
                    };
                    return invoke_native_external_function_with_metadata(
                        context,
                        function,
                        call_arguments,
                        Some(args),
                        Some(class_name),
                        context
                            .unit
                            .strict_types_for_function(php_ir::FunctionId::new(frame.function_id)),
                    );
                }
                if let Some(function) = native_method_in_hierarchy(context, &class_name, "__call") {
                    let method_name = context.encode(Value::String(PhpString::from_bytes(
                        method.as_bytes().to_vec(),
                    )))?;
                    let call_arguments =
                        encode_native_call_arguments_array(context, &encoded[1..])?;
                    return invoke_native_function(
                        context,
                        function,
                        &[*receiver, method_name, call_arguments],
                    );
                }
                if let Some((function, _entry)) =
                    native_external_method(context, &class_name, "__call")
                {
                    if let Some(error) = native_external_method_access_error(
                        context,
                        function,
                        frame.function_id,
                        false,
                    ) {
                        return Err(format!("E_PHP_THROW:Error:{error}"));
                    }
                    let method_name = context.encode(Value::String(PhpString::from_bytes(
                        method.as_bytes().to_vec(),
                    )))?;
                    let call_arguments =
                        encode_native_call_arguments_array(context, &encoded[1..])?;
                    return invoke_native_external_function(
                        context,
                        function,
                        &[*receiver, method_name, call_arguments],
                        Some(class_name),
                        context.unit.strict_types_for_span(instruction.span),
                    );
                }
            }
            if let php_ir::InstructionKind::CallStaticMethod {
                class_name,
                method,
                args,
                ..
            } = &instruction.kind
            {
                if class_name.eq_ignore_ascii_case("Closure") && method.eq_ignore_ascii_case("bind")
                {
                    let closure = encoded
                        .first()
                        .copied()
                        .ok_or_else(|| "Closure::bind() expects a closure".to_owned())?;
                    let closure = context.decode(closure)?;
                    let Value::Callable(callable) = closure else {
                        return Err("Closure::bind() expects a closure".to_owned());
                    };
                    let php_runtime::api::CallableValue::Closure(closure) = callable.as_ref()
                    else {
                        return Err("Closure::bind() expects a closure".to_owned());
                    };
                    let rebound = native_rebind_closure(
                        closure,
                        encoded
                            .get(1)
                            .copied()
                            .map(|value| context.decode(value))
                            .transpose()?,
                        encoded
                            .get(2)
                            .copied()
                            .map(|value| context.decode(value))
                            .transpose()?,
                    )?;
                    return context.encode(rebound);
                }
                let resolved_class = match class_name.to_ascii_lowercase().as_str() {
                    "self" => native_calling_class(context, frame.function_id)
                        .map(|class| class.name.clone()),
                    "static" => context.called_classes.last().cloned().or_else(|| {
                        native_calling_class(context, frame.function_id)
                            .map(|class| class.name.clone())
                    }),
                    "parent" => native_calling_class(context, frame.function_id)
                        .and_then(|class| class.parent.clone()),
                    _ => Some(class_name.clone()),
                };
                if let Some(class) = resolved_class.as_deref() {
                    native_autoload_class(context, class, &instruction)?;
                }
                if let Some(result) = resolved_class.as_deref().and_then(|class| {
                    initialize_native_throwable_parent(context, class, method, &encoded)
                }) {
                    return result;
                }
                let function = resolved_class
                    .as_deref()
                    .and_then(|class| native_method_in_hierarchy(context, class, method));
                if let Some(function) = function {
                    emit_native_deprecated_call(context, function, &instruction);
                    if let Some(error) = native_method_access_error(
                        context,
                        function,
                        frame.function_id,
                        class_name.eq_ignore_ascii_case("static"),
                    ) {
                        if let Some(class) = resolved_class.as_deref()
                            && let Some(magic) =
                                native_method_in_hierarchy(context, class, "__callStatic")
                        {
                            let method_name = context.encode(Value::String(
                                PhpString::from_bytes(method.as_bytes().to_vec()),
                            ))?;
                            let call_arguments =
                                encode_native_call_arguments_array(context, &encoded)?;
                            return invoke_native_function(
                                context,
                                magic,
                                &[method_name, call_arguments],
                            );
                        }
                        return Err(format!("E_PHP_THROW:Error:{error}"));
                    }
                    let is_instance_method = context.unit.classes.iter().any(|class| {
                        class
                            .methods
                            .iter()
                            .any(|entry| entry.function == function && !entry.flags.is_static)
                    });
                    let mut call_arguments =
                        Vec::with_capacity(encoded.len() + usize::from(is_instance_method));
                    if is_instance_method {
                        if frame.receiver_handle == 0 {
                            return Err(format!(
                                "Non-static method {}::{}() cannot be called statically",
                                resolved_class.as_deref().unwrap_or(class_name),
                                method
                            ));
                        }
                        call_arguments.push(frame.receiver_handle as i64);
                    }
                    call_arguments.extend_from_slice(&encoded);
                    let forwarding = matches!(
                        class_name.to_ascii_lowercase().as_str(),
                        "self" | "parent" | "static"
                    );
                    let called_class = if forwarding {
                        context
                            .called_classes
                            .last()
                            .cloned()
                            .or_else(|| resolved_class.clone())
                    } else {
                        resolved_class.clone()
                    };
                    let pushed_called_class = called_class.is_some();
                    if let Some(called_class) = called_class {
                        context.called_classes.push(called_class);
                    }
                    let result = invoke_native_function_with_metadata_strict(
                        context,
                        function,
                        &call_arguments,
                        Some(args),
                        context.unit.strict_types_for_span(instruction.span),
                    );
                    if pushed_called_class {
                        context.called_classes.pop();
                    }
                    return result;
                }
                if let Some(class) = resolved_class.as_deref()
                    && let Some((function, entry)) = native_external_method(context, class, method)
                {
                    if let Some(error) = native_external_method_access_error(
                        context,
                        function,
                        frame.function_id,
                        class_name.eq_ignore_ascii_case("static"),
                    ) {
                        return Err(format!("E_PHP_THROW:Error:{error}"));
                    }
                    if !entry.flags.is_static && frame.receiver_handle == 0 {
                        return Err(format!(
                            "Non-static method {class}::{method}() cannot be called statically"
                        ));
                    }
                    let mut call_arguments =
                        Vec::with_capacity(encoded.len() + usize::from(!entry.flags.is_static));
                    if !entry.flags.is_static {
                        call_arguments.push(frame.receiver_handle as i64);
                    }
                    call_arguments.extend_from_slice(&encoded);
                    return invoke_native_external_function_with_metadata(
                        context,
                        function,
                        &call_arguments,
                        Some(args),
                        Some(class.to_owned()),
                        context
                            .unit
                            .strict_types_for_function(php_ir::FunctionId::new(frame.function_id)),
                    );
                }
                if let Some(class) = resolved_class {
                    let method_name = context.encode(Value::String(PhpString::from_bytes(
                        method.as_bytes().to_vec(),
                    )))?;
                    let call_arguments = encode_native_call_arguments_array(context, &encoded)?;
                    if let Some(function) =
                        native_method_in_hierarchy(context, &class, "__callStatic")
                    {
                        context.called_classes.push(class.clone());
                        let result = invoke_native_function(
                            context,
                            function,
                            &[method_name, call_arguments],
                        );
                        context.called_classes.pop();
                        return result;
                    }
                    if let Some((function, _entry)) =
                        native_external_method(context, &class, "__callStatic")
                    {
                        if let Some(error) = native_external_method_access_error(
                            context,
                            function,
                            frame.function_id,
                            false,
                        ) {
                            return Err(format!("E_PHP_THROW:Error:{error}"));
                        }
                        return invoke_native_external_function(
                            context,
                            function,
                            &[method_name, call_arguments],
                            Some(class),
                            context.unit.strict_types_for_span(instruction.span),
                        );
                    }
                }
            }
            if matches!(
                instruction.kind,
                php_ir::InstructionKind::CallCallable { .. }
            ) && let [bound_object] = encoded.as_slice()
                && let Value::Object(object) = context.decode(*bound_object)?
            {
                let class_name = object.class_name();
                let display_name = context
                    .unit
                    .classes
                    .iter()
                    .find(|class| class.name == normalize_class_name(&class_name))
                    .map_or(class_name, |class| class.display_name.clone());
                return context.encode(Value::String(PhpString::from_bytes(
                    display_name.as_bytes().to_vec(),
                )));
            }
            if let php_ir::InstructionKind::NewObject { args, .. } = &instruction.kind
                && frame.target.function_id != u32::MAX
            {
                let constructor = php_ir::FunctionId::new(frame.target.function_id);
                if let Some(error) =
                    native_method_access_error(context, constructor, frame.function_id, false)
                {
                    // PHP's constructor visibility diagnostic omits the word
                    // "method", unlike an ordinary inaccessible method call.
                    let error = error
                        .replace("private method ", "private ")
                        .replace("protected method ", "protected ");
                    return Err(format!("E_PHP_THROW:Error:{error}"));
                }
                return invoke_native_function_with_metadata_strict(
                    context,
                    constructor,
                    &encoded,
                    Some(args),
                    context.unit.strict_types_for_span(instruction.span),
                );
            }
            let name = match &instruction.kind {
                php_ir::InstructionKind::CallFunction { name, .. }
                | php_ir::InstructionKind::BindReferenceFromCall { name, .. } => Some(name.clone()),
                php_ir::InstructionKind::NewObject {
                    display_class_name, ..
                } => {
                    let display_class_name = native_resolve_scoped_class_name(
                        context,
                        display_class_name,
                        frame.function_id,
                    )?;
                    return Err(format!(
                        "E_PHP_VM_UNKNOWN_CLASS: Class {display_class_name} not found"
                    ));
                }
                php_ir::InstructionKind::Pipe {
                    callable: php_ir::Operand::Register(callable),
                    ..
                } => {
                    let function = context.unit.functions.get(frame.function_id as usize);
                    let resolved = function.and_then(|function| {
                        function
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .find_map(|candidate| match &candidate.kind {
                                php_ir::InstructionKind::ResolveCallable {
                                    dst,
                                    callable:
                                        php_ir::instruction::CallableKind::FunctionName { name },
                                } if dst == callable => Some(name.clone()),
                                _ => None,
                            })
                    });
                    if resolved.is_none() {
                        let value =
                            function.and_then(|function| {
                                function
                                    .blocks
                                    .iter()
                                    .flat_map(|block| &block.instructions)
                                    .find_map(|candidate| match &candidate.kind {
                                        php_ir::InstructionKind::LoadConst { dst, constant }
                                            if dst == callable =>
                                        {
                                            context.unit.constants.get(constant.index()).and_then(
                                                |constant| ir_constant_value(constant).ok(),
                                            )
                                        }
                                        _ => None,
                                    })
                            });
                        return Err(format!(
                            "{} is not callable",
                            value.as_ref().map_or("value", native_value_type_name)
                        ));
                    }
                    resolved
                }
                php_ir::InstructionKind::Pipe { callable, .. } => {
                    let value = match callable {
                        php_ir::Operand::Constant(constant) => context
                            .unit
                            .constants
                            .get(constant.index())
                            .and_then(|constant| ir_constant_value(constant).ok()),
                        _ => None,
                    };
                    return Err(format!(
                        "{} is not callable",
                        value.as_ref().map_or("value", native_value_type_name)
                    ));
                }
                php_ir::InstructionKind::CallCallable {
                    callee: php_ir::Operand::Register(callable),
                    ..
                } => context
                    .unit
                    .functions
                    .get(frame.function_id as usize)
                    .and_then(|function| {
                        function
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .find_map(|candidate| match &candidate.kind {
                                php_ir::InstructionKind::ResolveCallable {
                                    dst,
                                    callable:
                                        php_ir::instruction::CallableKind::FunctionName { name },
                                } if dst == callable => Some(name.clone()),
                                _ => None,
                            })
                    }),
                php_ir::InstructionKind::CallMethod { method, .. } => {
                    let class_name = encoded
                        .first()
                        .and_then(|receiver| context.decode(*receiver).ok())
                        .and_then(|receiver| match receiver {
                            Value::Reference(reference) => match reference.get() {
                                Value::Object(object) => Some(object.class_name()),
                                _ => None,
                            },
                            Value::Object(object) => Some(object.class_name()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "object".to_owned());
                    return Err(format!(
                        "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not implemented"
                    ));
                }
                php_ir::InstructionKind::CallStaticMethod {
                    class_name, method, ..
                } => {
                    let class_name = native_resolve_scoped_class_name(
                        context,
                        class_name,
                        frame.function_id,
                    )?;
                    return Err(format!(
                        "E_PHP_VM_UNKNOWN_METHOD: static method {class_name}::{method} is not implemented"
                    ));
                }
                php_ir::InstructionKind::BindReferenceFromMethodCall { method, .. } => {
                    return Err(format!(
                        "E_PHP_VM_UNKNOWN_METHOD: by-reference method call {method}() is not implemented"
                    ));
                }
                _ => None,
            };
            let Some(name) = name else {
                return Err(format!(
                    "E_PHP_VM_UNRESOLVED_CALLABLE: native call target is unresolved for {:?} at function={} block={} instruction={} target_kind={} target_function={}",
                    instruction.kind,
                    frame.function_id,
                    frame.source_block_id,
                    frame.source_instruction_id,
                    frame.target.kind.0,
                    frame.target.function_id,
                ));
            };
            if matches!(
                instruction.kind,
                php_ir::InstructionKind::CallCallable { .. }
            ) && context.function_id(&name).is_none()
            {
                return Err(format!(
                    "E_PHP_THROW:Error:Call to undefined function {name}()"
                ));
            }
            if let Some(function_id) = context.function_id(&name) {
                emit_native_deprecated_call(context, function_id, &instruction);
                if matches!(
                    instruction.kind,
                    php_ir::InstructionKind::BindReferenceFromCall { .. }
                ) && context
                    .unit
                    .functions
                    .get(function_id.index())
                    .is_some_and(|function| !function.returns_by_ref)
                {
                    let path = context
                        .unit
                        .files
                        .get(instruction.span.file.index())
                        .map_or("<unknown>", |file| file.path.as_str());
                    let line = native_source_line(context, &instruction);
                    context.output.write_bytes(format!(
                        "\nNotice: Only variables should be assigned by reference in {path} on line {line}\n"
                    ));
                }
                if let php_ir::InstructionKind::CallFunction { args, .. } = &instruction.kind
                    && let Some(target) = context.unit.functions.get(function_id.index())
                {
                    let required = target
                        .params
                        .iter()
                        .take_while(|parameter| parameter.required)
                        .count();
                    if args.len() < required {
                        let path = context
                            .unit
                            .files
                            .get(instruction.span.file.index())
                            .map_or("<unknown>", |file| file.path.as_str());
                        let line = native_source_line(context, &instruction);
                        return Err(format!(
                            "E_PHP_THROW:ArgumentCountError:Too few arguments to function {}(), {} passed in {} on line {} and exactly {} expected",
                            target.name,
                            args.len(),
                            path,
                            line,
                            required
                        ));
                    }
                }
                let visible_arguments = encoded
                    .iter()
                    .map(|value| context.decode(*value))
                    .collect::<Result<Vec<_>, _>>()?;
                if context
                    .unit
                    .functions
                    .get(function_id.index())
                    .is_some_and(|function| {
                        function.flags.is_generator
                            || function
                                .blocks
                                .iter()
                                .flat_map(|block| &block.instructions)
                                .any(|instruction| {
                                    matches!(
                                        instruction.kind,
                                        php_ir::InstructionKind::Yield { .. }
                                            | php_ir::InstructionKind::YieldFrom { .. }
                                    )
                                })
                    })
                {
                    return context.encode(Value::Generator(php_runtime::api::GeneratorRef::new(
                        function_id.raw(),
                        visible_arguments,
                    )));
                }
                let metadata = match &instruction.kind {
                    php_ir::InstructionKind::CallFunction { args, .. }
                    | php_ir::InstructionKind::BindReferenceFromCall { args, .. } => {
                        Some(args.as_slice())
                    }
                    _ => None,
                };
                if let Some(parameters) = context
                    .unit
                    .functions
                    .get(function_id.index())
                    .map(|function| function.params.clone())
                {
                    mark_native_function_argument_references(
                        arguments,
                        metadata,
                        &parameters,
                    );
                }
                return invoke_native_function_with_metadata_strict(
                    context,
                    function_id,
                    &encoded,
                    metadata,
                    context.unit.strict_types_for_span(instruction.span),
                );
            }
            let metadata = match &instruction.kind {
                php_ir::InstructionKind::CallFunction { args, .. }
                | php_ir::InstructionKind::BindReferenceFromCall { args, .. } => {
                    Some(args.as_slice())
                }
                _ => None,
            };
            if let Some(function) = context.external_function(&name) {
                if let Some(parameters) = context
                    .dynamic_units
                    .get(function.unit)
                    .and_then(|unit| unit.compiled.unit().functions.get(function.function.index()))
                    .map(|function| function.params.clone())
                {
                    mark_native_function_argument_references(
                        arguments,
                        metadata,
                        &parameters,
                    );
                }
                return invoke_native_external_function_with_metadata(
                    context,
                    function,
                    &encoded,
                    metadata,
                    None,
                    context.unit.strict_types_for_span(instruction.span),
                );
            }
            let builtin_name = if php_std::arginfo::function_metadata_indexed(&name).is_some() {
                name.as_str()
            } else {
                name.rsplit('\\').next().unwrap_or(&name)
            };
            let expanded =
                bind_native_builtin_arguments(context, builtin_name, &encoded, metadata)?;
            execute_native_builtin(
                context,
                builtin_name,
                &expanded,
                &instruction,
                Some((frame.function_id, &local_values)),
            )
            })()
            .map_err(|message| {
                if message.starts_with("native runtime value ") {
                    format!("{message} while executing {instruction_kind}")
                } else {
                    message
                }
            });
            if context.options.collect_counters {
                context.exit_runtime_helper(helper_id);
            }
            outcome
        });
        match outcome {
            Some(Ok(value)) => {
                let status = if frame.flags & (1 << 1) != 0 {
                    php_jit::JitCallStatus::RETURN_REFERENCE
                } else {
                    php_jit::JitCallStatus::RETURN
                };
                (status.0 as i32, status, Some(value))
            }
            Some(Err(message)) if message == "E_PHP_RETHROW" => {
                let value = with_native_context(|context| {
                    let mut throwable = context.pending_throwable.take()?;
                    if let Some(source) = context
                        .instruction_for_source(
                            frame.function_id,
                            frame.source_block_id,
                            frame.source_instruction_id,
                        )
                        .cloned()
                    {
                        throwable = native_throwable_with_call_source(context, throwable, &source);
                    }
                    context.encode(throwable).ok()
                })
                .flatten();
                (
                    php_jit::JitCallStatus::THROW.0 as i32,
                    php_jit::JitCallStatus::THROW,
                    value,
                )
            }
            Some(Err(message)) if message.starts_with("E_PHP_THROW:") => {
                let payload = message.trim_start_matches("E_PHP_THROW:");
                let (class, message) = payload.split_once(':').unwrap_or(("Error", payload));
                let value = with_native_context(|context| {
                    let target = (frame.target.function_id != u32::MAX)
                        .then(|| php_ir::FunctionId::new(frame.target.function_id))
                        .and_then(|function| context.unit.functions.get(function.index()))
                        .cloned();
                    if class.eq_ignore_ascii_case("TypeError")
                        && message.contains("Argument #")
                        && let Some(target) = target
                    {
                        let encoded =
                            encode_native_throwable_at(context, class, message, target.span)
                                .ok()?;
                        let throwable = context.decode(encoded).ok()?;
                        let arguments = arguments
                            .iter()
                            .map(|argument| context.decode(argument.value.payload as i64))
                            .collect::<Result<Vec<_>, _>>()
                            .ok()?;
                        let mut throwable =
                            native_throwable_with_frame(throwable, &target.name, arguments);
                        if let Some(source) = context
                            .instruction_for_source(
                                frame.function_id,
                                frame.source_block_id,
                                frame.source_instruction_id,
                            )
                            .cloned()
                        {
                            throwable =
                                native_throwable_with_call_source(context, throwable, &source);
                        }
                        return context.encode(throwable).ok();
                    }
                    context
                        .instruction_for_source(
                            frame.function_id,
                            frame.source_block_id,
                            frame.source_instruction_id,
                        )
                        .cloned()
                        .and_then(|source| {
                            encode_native_throwable_at(context, class, message, source.span).ok()
                        })
                        .or_else(|| encode_native_throwable(context, class, message).ok())
                })
                .flatten();
                (
                    php_jit::JitCallStatus::THROW.0 as i32,
                    php_jit::JitCallStatus::THROW,
                    value,
                )
            }
            Some(Err(message)) if message == "E_PHP_SUSPEND_FIBER" => {
                let value =
                    with_native_context(|context| context.pending_fiber_suspension_value.take())
                        .flatten();
                (
                    php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32,
                    php_jit::JitCallStatus::SUSPEND_FIBER,
                    value,
                )
            }
            Some(Err(message)) if message.starts_with("E_PHP_EXIT:") => {
                let value = message
                    .trim_start_matches("E_PHP_EXIT:")
                    .parse::<i64>()
                    .ok();
                (
                    php_jit::JitCallStatus::EXIT.0 as i32,
                    php_jit::JitCallStatus::EXIT,
                    value,
                )
            }
            Some(Err(message)) => {
                let _ =
                    with_native_context(|context| publish_native_call_diagnostic(context, message));
                (
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                    php_jit::JitCallStatus::RUNTIME_ERROR,
                    None,
                )
            }
            None => (
                php_jit::JitCallStatus::COMPILE_REQUIRED.0 as i32,
                php_jit::JitCallStatus::COMPILE_REQUIRED,
                None,
            ),
        }
    }));
    let (status, call_status, value) = result.unwrap_or((
        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        php_jit::JitCallStatus::RUNTIME_ERROR,
        None,
    ));
    // SAFETY: `out` is a checked, caller-owned result record.
    unsafe {
        out.write(php_jit::JitCallResult {
            status: call_status,
            detail: status as u32,
            value: value.map_or_else(php_jit::JitAbiSlot::default, |value| php_jit::JitAbiSlot {
                tag: 3,
                flags: 0,
                payload: value as u64,
            }),
        });
    }
    status
}
