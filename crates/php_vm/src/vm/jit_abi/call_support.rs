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

#[derive(Clone)]
struct NativeSuppliedCallArgument {
    name: Option<String>,
    value: i64,
}

pub(super) fn authoritative_native_call_value_is_admitted(
    context: &NativeRequestColdState<'_>,
    mut encoded: i64,
) -> bool {
    for _ in 0..16 {
        let Some(index) = NativeRequestColdState::direct_value_index(encoded) else {
            // Immediate scalars and unit-local constants are already native
            // call operands. The binder stabilizes non-immediate constants
            // directly when it transfers ownership to the callee.
            return php_jit::jit_decode_runtime_value(encoded).is_none()
                && context.native_encoded_value_kind(encoded).is_some();
        };
        let Some(slot) = context
            .direct_value_slots
            .get(index)
            .filter(|slot| slot.refcount != 0)
        else {
            return false;
        };
        match slot.kind {
            php_jit::JIT_NATIVE_VALUE_VIEW_STRING
            | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            | php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT
            | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
            | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE
            | php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
            | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER => return true,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR => return true,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                if slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                    && slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY =>
            {
                encoded = slot.payload as i64;
            }
            _ => return false,
        }
    }
    false
}

pub(super) fn authoritative_native_call_arguments_are_admitted(
    context: &NativeRequestColdState<'_>,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> bool {
    arguments.iter().enumerate().all(|(index, value)| {
        if !authoritative_native_call_value_is_admitted(context, *value) {
            return false;
        }
        metadata
            .and_then(|metadata| metadata.get(index))
            .is_none_or(|argument| {
                !argument.unpack || context.direct_array_entries_for(*value).is_some()
            })
    })
}

/// Expands one PHP call argument vector while preserving the native value
/// handles stored in direct arrays.  Compatibility arrays are already on a
/// cold boundary; their temporary encodings are reported to the caller so
/// they can be released after binding.
fn expand_native_user_call_arguments(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    compatibility_owners: &mut Vec<i64>,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> Result<Vec<NativeSuppliedCallArgument>, NativeCallControl> {
    let Some(metadata) = metadata else {
        return Ok(arguments
            .iter()
            .copied()
            .map(|value| NativeSuppliedCallArgument { name: None, value })
            .collect());
    };
    let mut supplied = Vec::with_capacity(arguments.len());
    for (argument, value) in metadata.iter().zip(arguments) {
        if !argument.unpack {
            supplied.push(NativeSuppliedCallArgument {
                name: argument.name.clone(),
                value: *value,
            });
            continue;
        }
        if let Some(entries) = context.direct_array_entries_for(*value).map(<[_]>::to_vec) {
            for entry in entries {
                let name = match context.native_encoded_value_kind(entry.key) {
                    Some(NativeEncodedValueKind::Int) => None,
                    Some(NativeEncodedValueKind::String) => {
                        let bytes =
                            context.native_string_name_bytes(entry.key).ok_or_else(|| {
                                NativeCallControl::from(
                                    "native unpack string key has no byte storage",
                                )
                            })?;
                        Some(String::from_utf8_lossy(&bytes).into_owned())
                    }
                    _ => {
                        return Err(NativeCallControl::from(
                            "Keys must be of type int|string during argument unpacking",
                        ));
                    }
                };
                supplied.push(NativeSuppliedCallArgument {
                    name,
                    value: entry.value,
                });
            }
            continue;
        }

        if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline {
            return Err(NativeCallControl::BaselineRequired);
        }
        // Baseline-only compatibility: new arrays cross calls through direct
        // entries and exact native callbacks cannot enter this conversion.
        let Value::Array(array) = context.decode(*value)? else {
            return Err(NativeCallControl::from(
                "Only arrays and Traversables can be unpacked",
            ));
        };
        for (key, value) in array.iter() {
            let name = match key {
                php_runtime::api::ArrayKey::Int(_) => None,
                php_runtime::api::ArrayKey::String(name) => Some(name.to_string_lossy()),
            };
            let encoded = context.encode(value.clone())?;
            compatibility_owners.push(encoded);
            supplied.push(NativeSuppliedCallArgument {
                name,
                value: encoded,
            });
        }
    }
    Ok(supplied)
}

fn bind_native_by_value_parameter(
    context: &mut NativeRequestColdState<'_>,
    argument: i64,
    type_: Option<&php_ir::IrReturnType>,
    strict: bool,
    target_name: &str,
    parameter_index: usize,
    parameter_name: &str,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    let Some(type_) = type_ else {
        if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline {
            return context
                .duplicate_authoritative_dereferenced_native_value(argument)?
                .ok_or(NativeCallControl::BaselineRequired);
        }
        return Ok(context.duplicate_dereferenced_native_value(argument)?);
    };
    let argument = if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline {
        context
            .duplicate_authoritative_dereferenced_native_value(argument)?
            .ok_or(NativeCallControl::BaselineRequired)?
    } else {
        context.duplicate_dereferenced_native_value(argument)?
    };
    let coerced = match context.coerce_native_call_argument_encoded(argument, type_, strict) {
        Ok(value) => value,
        Err(error) => {
            context.release(argument)?;
            return Err(error.into());
        }
    };
    if let Some(value) = coerced {
        context.release(argument)?;
        if context.native_encoded_matches_ir_type(value, type_) == Some(true) {
            return Ok(value);
        }
        let actual = context.native_encoded_type_name(value);
        context.release(value)?;
        return Err(NativeCallControl::throw(
            "TypeError",
            format!(
                "{target_name}(): Argument #{} (${parameter_name}) must be of type {}, {actual} given",
                parameter_index + 1,
                native_ir_type_name(type_),
            ),
        ));
    }

    if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline {
        context.release(argument)?;
        return Err(NativeCallControl::BaselineRequired);
    }
    // Baseline-only compatibility. Exact native callbacks never decode an
    // argument or reconstruct it through the Rust value plane.
    let mut value = match context.decode(argument) {
        Ok(value) => value,
        Err(error) => {
            context.release(argument)?;
            return Err(error.into());
        }
    };
    context.release(argument)?;
    value = native_coerce_call_argument(value, type_, strict);
    if !(native_value_matches_ir_type_in_context(context, &value, type_)
        || matches!(type_, php_ir::IrReturnType::Callable)
            && native_value_is_callable(context, &value))
    {
        return Err(NativeCallControl::throw(
            "TypeError",
            format!(
                "{target_name}(): Argument #{} (${parameter_name}) must be of type {}, {} given",
                parameter_index + 1,
                native_ir_type_name(type_),
                native_value_type_name(&value)
            ),
        ));
    }
    Ok(context.encode_baseline_call_value(value)?)
}

fn bind_native_by_reference_parameter(
    context: &mut NativeRequestColdState<'_>,
    argument: i64,
    type_: Option<&php_ir::IrReturnType>,
    strict: bool,
    target_name: &str,
    parameter_index: usize,
    parameter_name: &str,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    if context.php_handle_is_reference(argument) != Some(true) {
        return Err(NativeCallControl::throw(
            "Error",
            format!(
                "{target_name}(): Argument #{} (${parameter_name}) could not be passed by reference",
                parameter_index + 1,
            ),
        ));
    }
    if let Some(payload) = context.direct_reference_payload(argument) {
        let uninitialized = context.php_handle_is_uninitialized(payload);
        let payload = if uninitialized {
            php_jit::jit_encode_constant(u32::MAX)
        } else {
            payload
        };
        if let Some(type_) = type_ {
            let Some(replacement) =
                context.coerce_native_call_argument_encoded(payload, type_, strict)?
            else {
                return bind_materialized_reference_parameter(
                    context,
                    argument,
                    type_,
                    strict,
                    target_name,
                    parameter_index,
                    parameter_name,
                    builtin_policy,
                );
            };
            if context.native_encoded_matches_ir_type(replacement, type_) != Some(true) {
                let actual = context.native_encoded_type_name(replacement);
                context.release(replacement)?;
                return Err(NativeCallControl::throw(
                    "TypeError",
                    format!(
                        "{target_name}(): Argument #{} (${parameter_name}) must be of type {}, {actual} given",
                        parameter_index + 1,
                        native_ir_type_name(type_),
                    ),
                ));
            }
            if !context.replace_direct_reference_payload_owned(argument, replacement)? {
                context.release(replacement)?;
                return Err("direct call reference payload disappeared during binding".into());
            }
        } else if uninitialized
            && !context.replace_direct_reference_payload_owned(
                argument,
                php_jit::jit_encode_constant(u32::MAX),
            )?
        {
            return Err("direct call reference payload disappeared during binding".into());
        }
        context.retain(argument)?;
        return Ok(argument);
    }
    let Some(type_) = type_ else {
        if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline {
            return Err(NativeCallControl::BaselineRequired);
        }
        // A materialized/compatibility reference is still one stable arena
        // identity; the callee receives its own native owner.
        if let Value::Reference(reference) = context.decode(argument)?
            && matches!(reference.get(), Value::Uninitialized)
        {
            reference.set(Value::Null);
        }
        return Ok(context.duplicate_baseline_call_argument(argument)?);
    };
    bind_materialized_reference_parameter(
        context,
        argument,
        type_,
        strict,
        target_name,
        parameter_index,
        parameter_name,
        builtin_policy,
    )
}

fn bind_materialized_reference_parameter(
    context: &mut NativeRequestColdState<'_>,
    argument: i64,
    type_: &php_ir::IrReturnType,
    strict: bool,
    target_name: &str,
    parameter_index: usize,
    parameter_name: &str,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline {
        return Err(NativeCallControl::BaselineRequired);
    }
    let Value::Reference(reference) = context.decode(argument)? else {
        return Err(NativeCallControl::throw(
            "Error",
            format!(
                "{target_name}(): Argument #{} (${parameter_name}) could not be passed by reference",
                parameter_index + 1,
            ),
        ));
    };
    if matches!(reference.get(), Value::Uninitialized) {
        reference.set(Value::Null);
    }
    let value = native_coerce_call_argument(reference.get(), type_, strict);
    if !(native_value_matches_ir_type_in_context(context, &value, type_)
        || matches!(type_, php_ir::IrReturnType::Callable)
            && native_value_is_callable(context, &value))
    {
        return Err(NativeCallControl::throw(
            "TypeError",
            format!(
                "{target_name}(): Argument #{} (${parameter_name}) must be of type {}, {} given",
                parameter_index + 1,
                native_ir_type_name(type_),
                native_value_type_name(&value)
            ),
        ));
    }
    reference.set(value);
    Ok(context.duplicate_baseline_call_argument(argument)?)
}

pub(super) fn native_catch_matches(
    context: &mut NativeRequestColdState<'_>,
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
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
) -> NativeCallResult {
    invoke_native_function_with_metadata(context, function, arguments, None)
}

pub(super) fn invoke_native_function_with_metadata(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> NativeCallResult {
    invoke_native_function_with_metadata_strict(
        context,
        function,
        arguments,
        metadata,
        context.unit.strict_types,
    )
}

pub(super) fn invoke_native_function_with_metadata_strict(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    strict: bool,
) -> NativeCallResult {
    invoke_native_function_with_metadata_strict_at_tier(
        context,
        function,
        arguments,
        metadata,
        strict,
        false,
        NativeCallableBuiltinPolicy::ExecuteBaseline,
    )
}

pub(super) fn invoke_native_resolved_function_with_metadata_strict(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    strict: bool,
) -> NativeCallResult {
    invoke_native_function_with_metadata_strict_at_tier(
        context,
        function,
        arguments,
        metadata,
        strict,
        true,
        NativeCallableBuiltinPolicy::ExecuteBaseline,
    )
}

pub(super) fn invoke_native_function_with_metadata_strict_at_tier(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    strict: bool,
    baseline_continuation: bool,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    bind_native_function_with_metadata_strict_at_tier(
        context,
        function,
        arguments,
        metadata,
        strict,
        builtin_policy,
        |context, function, bound, visible_arguments, target_metadata| {
            invoke_native_with_owned_bound_arguments(
                context,
                function,
                &bound,
                Some(visible_arguments),
                Some(target_metadata),
                baseline_continuation,
            )
        },
    )
}

fn bind_native_function_with_metadata_strict_at_tier<R>(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    strict: bool,
    builtin_policy: NativeCallableBuiltinPolicy,
    finish: impl FnOnce(
        &mut NativeRequestColdState<'_>,
        php_ir::FunctionId,
        smallvec::SmallVec<[i64; 8]>,
        request_state::NativeTraceArguments,
        NativeFunctionMetadataPtr,
    ) -> Result<R, NativeCallControl>,
) -> Result<R, NativeCallControl> {
    let target_metadata = NativeFunctionMetadataPtr::from_compiled(&context.compiled, function)
        .ok_or_else(|| {
            format!(
                "native function {} has no prepared metadata",
                function.raw()
            )
        })?;
    let target_name = target_metadata.name.as_ref();
    let target_params = target_metadata.params.as_ref();
    let leading = target_metadata.capture_count
        + usize::from(target_metadata.instance_method)
        + usize::from(target_metadata.implicit_closure_this);
    if arguments.len() < leading {
        return Err(format!(
            "{}() is missing its native receiver/capture arguments",
            target_name
        )
        .into());
    }
    let raw_supplied = &arguments[leading..];
    if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline
        && (!authoritative_native_call_arguments_are_admitted(context, &arguments[..leading], None)
            || !authoritative_native_call_arguments_are_admitted(context, raw_supplied, metadata))
    {
        return Err(NativeCallControl::BaselineRequired);
    }
    let variadic_index = target_params
        .iter()
        .position(|parameter| parameter.variadic);
    let fixed_count = variadic_index.unwrap_or(target_params.len());
    let mut bound = smallvec::SmallVec::<[i64; 8]>::new();
    let mut compatibility_owners = Vec::new();
    let binding = (|| -> Result<request_state::NativeTraceArguments, NativeCallControl> {
        // The callee owns every frame slot. Give receiver/captures independent
        // native owners before binding visible parameters.
        for argument in &arguments[..leading] {
            let duplicated = context.duplicate_authoritative_native_value(*argument)?;
            let value = match duplicated {
                Some(value) => value,
                None if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline => {
                    return Err(NativeCallControl::BaselineRequired);
                }
                None => context.duplicate_baseline_call_argument(*argument)?,
            };
            bound.push(value);
        }

        let supplied = expand_native_user_call_arguments(
            context,
            raw_supplied,
            metadata,
            &mut compatibility_owners,
            builtin_policy,
        )?;
        let mut assigned = vec![None::<NativeSuppliedCallArgument>; fixed_count];
        let mut variadic = Vec::<NativeSuppliedCallArgument>::new();
        let mut extra = Vec::<NativeSuppliedCallArgument>::new();
        let mut positional = 0usize;
        let mut saw_named = false;
        let mut variadic_names = std::collections::BTreeSet::new();

        for argument in supplied {
            if let Some(name) = argument.name.clone() {
                saw_named = true;
                if let Some(index) = target_params[..fixed_count]
                    .iter()
                    .position(|parameter| parameter.name.eq_ignore_ascii_case(&name))
                {
                    if assigned[index].replace(argument).is_some() {
                        return Err(NativeCallControl::throw(
                            "Error",
                            format!("Named parameter ${name} overwrites previous argument"),
                        ));
                    }
                } else if variadic_index.is_some() {
                    if !variadic_names.insert(name.to_ascii_lowercase()) {
                        return Err(NativeCallControl::throw(
                            "Error",
                            format!("Named parameter ${name} overwrites previous argument"),
                        ));
                    }
                    variadic.push(argument);
                } else {
                    return Err(NativeCallControl::throw(
                        "Error",
                        format!("Unknown named parameter ${name}"),
                    ));
                }
                continue;
            }
            if saw_named {
                return Err(NativeCallControl::throw(
                    "Error",
                    "Cannot use positional argument after named argument",
                ));
            }
            while positional < fixed_count && assigned[positional].is_some() {
                positional += 1;
            }
            if positional < fixed_count {
                assigned[positional] = Some(argument);
                positional += 1;
            } else if variadic_index.is_some() {
                variadic.push(argument);
            } else {
                extra.push(argument);
            }
        }

        // Introspection observes supplied arguments in bound parameter order,
        // followed by accepted surplus/variadic arguments. Direct handles are
        // borrowed from the caller; compatibility-unpack owners remain live
        // until the synchronous callee returns below.
        let visible_arguments = assigned
            .iter()
            .flatten()
            .chain(variadic.iter().filter(|argument| argument.name.is_none()))
            .chain(&extra)
            .map(|argument| argument.value)
            .collect::<request_state::NativeTraceArguments>();

        for (index, parameter) in target_params.iter().enumerate() {
            if parameter.variadic {
                let mut entries = Vec::with_capacity(variadic.len());
                let mut positional_key = 0i64;
                for argument in &variadic {
                    let value = if parameter.by_ref {
                        bind_native_by_reference_parameter(
                            context,
                            argument.value,
                            parameter.type_.as_ref(),
                            strict,
                            target_name,
                            index,
                            &parameter.name,
                            builtin_policy,
                        )?
                    } else {
                        bind_native_by_value_parameter(
                            context,
                            argument.value,
                            parameter.type_.as_ref(),
                            strict,
                            target_name,
                            index,
                            &parameter.name,
                            builtin_policy,
                        )?
                    };
                    let key = if let Some(name) = argument.name.as_ref() {
                        match context.encode_direct_string_bytes(name.as_bytes()) {
                            Ok(key) => key,
                            Err(error) => {
                                context.release(value)?;
                                return Err(error.into());
                            }
                        }
                    } else {
                        let key = positional_key;
                        positional_key = positional_key.checked_add(1).ok_or_else(|| {
                            php_runtime::api::PHP_ARRAY_APPEND_OVERFLOW_MESSAGE.to_owned()
                        })?;
                        key
                    };
                    entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
                }
                bound.push(context.publish_owned_direct_array_entries(entries)?);
                continue;
            }

            if let Some(argument) = assigned[index].as_ref() {
                let value = if parameter.by_ref {
                    bind_native_by_reference_parameter(
                        context,
                        argument.value,
                        parameter.type_.as_ref(),
                        strict,
                        target_name,
                        index,
                        &parameter.name,
                        builtin_policy,
                    )?
                } else {
                    bind_native_by_value_parameter(
                        context,
                        argument.value,
                        parameter.type_.as_ref(),
                        strict,
                        target_name,
                        index,
                        &parameter.name,
                        builtin_policy,
                    )?
                };
                bound.push(value);
            } else if let Some(default) = &parameter.default {
                if parameter.by_ref {
                    let value = native_runtime_constant_value(context, default)?;
                    bound.push(context.encode_native_reference_owner(
                        php_runtime::api::ReferenceCell::new(value),
                    )?);
                } else {
                    bound.push(context.encode_native_ir_constant_owned(default)?);
                }
            } else {
                return Err(NativeCallControl::throw(
                    "ArgumentCountError",
                    format!("Too few arguments to function {target_name}()"),
                ));
            }
        }

        Ok(visible_arguments)
    })();

    let visible_arguments = match binding {
        Ok(arguments) => arguments,
        Err(error) => {
            for value in compatibility_owners {
                let _ = context.release(value);
            }
            for value in bound {
                let _ = context.release(value);
            }
            return Err(error.into());
        }
    };
    let result = finish(context, function, bound, visible_arguments, target_metadata);
    let mut release_error = None;
    for value in compatibility_owners {
        if let Err(error) = context.release(value) {
            release_error.get_or_insert(error);
        }
    }
    match (result, release_error) {
        (Err(error), _) => Err(error),
        (Ok(_), Some(error)) => Err(error.into()),
        (Ok(value), None) => Ok(value),
    }
}

pub(super) fn create_native_generator_with_metadata_strict(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    strict: bool,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    let target = NativeExecutionTarget {
        unit: context.current_dynamic_unit,
        function,
        called_class: context.called_classes.last().cloned(),
        scope_class: context
            .lexical_scope_classes
            .last()
            .map(|scope| Arc::from(scope.as_str())),
    };
    bind_native_function_with_metadata_strict_at_tier(
        context,
        function,
        arguments,
        metadata,
        strict,
        builtin_policy,
        move |context, _, bound, _, _| {
            context
                .publish_native_generator_owned(target, bound.into_vec())
                .map_err(NativeCallControl::from)
        },
    )
}

fn invoke_native_with_owned_bound_arguments(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    bound: &[i64],
    trace_arguments: Option<request_state::NativeTraceArguments>,
    metadata: Option<NativeFunctionMetadataPtr>,
    baseline_continuation: bool,
) -> NativeCallResult {
    if native_function_is_generator(context, function) {
        return context
            .publish_native_generator_owned(
                NativeExecutionTarget {
                    unit: context.current_dynamic_unit,
                    function,
                    called_class: context.called_classes.last().cloned(),
                    scope_class: context
                        .lexical_scope_classes
                        .last()
                        .map(|scope| Arc::from(scope.as_str())),
                },
                bound.to_vec(),
            )
            .map_err(NativeCallControl::from);
    }
    // Bound handles are transferred into the callee frame. Native epilogues
    // release parameter locals on every return/unwind edge; releasing them a
    // second time here recycled live array/object slots during callbacks.
    invoke_native_method_with_prepared_trace_arguments(
        context,
        function,
        bound,
        trace_arguments,
        metadata,
        baseline_continuation,
    )
}

pub(super) fn bind_native_property_reference_arguments(
    context: &mut NativeRequestColdState<'_>,
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
            || context.php_handle_is_reference(*encoded) == Some(true)
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
        if let Some(reference) = context
            .bind_native_declared_property_reference(argument.property_receiver, &target.property)?
        {
            argument.value.payload = reference as u64;
            *encoded = reference;
            continue;
        }

        // Dynamic/magic properties have no admitted numeric slot and are
        // therefore already at the explicit cold call boundary.
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
        let reference = context.encode_native_reference_owner(reference)?;
        argument.value.payload = reference as u64;
        *encoded = reference;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativeCallableBuiltinPolicy {
    ExecuteBaseline,
    RequireBaseline,
}

fn external_function_is_generator(
    context: &NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
) -> bool {
    context
        .dynamic_units
        .get(target.unit)
        .and_then(|unit| unit.compiled.unit().functions.get(target.function.index()))
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
}

fn method_callable_requires_baseline(
    context: &NativeRequestColdState<'_>,
    class: &str,
    method: &str,
) -> bool {
    if native_method_in_hierarchy(context, class, method).is_some() {
        return false;
    }
    if native_external_method(context, class, method).is_some() {
        return false;
    }
    true
}

fn named_callable_requires_baseline(context: &NativeRequestColdState<'_>, name: &str) -> bool {
    if let Some((class, method)) = name.split_once("::") {
        return method_callable_requires_baseline(context, class, method);
    }
    if context.function_id(name).is_some() {
        return false;
    }
    if context.external_function(name).is_some() {
        return false;
    }
    // Fixed builtins execute in the baseline continuation when reached as a
    // callback. The exact callback ABI never enters the generic dispatcher.
    true
}

pub(super) fn exact_native_callable_requires_baseline(
    context: &mut NativeRequestColdState<'_>,
    callee: i64,
) -> bool {
    let direct_callee = context.dereference_direct_encoding(callee);
    match context.native_encoded_value_kind(direct_callee) {
        Some(NativeEncodedValueKind::String) => context
            .native_string_name_bytes(direct_callee)
            .map(|bytes| {
                named_callable_requires_baseline(context, &String::from_utf8_lossy(&bytes))
            })
            .unwrap_or(true),
        Some(NativeEncodedValueKind::Object) => context
            .native_query_object(direct_callee)
            .is_none_or(|object| {
                method_callable_requires_baseline(context, &object.class_name(), "__invoke")
            }),
        Some(NativeEncodedValueKind::Array) => {
            let Some(entries) = context.direct_array_entries_for(direct_callee) else {
                return true;
            };
            if entries.len() != 2 {
                return true;
            }
            let mut target = None;
            let mut method = None;
            for entry in entries {
                match context.native_encoded_int(entry.key) {
                    Some(0) => target = Some(context.dereference_direct_encoding(entry.value)),
                    Some(1) => method = Some(context.dereference_direct_encoding(entry.value)),
                    _ => return true,
                }
            }
            let Some(method) = method.and_then(|method| context.native_string_name_bytes(method))
            else {
                return true;
            };
            let method = String::from_utf8_lossy(&method);
            let Some(target) = target else {
                return true;
            };
            if let Some(object) = context.native_query_object(target) {
                method_callable_requires_baseline(context, &object.class_name(), &method)
            } else if let Some(class) = context.native_string_name_bytes(target) {
                method_callable_requires_baseline(
                    context,
                    &String::from_utf8_lossy(&class),
                    &method,
                )
            } else {
                true
            }
        }
        Some(NativeEncodedValueKind::Callable) => {
            match context.prepared_callable_dispatch(direct_callee) {
                Some(NativePreparedCallableDispatch::Closure) => {
                    context.prepared_closure_payload(direct_callee).is_none()
                }
                Some(NativePreparedCallableDispatch::Named(name)) => {
                    named_callable_requires_baseline(context, &name)
                }
                Some(NativePreparedCallableDispatch::BoundMethod { target, method }) => {
                    let class = match target {
                        php_runtime::api::CallableMethodTarget::Object(object) => {
                            object.class_name()
                        }
                        php_runtime::api::CallableMethodTarget::Class(class) => class,
                    };
                    method_callable_requires_baseline(context, &class, &method)
                }
                Some(NativePreparedCallableDispatch::Invalid(_)) | None => true,
            }
        }
        _ => true,
    }
}

pub(super) fn invoke_native_named_callable(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
    instruction: &php_ir::Instruction,
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    if let Some(function) = context.function_id(name) {
        if native_function_is_generator(context, function) {
            return create_native_generator_with_metadata_strict(
                context,
                function,
                arguments,
                metadata,
                context.unit.strict_types_for_span(instruction.span),
                builtin_policy,
            );
        }
        return invoke_native_function_with_metadata_strict_at_tier(
            context,
            function,
            arguments,
            metadata,
            context.unit.strict_types_for_span(instruction.span),
            false,
            builtin_policy,
        );
    }
    if let Some(function) = context.external_function(name) {
        if external_function_is_generator(context, function) {
            return create_native_external_generator_with_metadata_policy(
                context,
                function,
                arguments,
                metadata,
                None,
                context.unit.strict_types_for_span(instruction.span),
                builtin_policy,
            );
        }
        return invoke_native_external_function_with_metadata_policy(
            context,
            function,
            arguments,
            metadata,
            None,
            context.unit.strict_types_for_span(instruction.span),
            builtin_policy,
        );
    }
    if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline {
        return Err(NativeCallControl::BaselineRequired);
    }
    let builtin_name = if php_std::arginfo::function_metadata_indexed(name).is_some() {
        name
    } else {
        name.rsplit('\\').next().unwrap_or(name)
    };
    let expanded = bind_native_builtin_arguments(context, builtin_name, arguments, metadata)?;
    execute_baseline_native_builtin_control(
        context,
        builtin_name,
        &expanded,
        instruction,
        None,
        None,
    )
}

pub(super) fn expand_native_unpack_arguments(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> Result<Vec<i64>, String> {
    let Some(metadata) = metadata else {
        return Ok(arguments.to_vec());
    };
    let mut expanded = Vec::new();
    for (argument, value) in metadata.iter().zip(arguments) {
        if argument.unpack {
            if let Some(entries) = context.direct_array_entries_for(*value).map(<[_]>::to_vec) {
                expanded.extend(entries.into_iter().map(|entry| entry.value));
                continue;
            }
            // Traversable and compatibility arrays are explicit cold shapes.
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

fn native_builtin_default_encoded(
    context: &mut NativeRequestColdState<'_>,
    expression: &str,
) -> Result<i64, String> {
    let expression = expression.trim();
    match expression {
        "null" => Ok(php_jit::jit_encode_constant(u32::MAX)),
        "true" => Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)),
        "false" => Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE)),
        "PHP_INT_MAX" => Ok(i64::MAX),
        _ if expression.starts_with('"') && expression.ends_with('"') => {
            let inner = &expression[1..expression.len().saturating_sub(1)];
            let mut bytes = Vec::with_capacity(inner.len());
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
            context.encode_direct_string_bytes(&bytes)
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
            Ok(value)
        }
        _ => match expression.parse::<i64>() {
            Ok(value) => Ok(value),
            Err(_) => context
                .lookup_constant(expression)
                .and_then(|value| context.encode(value)),
        },
    }
}

pub(super) fn native_builtin_default_value(
    context: &NativeRequestColdState<'_>,
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

pub(super) fn native_builtin_arguments_require_binding(
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> bool {
    metadata.is_some_and(|arguments| {
        arguments
            .iter()
            .any(|argument| argument.name.is_some() || argument.unpack)
    })
}

pub(super) fn bind_native_builtin_arguments<'a>(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &'a [i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
) -> Result<std::borrow::Cow<'a, [i64]>, String> {
    if native_builtins::native_builtin_is_unavailable_target_function(name)
        || (name.eq_ignore_ascii_case("print")
            && metadata
                .is_some_and(|arguments| arguments.iter().any(|argument| argument.name.is_some())))
    {
        return Err(format!(
            "E_PHP_THROW:Error:Call to undefined function {name}()"
        ));
    }
    if !native_builtin_arguments_require_binding(metadata) {
        return Ok(std::borrow::Cow::Borrowed(arguments));
    }
    let Some(call_metadata) = metadata else {
        return Ok(std::borrow::Cow::Borrowed(arguments));
    };
    if !call_metadata.iter().any(|argument| argument.name.is_some()) {
        if !call_metadata.iter().any(|argument| argument.unpack) {
            return Ok(std::borrow::Cow::Borrowed(arguments));
        }
        return expand_native_unpack_arguments(context, arguments, metadata)
            .map(std::borrow::Cow::Owned);
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
            bound.push(native_builtin_default_encoded(context, default)?);
        } else {
            return Err(format!("Missing required argument ${}", parameter.name));
        }
    }
    Ok(std::borrow::Cow::Owned(bound))
}

pub(super) fn invoke_baseline_native_bound_method(
    context: &mut NativeRequestColdState<'_>,
    target: &php_runtime::api::CallableMethodTarget,
    method: &str,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    strict: bool,
    caller_function: Option<u32>,
) -> NativeCallResult {
    invoke_native_bound_method(
        context,
        target,
        method,
        arguments,
        metadata,
        strict,
        caller_function,
        NativeCallableBuiltinPolicy::ExecuteBaseline,
    )
}

fn invoke_native_bound_method(
    context: &mut NativeRequestColdState<'_>,
    target: &php_runtime::api::CallableMethodTarget,
    method: &str,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    strict: bool,
    caller_function: Option<u32>,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    let (class_name, receiver) = match target {
        php_runtime::api::CallableMethodTarget::Object(object) => (
            object.class_name(),
            Some(context.encode_native_object_owner(object.clone())?),
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
            return Err(NativeCallControl::throw("Error", error));
        }
        if caller_function.is_none() && (entry.flags.is_private || entry.flags.is_protected) {
            return Err(NativeCallControl::throw(
                "Error",
                format!(
                    "Call to {} method {class_name}::{method}() from global scope",
                    if entry.flags.is_private {
                        "private"
                    } else {
                        "protected"
                    },
                ),
            ));
        }
        let function = entry.function;
        let execution_target = NativeExecutionTarget {
            unit: context.current_dynamic_unit,
            function,
            called_class: if entry.flags.is_static {
                Some(Arc::from(class_name.as_str()))
            } else {
                context.called_classes.last().cloned()
            },
            scope_class: context
                .lexical_scope_classes
                .last()
                .map(|scope| Arc::from(scope.as_str())),
        };
        return context.run_in_native_execution_target(&execution_target, |context| {
            if native_function_is_generator(context, function) {
                return create_native_generator_with_metadata_strict(
                    context,
                    function,
                    &call_arguments,
                    metadata,
                    strict,
                    builtin_policy,
                );
            }
            invoke_native_function_with_metadata_strict_at_tier(
                context,
                function,
                &call_arguments,
                metadata,
                strict,
                false,
                builtin_policy,
            )
        });
    }
    if let Some((function, _)) = native_external_method(context, &class_name, method) {
        if external_function_is_generator(context, function) {
            return create_native_external_generator_with_metadata_policy(
                context,
                function,
                &call_arguments,
                metadata,
                Some(class_name),
                strict,
                builtin_policy,
            );
        }
        return invoke_native_external_function_with_metadata_policy(
            context,
            function,
            &call_arguments,
            metadata,
            Some(class_name),
            strict,
            builtin_policy,
        );
    }
    Err(NativeCallControl::RuntimeError(format!(
        "Call to undefined method {class_name}::{method}()"
    )))
}

fn invoke_native_closure_payload(
    context: &mut NativeRequestColdState<'_>,
    closure: &php_runtime::api::ClosurePayload,
    prepared_implicit_this: Option<i64>,
    prepared_captures: Option<&[i64]>,
    arguments: &[i64],
    instruction: &php_ir::Instruction,
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline && prepared_captures.is_none()
    {
        return Err(NativeCallControl::BaselineRequired);
    }
    let function = php_ir::FunctionId::new(closure.function);
    let owner_unit = closure.context.owner_unit;
    let has_implicit_this = owner_unit
        .and_then(|unit| context.dynamic_units.get(unit))
        .map(|package| package.compiled.unit())
        .unwrap_or(&*context.unit)
        .functions
        .get(function.index())
        .is_some_and(native_function_has_implicit_closure_this);
    let mut closure_arguments = Vec::with_capacity(
        usize::from(has_implicit_this) + closure.captures.len() + arguments.len(),
    );
    let mut temporary_owners = Vec::new();
    if has_implicit_this {
        let encoded = if let Some(encoded) = prepared_implicit_this {
            encoded
        } else {
            let encoded = context.encode(
                closure
                    .bound_this
                    .as_ref()
                    .map_or(Value::Null, |object| Value::Object(object.clone())),
            )?;
            temporary_owners.push(encoded);
            encoded
        };
        closure_arguments.push(encoded);
    }
    if let Some(captures) = prepared_captures {
        closure_arguments.extend_from_slice(captures);
    } else {
        // Compatibility for a closure reached through a baseline
        // ReferenceCell. Direct native closures never enter this branch.
        for capture in &closure.captures {
            let encoded = if capture.name.eq_ignore_ascii_case("this")
                && let Some(object) = &closure.bound_this
            {
                context.encode_native_object_owner(object.clone())?
            } else if let Some(reference) = capture.reference() {
                context.encode_native_reference_owner(reference)?
            } else {
                context
                    .encode_baseline_call_value(capture.value().cloned().unwrap_or(Value::Null))?
            };
            closure_arguments.push(encoded);
            temporary_owners.push(encoded);
        }
    }
    closure_arguments.extend_from_slice(arguments);

    let pushed_scope = closure.context.scope_class.is_some();
    if let Some(scope_class) = &closure.context.scope_class {
        context.lexical_scope_classes.push(scope_class.to_string());
    }
    let result = (|| -> NativeCallResult {
        let generator = owner_unit.map_or_else(
            || native_function_is_generator(context, function),
            |unit| {
                external_function_is_generator(context, NativeDynamicFunction { unit, function })
            },
        );
        if generator {
            if let Some(unit) = owner_unit {
                return create_native_external_generator_with_metadata_policy(
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
                    builtin_policy,
                );
            }
            let execution_target = NativeExecutionTarget {
                unit: context.current_dynamic_unit,
                function,
                called_class: closure.context.called_class.clone(),
                scope_class: closure.context.scope_class.clone(),
            };
            return context.run_in_native_execution_target(&execution_target, |context| {
                create_native_generator_with_metadata_strict(
                    context,
                    function,
                    &closure_arguments,
                    metadata,
                    context.unit.strict_types_for_span(instruction.span),
                    builtin_policy,
                )
            });
        }
        if let Some(unit) = owner_unit {
            return invoke_native_external_function_with_metadata_policy(
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
                builtin_policy,
            );
        }
        let execution_target = NativeExecutionTarget {
            unit: context.current_dynamic_unit,
            function,
            called_class: closure.context.called_class.clone(),
            scope_class: closure.context.scope_class.clone(),
        };
        context.run_in_native_execution_target(&execution_target, |context| {
            invoke_native_function_with_metadata_strict_at_tier(
                context,
                function,
                &closure_arguments,
                metadata,
                context.unit.strict_types_for_span(instruction.span),
                false,
                builtin_policy,
            )
        })
    })();
    if pushed_scope {
        context.lexical_scope_classes.pop();
    }
    let mut release_error = None;
    for owner in temporary_owners {
        if let Err(error) = context.release_if_live(owner) {
            release_error.get_or_insert(error);
        }
    }
    match (result, release_error) {
        (Err(error), _) => Err(error),
        (Ok(_), Some(error)) => Err(error.into()),
        (Ok(value), None) => Ok(value),
    }
}

pub(super) fn execute_native_dynamic_callable(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    encoded: &[i64],
    caller_function: Option<u32>,
    prepared_arguments: Option<&[php_ir::instruction::IrCallArg]>,
    prepared_builtin_source: bool,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> Option<NativeCallResult> {
    if !prepared_builtin_source
        && !matches!(
            instruction.kind,
            php_ir::InstructionKind::CallCallable { .. }
                | php_ir::InstructionKind::CallClosure { .. }
                | php_ir::InstructionKind::Pipe { .. }
        )
    {
        return None;
    }
    let Some((callee, arguments)) = encoded.split_first() else {
        return Some(Err("callable operand is missing".into()));
    };
    let metadata = prepared_arguments.or_else(|| match &instruction.kind {
        php_ir::InstructionKind::CallCallable { args, .. }
        | php_ir::InstructionKind::CallClosure { args, .. } => Some(args.as_slice()),
        _ => None,
    });
    let direct_callee = context.dereference_direct_encoding(*callee);
    if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline
        && exact_native_callable_requires_baseline(context, *callee)
    {
        return Some(Err(NativeCallControl::BaselineRequired));
    }
    match context.native_encoded_value_kind(direct_callee) {
        Some(NativeEncodedValueKind::String) => {
            let Some(bytes) = context.native_string_name_bytes(direct_callee) else {
                return Some(Err("native callable string has no byte storage".into()));
            };
            let name = String::from_utf8_lossy(&bytes);
            let result = if let Some((class, method)) = name.split_once("::") {
                invoke_native_bound_method(
                    context,
                    &php_runtime::api::CallableMethodTarget::Class(class.to_owned()),
                    method,
                    arguments,
                    metadata,
                    context.unit.strict_types_for_span(instruction.span),
                    caller_function,
                    builtin_policy,
                )
            } else {
                invoke_native_named_callable(
                    context,
                    &name,
                    arguments,
                    instruction,
                    metadata,
                    builtin_policy,
                )
            };
            return Some(result);
        }
        Some(NativeEncodedValueKind::Object) => {
            let Some(object) = context.native_query_object(direct_callee) else {
                return Some(Err("native callable object owner is unavailable".into()));
            };
            return Some(invoke_native_bound_method(
                context,
                &php_runtime::api::CallableMethodTarget::Object(object),
                "__invoke",
                arguments,
                metadata,
                context.unit.strict_types_for_span(instruction.span),
                caller_function,
                builtin_policy,
            ));
        }
        Some(NativeEncodedValueKind::Array) => {
            if let Some(entries) = context
                .direct_array_entries_for(direct_callee)
                .map(<[_]>::to_vec)
            {
                if entries.len() != 2 {
                    return Some(Err("callable array must contain exactly two members".into()));
                }
                let mut target = None;
                let mut method = None;
                for entry in entries {
                    match context.native_encoded_int(entry.key) {
                        Some(0) => target = Some(entry.value),
                        Some(1) => method = Some(entry.value),
                        _ => {}
                    }
                }
                let Some(target) = target.map(|target| context.dereference_direct_encoding(target))
                else {
                    return Some(Err("callable array target is missing".into()));
                };
                let Some(method) = method
                    .map(|method| context.dereference_direct_encoding(method))
                    .and_then(|method| context.native_string_name_bytes(method))
                    .map(|method| String::from_utf8_lossy(&method).into_owned())
                else {
                    return Some(Err("callable array method must be a string".into()));
                };
                let target = if let Some(object) = context.native_query_object(target) {
                    php_runtime::api::CallableMethodTarget::Object(object)
                } else if let Some(class) = context.native_string_name_bytes(target) {
                    php_runtime::api::CallableMethodTarget::Class(
                        String::from_utf8_lossy(&class).into_owned(),
                    )
                } else {
                    return Some(Err(format!(
                        "callable array target must be object or class-string, {} given",
                        context.native_encoded_type_name(target)
                    )
                    .into()));
                };
                return Some(invoke_native_bound_method(
                    context,
                    &target,
                    &method,
                    arguments,
                    metadata,
                    context.unit.strict_types_for_span(instruction.span),
                    caller_function,
                    builtin_policy,
                ));
            }
        }
        _ => {}
    }
    if let Some((closure, implicit_this, captures)) =
        context.prepared_closure_invocation(direct_callee)
    {
        return Some(invoke_native_closure_payload(
            context,
            &closure,
            implicit_this,
            Some(&captures),
            arguments,
            instruction,
            metadata,
            builtin_policy,
        ));
    }
    if let Some(callable) = context.prepared_callable_dispatch(direct_callee) {
        let result = match callable {
            NativePreparedCallableDispatch::Named(name) => invoke_native_named_callable(
                context,
                &name,
                arguments,
                instruction,
                metadata,
                builtin_policy,
            ),
            NativePreparedCallableDispatch::BoundMethod { target, method } => {
                invoke_native_bound_method(
                    context,
                    &target,
                    &method,
                    arguments,
                    metadata,
                    context.unit.strict_types_for_span(instruction.span),
                    caller_function,
                    builtin_policy,
                )
            }
            NativePreparedCallableDispatch::Invalid(target) => {
                Err(format!("{target} is not callable").into())
            }
            NativePreparedCallableDispatch::Closure => {
                Err("direct native closure record is invalid".into())
            }
        };
        return Some(result);
    }
    if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline {
        // Boxed compatibility callables are baseline carriers. Exact native
        // callback handlers never decode them.
        return Some(Err(NativeCallControl::BaselineRequired));
    }
    let callee = match context.decode(*callee) {
        Ok(value) => dereference_native_callable_value(value),
        Err(error) => return Some(Err(error.into())),
    };
    let result = (|| -> NativeCallResult {
        match callee {
            Value::Callable(callable) => match callable.as_ref() {
                php_runtime::api::CallableValue::UserFunction { name }
                | php_runtime::api::CallableValue::InternalBuiltin { name } => {
                    invoke_native_named_callable(
                        context,
                        name,
                        arguments,
                        instruction,
                        metadata,
                        builtin_policy,
                    )
                }
                php_runtime::api::CallableValue::Closure(closure) => invoke_native_closure_payload(
                    context,
                    closure,
                    None,
                    None,
                    arguments,
                    instruction,
                    metadata,
                    builtin_policy,
                ),
                php_runtime::api::CallableValue::BoundMethod { target, method, .. } => {
                    invoke_native_bound_method(
                        context,
                        target,
                        method,
                        arguments,
                        metadata,
                        context.unit.strict_types_for_span(instruction.span),
                        caller_function,
                        builtin_policy,
                    )
                }
                php_runtime::api::CallableValue::MethodPlaceholder { target }
                | php_runtime::api::CallableValue::UnresolvedDynamic { target } => {
                    Err(format!("{target} is not callable").into())
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
                        builtin_policy,
                    )
                } else {
                    invoke_native_named_callable(
                        context,
                        &name,
                        arguments,
                        instruction,
                        metadata,
                        builtin_policy,
                    )
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
                builtin_policy,
            ),
            Value::Array(array) => {
                let target = array
                    .get(&php_runtime::api::ArrayKey::Int(0))
                    .cloned()
                    .map(dereference_native_callable_value)
                    .ok_or_else(|| NativeCallControl::from("callable array target is missing"))?;
                let method = array
                    .get(&php_runtime::api::ArrayKey::Int(1))
                    .cloned()
                    .map(dereference_native_callable_value)
                    .ok_or_else(|| NativeCallControl::from("callable array method is missing"))?;
                let Value::String(method) = method else {
                    return Err("callable array method must be a string".into());
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
                        )
                        .into());
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
                    builtin_policy,
                )
            }
            value => Err(format!("{} is not callable", native_value_type_name(&value)).into()),
        }
    })();
    Some(result)
}

pub(super) fn execute_native_dynamic_constructor(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    encoded: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::DynamicNewObject { args, .. } = &instruction.kind else {
        return None;
    };
    let Some((class_name, arguments)) = encoded.split_first() else {
        return Some(Err("dynamic class operand is missing".to_owned()));
    };
    let direct_class_name = context.dereference_direct_encoding(*class_name);
    let class_name = if let Some(bytes) = context.native_string_name_bytes(direct_class_name) {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        // A non-native class-name carrier is already on the explicit dynamic
        // baseline boundary. Do not make ordinary direct strings traverse it.
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
        class_name.to_string_lossy()
    };
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

        let class = native_active_class_handle(context, &class_name)
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
        let object = new_native_object(context, None, &class)?;
        let receiver = context.encode_native_object_owner(object)?;
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
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    encoded: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallMethod { method, .. } = &instruction.kind else {
        return None;
    };
    let receiver = encoded.first()?;
    if let Some(index) = context.direct_generator_index(*receiver) {
        let result = (|| -> Result<i64, String> {
            let discard_entry = |context: &mut NativeRequestColdState<'_>,
                                 entry: Option<(i64, i64)>|
             -> Result<(), String> {
                if let Some((key, value)) = entry {
                    context.release(key)?;
                    context.release(value)?;
                }
                Ok(())
            };
            let lifecycle = |context: &NativeRequestColdState<'_>| {
                context
                    .direct_generator(index)
                    .map(|generator| generator.lifecycle)
                    .ok_or_else(|| format!("direct Generator {index} is missing"))
            };
            let ensure_started = |context: &mut NativeRequestColdState<'_>| {
                if lifecycle(context)? == php_runtime::api::GeneratorState::Created {
                    let entry = context.resume_direct_generator(
                        *receiver,
                        php_jit::JitNativeResumeInputKind::START,
                        php_jit::jit_encode_constant(u32::MAX),
                    )?;
                    discard_entry(context, entry)?;
                }
                Ok::<(), String>(())
            };
            match method.to_ascii_lowercase().as_str() {
                "rewind" => {
                    ensure_started(context)?;
                    if !context.generator_can_rewind(*receiver) {
                        return Err(
                            "E_PHP_THROW:Exception:Cannot rewind a generator that was already run"
                                .to_owned(),
                        );
                    }
                    Ok(php_jit::jit_encode_constant(u32::MAX))
                }
                "valid" => {
                    ensure_started(context)?;
                    Ok(php_jit::jit_encode_constant(
                        if lifecycle(context)? == php_runtime::api::GeneratorState::Suspended {
                            php_jit::JIT_VALUE_TRUE
                        } else {
                            php_jit::JIT_VALUE_FALSE
                        },
                    ))
                }
                "current" | "key" => {
                    ensure_started(context)?;
                    let value = context.direct_generator(index).and_then(|generator| {
                        if method.eq_ignore_ascii_case("current") {
                            generator.current_value
                        } else {
                            generator.current_key
                        }
                    });
                    value.map_or_else(
                        || Ok(php_jit::jit_encode_constant(u32::MAX)),
                        |value| context.duplicate_direct_generator_value(value),
                    )
                }
                "next" => {
                    ensure_started(context)?;
                    if lifecycle(context)? == php_runtime::api::GeneratorState::Suspended {
                        let entry = context.resume_direct_generator(
                            *receiver,
                            php_jit::JitNativeResumeInputKind::VALUE,
                            php_jit::jit_encode_constant(u32::MAX),
                        )?;
                        discard_entry(context, entry)?;
                    }
                    Ok(php_jit::jit_encode_constant(u32::MAX))
                }
                "send" => {
                    ensure_started(context)?;
                    if lifecycle(context)? != php_runtime::api::GeneratorState::Suspended {
                        return Ok(php_jit::jit_encode_constant(u32::MAX));
                    }
                    let input = encoded
                        .get(1)
                        .copied()
                        .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
                    let input = context
                        .duplicate_authoritative_dereferenced_native_value(input)?
                        .ok_or_else(|| {
                            "Generator::send() received a cold compatibility value".to_owned()
                        })?;
                    let next = context.resume_direct_generator(
                        *receiver,
                        php_jit::JitNativeResumeInputKind::VALUE,
                        input,
                    )?;
                    match next {
                        Some((key, value)) => {
                            context.release(key)?;
                            Ok(value)
                        }
                        None => Ok(php_jit::jit_encode_constant(u32::MAX)),
                    }
                }
                "throw" => {
                    ensure_started(context)?;
                    let input = encoded
                        .get(1)
                        .copied()
                        .ok_or_else(|| "Generator::throw() expects an exception".to_owned())?;
                    let input = context
                        .duplicate_authoritative_native_value(input)?
                        .ok_or_else(|| {
                            "Generator::throw() received a cold compatibility value".to_owned()
                        })?;
                    let next = context.resume_direct_generator(
                        *receiver,
                        php_jit::JitNativeResumeInputKind::THROW,
                        input,
                    )?;
                    match next {
                        Some((key, value)) => {
                            context.release(key)?;
                            Ok(value)
                        }
                        None => Ok(php_jit::jit_encode_constant(u32::MAX)),
                    }
                }
                "getreturn" => {
                    if lifecycle(context)? != php_runtime::api::GeneratorState::Closed {
                        return Err(
                            "Cannot get return value of a generator that hasn't returned"
                                .to_owned(),
                        );
                    }
                    let value = context
                        .direct_generator(index)
                        .and_then(|generator| generator.return_value);
                    value.map_or_else(
                        || Ok(php_jit::jit_encode_constant(u32::MAX)),
                        |value| context.duplicate_direct_generator_value(value),
                    )
                }
                _ => Err(format!("Call to undefined method Generator::{method}()")),
            }
        })();
        return Some(result);
    }
    let generator = match context.decode(*receiver) {
        Ok(Value::Generator(generator)) => generator,
        Ok(_) => return None,
        Err(error) => return Some(Err(error)),
    };
    let result = (|| -> Result<i64, String> {
        let iterator = context.baseline_generator_iterator(generator.clone())?;
        let ensure_started = |context: &mut NativeRequestColdState<'_>| {
            if generator.state() == php_runtime::api::GeneratorState::Created {
                context.baseline_iterator_next(iterator).map(|_| ())
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
                    let _ = context.baseline_iterator_next(iterator)?;
                }
                context.encode(Value::Null)
            }
            "send" => {
                ensure_started(context)?;
                let value = encoded
                    .get(1)
                    .copied()
                    .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
                let next = context.resume_baseline_iterator(
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
                let next = context.resume_baseline_iterator(
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

fn take_captured_native_fiber_execution(
    context: &mut NativeRequestColdState<'_>,
    link: u64,
    fallback: Option<&NativeExecutionTarget>,
) -> Result<Option<Box<NativeFiberExecution>>, String> {
    let Some(mut state) = context.take_native_fiber_suspension_state(link)? else {
        return Ok(None);
    };
    if state.control_reserved == NATIVE_FIBER_GENERATOR_FOREACH_CONTINUATION {
        let root = state.yielded_key;
        let active = state.control_value;
        let mut cursor = root;
        let mut parents = Vec::new();
        while cursor != active {
            let index = context.direct_generator_index(cursor).ok_or_else(|| {
                "native Generator Fiber continuation lost its root activation".to_owned()
            })?;
            let child = context
                .direct_generator(index)
                .and_then(|generator| generator.delegation.as_ref())
                .and_then(|delegation| match delegation {
                    NativeGeneratorDelegation::Generator { generator } => Some(*generator),
                    NativeGeneratorDelegation::Array { .. } => None,
                })
                .ok_or_else(|| {
                    "native Generator Fiber continuation lost its delegation chain".to_owned()
                })?;
            parents.push(cursor);
            cursor = child;
        }
        parents.reverse();
        let active_index = context.direct_generator_index(active).ok_or_else(|| {
            "native Generator Fiber continuation lost its active activation".to_owned()
        })?;
        let (target, handle, mut active_state) = context
            .direct_generator(active_index)
            .and_then(|generator| {
                generator
                    .handle
                    .clone()
                    .zip(generator.state)
                    .map(|(handle, state)| (generator.target.clone(), handle, state))
            })
            .ok_or_else(|| {
                "native Generator Fiber continuation has no compiled activation state".to_owned()
            })?;
        let nested_link = std::mem::take(&mut active_state.delegation_handle);
        if let Some(generator) = context.direct_generator_mut(active_index) {
            generator.state = Some(active_state);
        }
        let nested = take_captured_native_fiber_execution(context, nested_link, Some(&target))?;
        return Ok(Some(Box::new(NativeFiberExecution {
            target,
            handle,
            arguments: Vec::new(),
            state: active_state,
            nested,
            generator: Some(NativeGeneratorFiberFrame { active, parents }),
        })));
    }
    let nested_link = std::mem::take(&mut state.delegation_handle);
    let target = context.native_execution_target_from_state(&state, fallback)?;
    let handle = context.run_in_native_execution_target(&target, |context| {
        ensure_native_baseline_entry(context, target.function)
    })?;
    let arity = handle
        .region_state_metadata()
        .and_then(|metadata| {
            metadata
                .function_entries
                .iter()
                .find(|entry| entry.function.raw() == state.function_id)
                .map(|entry| entry.arity)
        })
        .ok_or_else(|| {
            format!(
                "captured native Fiber function {} has no entry metadata",
                state.function_id
            )
        })?;
    let nested = take_captured_native_fiber_execution(context, nested_link, Some(&target))?;
    Ok(Some(Box::new(NativeFiberExecution {
        target,
        handle,
        arguments: vec![0; usize::from(arity)],
        state,
        nested,
        generator: None,
    })))
}

pub(super) fn finish_native_fiber_outcome(
    context: &mut NativeRequestColdState<'_>,
    fiber: &NativeFiberReceiver,
    target: NativeExecutionTarget,
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
            context.discard_native_fiber_suspension_states();
            context.terminate_fiber_receiver(fiber, Some(value))?;
            let fiber_id = context.fiber_receiver_id(fiber)?;
            if let Some(stale) = context.fiber_executions.remove(&fiber_id) {
                context.abandon_native_fiber_execution(stale)?;
            }
            Ok(php_jit::jit_encode_constant(u32::MAX))
        }
        php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            mut state,
        } if status == php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32 => {
            context.set_fiber_receiver_state(fiber, php_runtime::api::FiberState::Suspended)?;
            let fiber_id = context.fiber_receiver_id(fiber)?;
            let suspension_link = std::mem::take(&mut state.delegation_handle);
            let nested =
                match take_captured_native_fiber_execution(context, suspension_link, Some(&target))
                {
                    Ok(nested) => nested,
                    Err(error) => {
                        context.discard_native_fiber_suspension_states();
                        return Err(error);
                    }
                };
            context.fiber_executions.insert(
                fiber_id,
                NativeFiberExecution {
                    target,
                    handle,
                    arguments,
                    state,
                    nested,
                    generator: None,
                },
            );
            Ok(value)
        }
        php_jit::JitI64InvokeOutcome::SideExit { status, value, .. }
            if status == php_jit::JitCallStatus::THROW.0 as i32 =>
        {
            context.discard_native_fiber_suspension_states();
            context.set_fiber_receiver_state(fiber, php_runtime::api::FiberState::Errored)?;
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
        php_jit::JitI64InvokeOutcome::SideExit { status, state, .. } => {
            context.discard_native_fiber_suspension_states();
            context.set_fiber_receiver_state(fiber, php_runtime::api::FiberState::Errored)?;
            let diagnostic = context
                .diagnostic
                .as_ref()
                .map(|diagnostic| format!(": {}", diagnostic.message()))
                .unwrap_or_default();
            let continuation = context
                .instruction_for_continuation(state.function_id, state.continuation_id)
                .map(|instruction| format!(" at {:?}", instruction.kind))
                .unwrap_or_else(|| {
                    format!(
                        " at native continuation {}:{}",
                        state.function_id, state.continuation_id
                    )
                });
            Err(format!(
                "native fiber returned status {status}{continuation}{diagnostic}"
            ))
        }
    }
}

pub(super) fn execute_native_fiber_suspend(
    context: &mut NativeRequestColdState<'_>,
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

enum NativeFiberExecutionOutcome {
    Suspended {
        execution: NativeFiberExecution,
        value: i64,
    },
    Completed {
        target: NativeExecutionTarget,
        handle: php_jit::JitFunctionHandle,
        arguments: Vec<i64>,
        outcome: php_jit::JitI64InvokeOutcome,
        generator_entry: Option<(i64, i64, i64)>,
    },
}

fn completed_native_fiber_control(
    outcome: &php_jit::JitI64InvokeOutcome,
) -> Result<(php_jit::JitCallStatus, i64), String> {
    match outcome {
        php_jit::JitI64InvokeOutcome::Returned(value) => {
            Ok((php_jit::JitCallStatus::RETURN, *value))
        }
        php_jit::JitI64InvokeOutcome::SideExit { status, value, .. } => {
            let status = php_jit::JitCallStatus(*status as u32);
            if matches!(
                status,
                php_jit::JitCallStatus::RETURN
                    | php_jit::JitCallStatus::RETURN_REFERENCE
                    | php_jit::JitCallStatus::THROW
                    | php_jit::JitCallStatus::EXIT
                    | php_jit::JitCallStatus::RUNTIME_ERROR
            ) {
                Ok((status, *value))
            } else {
                Err(format!("native nested fiber returned status {}", status.0))
            }
        }
    }
}

fn classify_native_fiber_execution_outcome(
    context: &mut NativeRequestColdState<'_>,
    mut execution: NativeFiberExecution,
    outcome: php_jit::JitI64InvokeOutcome,
) -> Result<NativeFiberExecutionOutcome, String> {
    match outcome {
        php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            mut state,
        } if status == php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32 => {
            let suspension_link = std::mem::take(&mut state.delegation_handle);
            execution.state = state;
            execution.nested = take_captured_native_fiber_execution(
                context,
                suspension_link,
                Some(&execution.target),
            )?;
            Ok(NativeFiberExecutionOutcome::Suspended { execution, value })
        }
        outcome => Ok(NativeFiberExecutionOutcome::Completed {
            target: execution.target,
            handle: execution.handle,
            arguments: execution.arguments,
            outcome,
            generator_entry: None,
        }),
    }
}

fn resume_native_generator_fiber_execution(
    context: &mut NativeRequestColdState<'_>,
    mut execution: NativeFiberExecution,
    frame: NativeGeneratorFiberFrame,
    kind: php_jit::JitNativeResumeInputKind,
    value: i64,
) -> Result<NativeFiberExecutionOutcome, String> {
    let advance = if let Some(nested) = execution.nested.take() {
        match resume_native_fiber_execution(context, *nested, kind, value) {
            Ok(NativeFiberExecutionOutcome::Suspended {
                execution: nested,
                value,
            }) => {
                execution.nested = Some(Box::new(nested));
                return Ok(NativeFiberExecutionOutcome::Suspended { execution, value });
            }
            Ok(NativeFiberExecutionOutcome::Completed {
                outcome,
                generator_entry,
                ..
            }) => {
                if generator_entry.is_some() {
                    context.abandon_native_fiber_execution(execution)?;
                    return Err(
                        "nested Generator foreach completion escaped its compiled caller"
                            .to_owned(),
                    );
                }
                let (status, value) = completed_native_fiber_control(&outcome)?;
                context.completed_nested_fiber_call = Some((
                    execution.state.function_id,
                    execution.state.continuation_id,
                    status,
                    value,
                ));
                execution.state.control_status = status;
                execution.state.control_value = value;
                let active_index =
                    context
                        .direct_generator_index(frame.active)
                        .ok_or_else(|| {
                            "active direct Generator disappeared during Fiber resume".to_owned()
                        })?;
                if let Some(generator) = context.direct_generator_mut(active_index) {
                    generator.state = Some(execution.state);
                }
                context.advance_direct_generator(
                    frame.active,
                    php_jit::JitNativeResumeInputKind::VALUE,
                    php_jit::jit_encode_constant(u32::MAX),
                )?
            }
            Err(error) => {
                context.abandon_native_fiber_execution(execution)?;
                return Err(error);
            }
        }
    } else {
        let active_index = context
            .direct_generator_index(frame.active)
            .ok_or_else(|| "active direct Generator disappeared during Fiber resume".to_owned())?;
        if let Some(generator) = context.direct_generator_mut(active_index) {
            generator.state = Some(execution.state);
        }
        context.advance_direct_generator(frame.active, kind, value)?
    };

    match context.propagate_direct_generator_fiber_advance(frame.active, frame.parents, advance)? {
        NativeGeneratorAdvance::Yielded { key, value } => {
            Ok(NativeFiberExecutionOutcome::Completed {
                target: execution.target,
                handle: execution.handle,
                arguments: execution.arguments,
                outcome: php_jit::JitI64InvokeOutcome::SideExit {
                    status: php_jit::JitCallStatus::RETURN.0 as i32,
                    value,
                    state: php_jit::JitDeoptState::default(),
                },
                generator_entry: Some((key, value, 1)),
            })
        }
        NativeGeneratorAdvance::Complete => {
            let missing = php_jit::jit_encode_constant(u32::MAX);
            Ok(NativeFiberExecutionOutcome::Completed {
                target: execution.target,
                handle: execution.handle,
                arguments: execution.arguments,
                outcome: php_jit::JitI64InvokeOutcome::SideExit {
                    status: php_jit::JitCallStatus::RETURN.0 as i32,
                    value: missing,
                    state: php_jit::JitDeoptState::default(),
                },
                generator_entry: Some((missing, missing, 0)),
            })
        }
        NativeGeneratorAdvance::FiberSuspended {
            value,
            active,
            parents,
        } => {
            let active_index = context.direct_generator_index(active).ok_or_else(|| {
                "resuspended direct Generator lost its native activation".to_owned()
            })?;
            let (target, handle, mut state) = context
                .direct_generator(active_index)
                .and_then(|generator| {
                    generator
                        .handle
                        .clone()
                        .zip(generator.state)
                        .map(|(handle, state)| (generator.target.clone(), handle, state))
                })
                .ok_or_else(|| {
                    "resuspended direct Generator has no compiled activation state".to_owned()
                })?;
            let nested_link = std::mem::take(&mut state.delegation_handle);
            if let Some(generator) = context.direct_generator_mut(active_index) {
                generator.state = Some(state);
            }
            let nested = take_captured_native_fiber_execution(context, nested_link, Some(&target))?;
            Ok(NativeFiberExecutionOutcome::Suspended {
                execution: NativeFiberExecution {
                    target,
                    handle,
                    arguments: Vec::new(),
                    state,
                    nested,
                    generator: Some(NativeGeneratorFiberFrame { active, parents }),
                },
                value,
            })
        }
    }
}

fn resume_native_fiber_execution(
    context: &mut NativeRequestColdState<'_>,
    execution: NativeFiberExecution,
    kind: php_jit::JitNativeResumeInputKind,
    value: i64,
) -> Result<NativeFiberExecutionOutcome, String> {
    if let Some(generator) = execution.generator.clone() {
        return resume_native_generator_fiber_execution(context, execution, generator, kind, value);
    }
    let target = execution.target.clone();
    context.run_in_native_execution_target(&target, |context| {
        resume_native_fiber_execution_in_target(context, execution, kind, value)
    })
}

fn resume_native_fiber_execution_in_target(
    context: &mut NativeRequestColdState<'_>,
    mut execution: NativeFiberExecution,
    kind: php_jit::JitNativeResumeInputKind,
    value: i64,
) -> Result<NativeFiberExecutionOutcome, String> {
    let outcome = if let Some(nested) = execution.nested.take() {
        match resume_native_fiber_execution(context, *nested, kind, value) {
            Ok(NativeFiberExecutionOutcome::Suspended {
                execution: nested,
                value,
            }) => {
                execution.nested = Some(Box::new(nested));
                return Ok(NativeFiberExecutionOutcome::Suspended { execution, value });
            }
            Ok(NativeFiberExecutionOutcome::Completed {
                outcome,
                generator_entry,
                ..
            }) => {
                let (status, value) = match completed_native_fiber_control(&outcome) {
                    Ok(completed) => completed,
                    Err(error) => {
                        context.abandon_native_fiber_execution(execution)?;
                        return Err(error);
                    }
                };
                context.completed_nested_fiber_call = Some((
                    execution.state.function_id,
                    execution.state.continuation_id,
                    status,
                    value,
                ));
                execution.state.control_status = status;
                execution.state.control_value = value;
                if let Some((key, _, has)) = generator_entry {
                    execution.state.yielded_key = key;
                    execution.state.suspend_flags = has as u32;
                }
                let runtime = context.native_runtime_ptr();
                let resumed = execution
                    .handle
                    .invoke_i64_same_artifact_transition_with_unwind_runtime(
                        &execution.state,
                        php_jit::JIT_RUNTIME_ABI_HASH,
                        runtime,
                        |types, value| native_catch_matches(context, types, value),
                    );
                if context.completed_nested_fiber_call.as_ref().is_some_and(
                    |(function, continuation, _, _)| {
                        *function == execution.state.function_id
                            && *continuation == execution.state.continuation_id
                    },
                ) {
                    context.completed_nested_fiber_call = None;
                }
                match resumed {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        context.abandon_native_fiber_execution(execution)?;
                        return Err(format!("native fiber caller resume failed: {error:?}"));
                    }
                }
            }
            Err(error) => {
                context.abandon_native_fiber_execution(execution)?;
                return Err(error);
            }
        }
    } else {
        let runtime = context.native_runtime_ptr();
        match execution
            .handle
            .invoke_i64_suspension_resume_with_native_unwind_runtime(
                &execution.arguments,
                &execution.state,
                kind,
                value,
                php_jit::JIT_RUNTIME_ABI_HASH,
                runtime,
                |types, value| native_catch_matches(context, types, value),
            ) {
            Ok(outcome) => outcome,
            Err(error) => {
                context.abandon_native_fiber_execution(execution)?;
                return Err(format!("native fiber resume failed: {error:?}"));
            }
        }
    };
    classify_native_fiber_execution_outcome(context, execution, outcome)
}

pub(super) fn execute_native_fiber_method(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    encoded: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallMethod { method, .. } = &instruction.kind else {
        return None;
    };
    let receiver = encoded.first()?;
    let fiber = match context.native_fiber_receiver(*receiver) {
        Ok(Some(fiber)) => fiber,
        Ok(None) => return None,
        Err(error) => return Some(Err(error)),
    };
    let result = (|| -> Result<i64, String> {
        match method.to_ascii_lowercase().as_str() {
            "isstarted" => Ok(php_jit::jit_encode_constant(
                if context.fiber_receiver_state(&fiber)? != php_runtime::api::FiberState::NotStarted
                {
                    php_jit::JIT_VALUE_TRUE
                } else {
                    php_jit::JIT_VALUE_FALSE
                },
            )),
            "issuspended" => Ok(php_jit::jit_encode_constant(
                if context.fiber_receiver_state(&fiber)? == php_runtime::api::FiberState::Suspended
                {
                    php_jit::JIT_VALUE_TRUE
                } else {
                    php_jit::JIT_VALUE_FALSE
                },
            )),
            "isrunning" => Ok(php_jit::jit_encode_constant(
                if context.fiber_receiver_state(&fiber)? == php_runtime::api::FiberState::Running {
                    php_jit::JIT_VALUE_TRUE
                } else {
                    php_jit::JIT_VALUE_FALSE
                },
            )),
            "isterminated" => Ok(php_jit::jit_encode_constant(
                if context.fiber_receiver_state(&fiber)? == php_runtime::api::FiberState::Terminated
                {
                    php_jit::JIT_VALUE_TRUE
                } else {
                    php_jit::JIT_VALUE_FALSE
                },
            )),
            "getreturn" => {
                let fiber_state = context.fiber_receiver_state(&fiber)?;
                if fiber_state != php_runtime::api::FiberState::Terminated {
                    let state = if fiber_state == php_runtime::api::FiberState::NotStarted {
                        "been started"
                    } else {
                        "returned"
                    };
                    return Err(format!(
                        "E_PHP_THROW:FiberError:Cannot get fiber return value: The fiber has not {state}"
                    ));
                }
                context
                    .fiber_receiver_return_value(&fiber)?
                    .map_or_else(|| Ok(php_jit::jit_encode_constant(u32::MAX)), Ok)
            }
            "start" => {
                if context.fiber_receiver_state(&fiber)? != php_runtime::api::FiberState::NotStarted
                {
                    return Err(
                        "E_PHP_THROW:FiberError:Cannot start a fiber that has already been started"
                            .to_owned(),
                    );
                }
                let callable = context.fiber_receiver_callable(&fiber)?;
                let Some((closure, implicit_this, captures)) =
                    context.prepared_closure_invocation(callable)
                else {
                    return Err("Fiber callback must resolve to a native closure".to_owned());
                };
                let function = php_ir::FunctionId::new(closure.function);
                let owner_unit = closure.context.owner_unit;
                let target = NativeExecutionTarget {
                    unit: owner_unit,
                    function,
                    called_class: closure.context.called_class.clone(),
                    scope_class: closure.context.scope_class.clone(),
                };
                if let Some(unit) = target.unit {
                    prepare_dynamic_native_entry(context, unit, function)?;
                }
                let handle = context.run_in_native_execution_target(&target, |context| {
                    ensure_native_entry(context, function)
                })?;
                let has_implicit_this = owner_unit
                    .and_then(|unit| context.dynamic_units.get(unit))
                    .map(|package| package.compiled.unit())
                    .unwrap_or(&*context.unit)
                    .functions
                    .get(function.index())
                    .and_then(php_ir::IrFunction::implicit_closure_this_local)
                    .is_some();
                let implicit_this = has_implicit_this.then(|| {
                    implicit_this.unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX))
                });
                let mut arguments = Vec::with_capacity(
                    usize::from(implicit_this.is_some()) + captures.len() + encoded.len() - 1,
                );
                let direct_fiber = matches!(&fiber, NativeFiberReceiver::Direct(_));
                let start_arguments = implicit_this
                    .into_iter()
                    .chain(captures.iter().copied())
                    .map(|argument| (argument, false))
                    .chain(
                        encoded[1..]
                            .iter()
                            .copied()
                            .map(|argument| (argument, true)),
                    );
                for (argument, by_value) in start_arguments {
                    let duplicated = match (direct_fiber, by_value) {
                        (true, true) => {
                            match context
                                .duplicate_authoritative_dereferenced_native_value(argument)
                            {
                                Ok(Some(argument)) => Ok(argument),
                                // A direct Fiber may be started from the
                                // baseline-native tier with a materialized
                                // top-level lvalue. This is the already-taken
                                // cold continuation, not an optimizing path.
                                Ok(None) => context.duplicate_dereferenced_native_value(argument),
                                Err(error) => Err(error),
                            }
                        }
                        (true, false) => {
                            match context.duplicate_authoritative_native_value(argument) {
                                Ok(Some(argument)) => Ok(argument),
                                Ok(None) => context.duplicate_baseline_call_argument(argument),
                                Err(error) => Err(error),
                            }
                        }
                        (false, true) => context.duplicate_dereferenced_native_value(argument),
                        (false, false) => context.duplicate_baseline_call_argument(argument),
                    };
                    match duplicated {
                        Ok(argument) => arguments.push(argument),
                        Err(error) => {
                            for argument in arguments {
                                let _ = context.release_if_live(argument);
                            }
                            return Err(error);
                        }
                    }
                }
                if target.unit != context.current_dynamic_unit
                    && let Err(error) =
                        context.stabilize_owned_native_values_for_cross_unit(&mut arguments)
                {
                    for argument in arguments {
                        let _ = context.release_if_live(argument);
                    }
                    return Err(error);
                }
                context.set_fiber_receiver_state(&fiber, php_runtime::api::FiberState::Running)?;
                let fiber_id = context.fiber_receiver_id(&fiber)?;
                let previous_fiber = context.active_fiber.replace(fiber_id);
                let invocation_target = target.clone();
                let mut arguments = Some(arguments);
                let outcome = context.run_in_native_execution_target(&target, |context| {
                    let arguments = arguments
                        .take()
                        .expect("Fiber arguments enter their native frame exactly once");
                    let runtime = context.native_runtime_ptr();
                    let outcome = handle.invoke_i64_with_deopt_runtime(
                        &arguments,
                        php_jit::JIT_RUNTIME_ABI_HASH,
                        runtime,
                    );
                    let (handle, outcome) = match resume_native_optimizing_exit_with_artifact(
                        context,
                        Some(handle),
                        outcome,
                    ) {
                        Ok((handle, outcome)) => (
                            handle.expect("Fiber invocation always has an active artifact"),
                            outcome,
                        ),
                        Err(error) => {
                            for argument in arguments {
                                context.release_if_live(argument)?;
                            }
                            return Err(format!("native fiber invocation failed: {error:?}"));
                        }
                    };
                    finish_native_fiber_outcome(
                        context,
                        &fiber,
                        invocation_target,
                        handle,
                        arguments,
                        outcome,
                    )
                });
                context.active_fiber = previous_fiber;
                if let Some(arguments) = arguments {
                    for argument in arguments {
                        context.release_if_live(argument)?;
                    }
                }
                if outcome.is_err() {
                    context
                        .set_fiber_receiver_state(&fiber, php_runtime::api::FiberState::Errored)?;
                }
                outcome
            }
            "resume" | "throw" => {
                if context.fiber_receiver_state(&fiber)? != php_runtime::api::FiberState::Suspended
                {
                    return Err(
                        "E_PHP_THROW:FiberError:Cannot resume a fiber that is not suspended"
                            .to_owned(),
                    );
                }
                let fiber_id = context.fiber_receiver_id(&fiber)?;
                let execution = context
                    .fiber_executions
                    .remove(&fiber_id)
                    .ok_or_else(|| "native fiber suspension state is missing".to_owned())?;
                let resume_target = execution.resume_target().clone();
                let mut value = if let Some(value) = encoded.get(1).copied() {
                    if matches!(&fiber, NativeFiberReceiver::Direct(_)) {
                        match context.duplicate_authoritative_dereferenced_native_value(value)? {
                            Some(value) => value,
                            // Materialized values are admitted only after the
                            // explicit baseline-native Fiber boundary.
                            None => context.duplicate_dereferenced_native_value(value)?,
                        }
                    } else {
                        context.duplicate_dereferenced_native_value(value)?
                    }
                } else {
                    php_jit::jit_encode_constant(u32::MAX)
                };
                if resume_target.unit != context.current_dynamic_unit {
                    let mut input = [value];
                    if let Err(error) =
                        context.stabilize_owned_native_values_for_cross_unit(&mut input)
                    {
                        let _ = context.release_if_live(value);
                        context.abandon_native_fiber_execution(execution)?;
                        return Err(error);
                    }
                    value = input[0];
                }
                context.set_fiber_receiver_state(&fiber, php_runtime::api::FiberState::Running)?;
                let kind = if method.eq_ignore_ascii_case("throw") {
                    php_jit::JitNativeResumeInputKind::THROW
                } else {
                    php_jit::JitNativeResumeInputKind::VALUE
                };
                let previous_fiber = context.active_fiber.replace(fiber_id);
                let outcome = resume_native_fiber_execution(context, execution, kind, value);
                context.active_fiber = previous_fiber;
                match outcome {
                    Ok(NativeFiberExecutionOutcome::Suspended { execution, value }) => {
                        context.fiber_executions.insert(fiber_id, execution);
                        context.set_fiber_receiver_state(
                            &fiber,
                            php_runtime::api::FiberState::Suspended,
                        )?;
                        Ok(value)
                    }
                    Ok(NativeFiberExecutionOutcome::Completed {
                        target,
                        handle,
                        arguments,
                        outcome,
                        generator_entry,
                    }) => {
                        if generator_entry.is_some() {
                            return Err(
                                "Generator foreach completion escaped the suspended Fiber caller"
                                    .to_owned(),
                            );
                        }
                        context.run_in_native_execution_target(&target, |context| {
                            finish_native_fiber_outcome(
                                context,
                                &fiber,
                                target.clone(),
                                handle,
                                arguments,
                                outcome,
                            )
                        })
                    }
                    Err(error) => {
                        context.set_fiber_receiver_state(
                            &fiber,
                            php_runtime::api::FiberState::Errored,
                        )?;
                        Err(error)
                    }
                }
            }
            _ => Err(format!("Call to undefined method Fiber::{method}()")),
        }
    })();
    Some(result)
}

pub(super) fn invoke_native_callable_value(
    context: &mut NativeRequestColdState<'_>,
    callable: Value,
    arguments: &[Value],
    source: &php_ir::Instruction,
    metadata: Option<Vec<php_ir::instruction::IrCallArg>>,
) -> Result<i64, String> {
    invoke_native_callable_value_from(context, callable, arguments, source, metadata, None)
}

pub(super) fn invoke_native_callable_value_from(
    context: &mut NativeRequestColdState<'_>,
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
    let result = invoke_native_encoded_callable_value_from(
        context,
        &encoded,
        source,
        metadata,
        caller_function,
        NativeCallableBuiltinPolicy::ExecuteBaseline,
    );
    let mut release_error = None;
    for value in encoded {
        if let Err(error) = context.release_if_live(value) {
            release_error.get_or_insert(error);
        }
    }
    match (result, release_error) {
        (Err(control), _) => Err(control.into_baseline_error()),
        (Ok(_), Some(error)) => Err(error),
        (Ok(value), None) => Ok(value),
    }
}

pub(super) fn invoke_native_encoded_callable_value_from(
    context: &mut NativeRequestColdState<'_>,
    encoded: &[i64],
    source: &php_ir::Instruction,
    metadata: Option<Vec<php_ir::instruction::IrCallArg>>,
    caller_function: Option<u32>,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    execute_native_dynamic_callable(
        context,
        source,
        encoded,
        caller_function,
        metadata.as_deref(),
        true,
        builtin_policy,
    )
    .unwrap_or_else(|| Err("dynamic callable dispatch was not selected".into()))
}
