//! Callable dispatch and dynamic call execution.

use super::builtin_intrinsics::try_execute_simple_literal_pcre_builtin;
use super::prelude::*;

fn internal_registry_builtin_call_name(name: &str) -> Option<String> {
    let registry = BuiltinRegistry::new();
    if registry.contains(name) {
        return Some(name.to_owned());
    }
    let short = name.rsplit_once('\\')?.1;
    if short.is_empty() {
        return None;
    }
    registry.contains(short).then(|| short.to_owned())
}

pub(super) fn builtin_function_call_target(name: &str) -> Option<FunctionCallCacheTarget> {
    if is_autoload_builtin_name(name) || is_symbol_introspection_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::AutoloadOrSymbolIntrospection,
            name: Arc::from(name),
        });
    }
    if is_config_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::Config,
            name: Arc::from(name),
        });
    }
    if is_error_handling_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::ErrorHandling,
            name: Arc::from(name),
        });
    }
    if is_output_buffering_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::OutputBuffering,
            name: Arc::from(name),
        });
    }
    if is_environment_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::Environment,
            name: Arc::from(name),
        });
    }
    if is_process_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::Process,
            name: Arc::from(name),
        });
    }
    if is_pcre_callback_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::PcreCallback,
            name: Arc::from(name),
        });
    }
    if is_filter_callback_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::FilterCallback,
            name: Arc::from(name),
        });
    }
    if is_array_callback_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::ArrayCallback,
            name: Arc::from(name),
        });
    }
    if is_array_sort_builtin_name(name) {
        return Some(FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::ArraySort,
            name: Arc::from(name),
        });
    }
    internal_registry_builtin_call_name(name).map(|name| FunctionCallCacheTarget::Builtin {
        kind: FunctionCallBuiltinKind::InternalRegistry,
        name: Arc::from(name),
    })
}

pub(super) fn namespaced_function_global_fallback(name: &str) -> Option<&str> {
    let short = name.rsplit_once('\\')?.1;
    (!short.is_empty()).then_some(short)
}

pub(super) fn load_local_is_pre_call_by_ref_out_param(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    instructions: &[Instruction],
    instruction_index: usize,
    dst: RegId,
    local: LocalId,
) -> bool {
    let loaded = Operand::Register(dst);
    for instruction in instructions.iter().skip(instruction_index + 1) {
        match &instruction.kind {
            InstructionKind::Nop => continue,
            InstructionKind::LoadConst { dst: next_dst, .. }
            | InstructionKind::FetchConst { dst: next_dst, .. }
            | InstructionKind::LoadLocal { dst: next_dst, .. }
            | InstructionKind::LoadLocalQuiet { dst: next_dst, .. } => {
                if *next_dst == dst {
                    return false;
                }
                continue;
            }
            InstructionKind::CallFunction { name, args, .. } => {
                return call_function_load_is_actual_by_ref_arg(
                    compiled, state, name, args, loaded, local,
                );
            }
            InstructionKind::CallMethod { args, .. }
            | InstructionKind::CallStaticMethod { args, .. }
            | InstructionKind::CallClosure { args, .. }
            | InstructionKind::CallCallable { args, .. }
            | InstructionKind::NewObject { args, .. }
            | InstructionKind::BindReferenceFromCall { args, .. }
            | InstructionKind::BindReferenceFromMethodCall { args, .. } => {
                return args.iter().any(|arg| {
                    arg.value == loaded
                        && arg.by_ref_local.is_some_and(|arg_local| arg_local == local)
                });
            }
            _ => return false,
        }
    }
    false
}

fn call_function_load_is_actual_by_ref_arg(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    name: &str,
    args: &[IrCallArg],
    loaded: Operand,
    local: LocalId,
) -> bool {
    args.iter().enumerate().any(|(index, arg)| {
        arg.value == loaded
            && arg.by_ref_local.is_some_and(|arg_local| arg_local == local)
            && direct_function_arg_requires_reference(compiled, state, name, index, arg)
    })
}

fn direct_function_arg_requires_reference(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    name: &str,
    index: usize,
    arg: &IrCallArg,
) -> bool {
    let normalized = normalize_function_name(name);
    if !normalized.contains('\\') && builtin_function_call_target(&normalized).is_some() {
        return direct_builtin_arg_requires_reference(
            &normalized,
            index,
            ir_call_arg_has_by_ref_metadata(arg),
        );
    }
    if let Some(function_id) = compiled.lookup_function(&normalized)
        && let Some(function) = compiled.unit().functions.get(function_id.index())
    {
        return ir_function_arg_requires_reference(function, index, arg);
    }
    if let Some((owner, function_id)) = dynamic_function_in_state(state, &normalized)
        && let Some(function) = owner.unit().functions.get(function_id.index())
    {
        return ir_function_arg_requires_reference(function, index, arg);
    }
    if let Some(fallback_name) = namespaced_function_global_fallback(&normalized) {
        if let Some(function_id) = compiled.lookup_function(fallback_name)
            && let Some(function) = compiled.unit().functions.get(function_id.index())
        {
            return ir_function_arg_requires_reference(function, index, arg);
        }
        if let Some((owner, function_id)) = dynamic_function_in_state(state, fallback_name)
            && let Some(function) = owner.unit().functions.get(function_id.index())
        {
            return ir_function_arg_requires_reference(function, index, arg);
        }
    }
    direct_builtin_arg_requires_reference(&normalized, index, ir_call_arg_has_by_ref_metadata(arg))
}

fn ir_call_arg_has_by_ref_metadata(arg: &IrCallArg) -> bool {
    arg.by_ref_local.is_some()
        || arg.by_ref_dim.is_some()
        || arg.by_ref_property.is_some()
        || arg.by_ref_property_dim.is_some()
}

fn ir_function_arg_requires_reference(
    function: &IrFunction,
    index: usize,
    arg: &IrCallArg,
) -> bool {
    ir_function_param_requires_reference(function, index, arg.name.as_deref())
}

fn ir_function_param_requires_reference(
    function: &IrFunction,
    index: usize,
    arg_name: Option<&str>,
) -> bool {
    let param = if let Some(name) = arg_name {
        function.params.iter().find(|param| param.name == name)
    } else {
        function.params.get(index).or_else(|| {
            function
                .params
                .last()
                .filter(|param| param.variadic && index >= function.params.len())
        })
    };
    param.is_some_and(|param| param.by_ref)
}

fn direct_builtin_arg_requires_reference(
    name: &str,
    index: usize,
    arg_is_referenceable: bool,
) -> bool {
    let target = builtin_function_call_target(name).or_else(|| {
        namespaced_function_global_fallback(name).and_then(builtin_function_call_target)
    });
    let Some(FunctionCallCacheTarget::Builtin { kind, name }) = target else {
        return false;
    };
    match kind {
        FunctionCallBuiltinKind::InternalRegistry | FunctionCallBuiltinKind::PcreCallback => {
            internal_builtin_param_requires_reference(&name, index)
        }
        FunctionCallBuiltinKind::FilterCallback => {
            internal_builtin_param_requires_reference(&name, index)
        }
        FunctionCallBuiltinKind::Process => process_builtin_param_requires_reference(&name, index),
        FunctionCallBuiltinKind::ArraySort => {
            array_sort_builtin_param_requires_reference(&name, index, arg_is_referenceable)
        }
        FunctionCallBuiltinKind::ArrayCallback => {
            array_callback_builtin_param_requires_reference(&name, index)
        }
        _ => false,
    }
}

fn array_callback_builtin_param_requires_reference(function: &str, index: usize) -> bool {
    matches!(function, "array_walk" | "array_walk_recursive") && index == 0
}

pub(super) fn internal_builtin_param_requires_reference(function: &str, index: usize) -> bool {
    if let Some(metadata) = php_std::arginfo::function_metadata_indexed(function)
        && metadata.params.get(index).is_some_and(|param| param.by_ref)
    {
        return true;
    }
    (function == "str_replace" && index == 3)
        || (function == "parse_str" && index == 1)
        || (function == "mysqli_stmt_bind_param" && index >= 2)
        || (function == "mysqli_stmt_bind_result" && index >= 1)
        || (function == "curl_multi_exec" && index == 1)
        || (function == "openssl_random_pseudo_bytes" && index == 1)
        || (matches!(function, "preg_match" | "preg_match_all") && index == 2)
        || (function == "preg_replace" && index == 4)
        || (function == "preg_replace_callback" && index == 4)
        || (function == "preg_replace_callback_array" && index == 3)
        || (function == "apcu_fetch" && index == 1)
        || (matches!(function, "apcu_dec" | "apcu_inc") && index == 2)
        || (matches!(
            function,
            "array_pop"
                | "array_push"
                | "array_shift"
                | "array_splice"
                | "array_unshift"
                | "end"
                | "next"
                | "prev"
                | "reset"
                | "shuffle"
        ) && index == 0)
}

fn process_builtin_param_requires_reference(function: &str, index: usize) -> bool {
    matches!((function, index), ("exec", 1 | 2) | ("passthru", 1))
}

fn array_sort_builtin_param_requires_reference(
    function: &str,
    index: usize,
    arg_is_referenceable: bool,
) -> bool {
    if function == "array_multisort" {
        return arg_is_referenceable;
    }
    index == 0
}

pub(super) fn dense_load_local_is_pre_call_by_ref_out_param(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    dense: &DenseBytecodeUnit,
    instructions: &[DenseInstruction],
    instruction_offset: usize,
    dst: u32,
    local: u32,
) -> bool {
    let loaded = DenseOperand {
        kind: DenseOperandKind::Register,
        index: dst,
    };
    for instruction in instructions.iter().skip(instruction_offset + 1) {
        match instruction.opcode {
            DenseOpcode::Nop => continue,
            DenseOpcode::LoadConst
            | DenseOpcode::LoadConstEcho
            | DenseOpcode::FetchConst
            | DenseOpcode::Move
            | DenseOpcode::LoadLocal
            | DenseOpcode::LoadLocalEcho => {
                if dense_instruction_dst(instruction) == Some(dst) {
                    return false;
                }
                continue;
            }
            DenseOpcode::LoadLocalLoadConst => {
                let DenseOperands::LoadLocalLoadConst {
                    first_dst,
                    second_dst,
                    ..
                } = instruction.operands
                else {
                    return false;
                };
                if first_dst == dst || second_dst == dst {
                    return false;
                }
                continue;
            }
            DenseOpcode::CallFunction | DenseOpcode::CallFunctionDiscard => {
                let DenseOperands::Call { name, ref args, .. } = instruction.operands else {
                    return false;
                };
                let Some(name) = dense.names.get(name as usize) else {
                    return false;
                };
                return dense_call_function_load_is_actual_by_ref_arg(
                    compiled, state, dense, name, args, loaded, local,
                );
            }
            DenseOpcode::CallMethod | DenseOpcode::CallStaticMethod => {
                let Some(args) = dense_instruction_call_args(instruction) else {
                    return false;
                };
                return args.iter().any(|arg| {
                    arg.value == loaded
                        && arg.by_ref_local.is_some_and(|arg_local| arg_local == local)
                });
            }
            _ => return false,
        }
    }
    false
}

fn dense_call_function_load_is_actual_by_ref_arg(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    dense: &DenseBytecodeUnit,
    name: &str,
    args: &[DenseCallArg],
    loaded: DenseOperand,
    local: u32,
) -> bool {
    args.iter().enumerate().any(|(index, arg)| {
        arg.value == loaded
            && arg.by_ref_local.is_some_and(|arg_local| arg_local == local)
            && dense_direct_function_arg_requires_reference(
                compiled, state, dense, name, index, arg,
            )
    })
}

fn dense_direct_function_arg_requires_reference(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    dense: &DenseBytecodeUnit,
    name: &str,
    index: usize,
    arg: &DenseCallArg,
) -> bool {
    let normalized = normalize_function_name(name);
    if !normalized.contains('\\') && builtin_function_call_target(&normalized).is_some() {
        return direct_builtin_arg_requires_reference(
            &normalized,
            index,
            dense_call_arg_has_by_ref_metadata(arg),
        );
    }
    let arg_name = arg
        .name
        .and_then(|name| dense.names.get(name as usize).map(String::as_str));
    if let Some(function_id) = compiled.lookup_function(&normalized)
        && let Some(function) = compiled.unit().functions.get(function_id.index())
    {
        return ir_function_param_requires_reference(function, index, arg_name);
    }
    if let Some((owner, function_id)) = dynamic_function_in_state(state, &normalized)
        && let Some(function) = owner.unit().functions.get(function_id.index())
    {
        return ir_function_param_requires_reference(function, index, arg_name);
    }
    if let Some(fallback_name) = namespaced_function_global_fallback(&normalized) {
        if let Some(function_id) = compiled.lookup_function(fallback_name)
            && let Some(function) = compiled.unit().functions.get(function_id.index())
        {
            return ir_function_param_requires_reference(function, index, arg_name);
        }
        if let Some((owner, function_id)) = dynamic_function_in_state(state, fallback_name)
            && let Some(function) = owner.unit().functions.get(function_id.index())
        {
            return ir_function_param_requires_reference(function, index, arg_name);
        }
    }
    direct_builtin_arg_requires_reference(
        &normalized,
        index,
        dense_call_arg_has_by_ref_metadata(arg),
    )
}

fn dense_call_arg_has_by_ref_metadata(arg: &DenseCallArg) -> bool {
    arg.by_ref_local.is_some()
        || arg.by_ref_dim.is_some()
        || arg.by_ref_property.is_some()
        || arg.by_ref_property_dim.is_some()
}

fn dense_instruction_dst(instruction: &DenseInstruction) -> Option<u32> {
    match instruction.operands {
        DenseOperands::RegConst { dst, .. }
        | DenseOperands::RegOperand { dst, .. }
        | DenseOperands::Binary { dst, .. }
        | DenseOperands::Call { dst, .. }
        | DenseOperands::MethodCall { dst, .. }
        | DenseOperands::StaticCall { dst, .. }
        | DenseOperands::RegName { dst, .. }
        | DenseOperands::Cast { dst, .. }
        | DenseOperands::Dst { dst }
        | DenseOperands::FetchDim { dst, .. }
        | DenseOperands::AssignDim { dst, .. }
        | DenseOperands::IssetDim { dst, .. }
        | DenseOperands::EmptyDim { dst, .. }
        | DenseOperands::ForeachNext { has_value: dst, .. }
        | DenseOperands::FetchProperty { dst, .. }
        | DenseOperands::AssignProperty { dst, .. } => Some(dst),
        _ => None,
    }
}

fn dense_instruction_call_args(instruction: &DenseInstruction) -> Option<&[DenseCallArg]> {
    match &instruction.operands {
        DenseOperands::Call { args, .. }
        | DenseOperands::MethodCall { args, .. }
        | DenseOperands::StaticCall { args, .. } => Some(args),
        _ => None,
    }
}

pub(super) fn read_call_args_for_function_at_frame(
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    function: &str,
    args: &[IrCallArg],
) -> Result<Vec<CallArgument>, String> {
    read_call_args_with_value_policy_at_frame(unit, stack, frame_index, args, |index, arg| {
        is_quiet_by_ref_internal_builtin_arg(function, index, arg)
    })
}

pub(super) fn read_call_args_at_frame(
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    args: &[IrCallArg],
) -> Result<Vec<CallArgument>, String> {
    read_call_args_with_value_policy_at_frame(unit, stack, frame_index, args, |_, _| false)
}

fn read_call_args_with_value_policy_at_frame(
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    args: &[IrCallArg],
    mut use_null_placeholder: impl FnMut(usize, &IrCallArg) -> bool,
) -> Result<Vec<CallArgument>, String> {
    let mut out = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        let _source = layout_source::enter(layout_source::CALL_ARGUMENT_SNAPSHOT);
        let value = if use_null_placeholder(index, arg) {
            Value::Null
        } else {
            read_operand_at_frame(unit, stack, frame_index, arg.value)?
        };
        if arg.unpack {
            if arg.name.is_some() {
                return Err(
                    "E_PHP_VM_NAMED_UNPACK_ARG: unpacked arguments cannot have an explicit name"
                        .to_owned(),
                );
            }
            let Value::Array(array) = value else {
                return Err(format!(
                    "E_PHP_VM_UNPACK_NON_ARRAY: cannot unpack {} as call arguments",
                    value_type_name(&value)
                ));
            };
            for (key, value) in array.iter() {
                let name = match key {
                    ArrayKey::Int(_) => None,
                    ArrayKey::String(key) => Some(key.to_string()),
                };
                out.push(CallArgument {
                    name,
                    value: value.clone(),
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                });
            }
            continue;
        }
        let by_ref_dim = arg
            .by_ref_dim
            .as_ref()
            .map(|target| {
                read_dim_operands_at_frame(unit, stack, frame_index, &target.dims).map(|dims| {
                    CallDimTarget {
                        local: target.local,
                        dims,
                    }
                })
            })
            .transpose()?;
        let by_ref_property = arg
            .by_ref_property
            .as_ref()
            .map(
                |target| match read_operand_at_frame(unit, stack, frame_index, target.object)? {
                    Value::Object(object) => Ok(CallPropertyTarget {
                        object,
                        property: target.property.clone(),
                    }),
                    other => Err(format!(
                        "E_PHP_VM_BY_REF_PROPERTY_NON_OBJECT: cannot bind property ${} on {}",
                        target.property,
                        value_type_name(&other)
                    )),
                },
            )
            .transpose()?;
        let by_ref_property_dim = arg
            .by_ref_property_dim
            .as_ref()
            .map(
                |target| match read_operand_at_frame(unit, stack, frame_index, target.object)? {
                    Value::Object(object) => {
                        let dims =
                            read_dim_operands_at_frame(unit, stack, frame_index, &target.dims)?;
                        Ok(CallPropertyDimTarget {
                            object,
                            property: target.property.clone(),
                            dims,
                        })
                    }
                    other => Err(format!(
                        "E_PHP_VM_BY_REF_PROPERTY_DIM_NON_OBJECT: cannot bind property dimension ${} on {}",
                        target.property,
                        value_type_name(&other)
                    )),
                },
            )
            .transpose()?;
        out.push(CallArgument {
            name: arg.name.clone(),
            value,
            value_kind: arg.value_kind,
            by_ref_local: arg.by_ref_local,
            by_ref_dim,
            by_ref_property,
            by_ref_property_dim,
        });
    }
    Ok(out)
}

fn is_quiet_by_ref_internal_builtin_arg(function: &str, index: usize, arg: &IrCallArg) -> bool {
    if arg.by_ref_local.is_none() || arg.unpack {
        return false;
    }

    let function = normalize_function_name(function);
    if generated_internal_builtin_param_is_by_ref(&function, index, arg.name.as_deref()) {
        return true;
    }

    match function.as_str() {
        "apcu_fetch" => index == 1 || arg.name.as_deref() == Some("success"),
        "apcu_dec" | "apcu_inc" => index == 2 || arg.name.as_deref() == Some("success"),
        "exif_thumbnail" => {
            (1..=3).contains(&index)
                || matches!(arg.name.as_deref(), Some("width" | "height" | "image_type"))
        }
        "getimagesize" => index == 1 || arg.name.as_deref() == Some("image_info"),
        "is_callable" => index == 2 || arg.name.as_deref() == Some("callable_name"),
        "msg_send" => index == 5 || arg.name.as_deref() == Some("error_code"),
        "msg_receive" => {
            index == 2
                || index == 4
                || index == 7
                || matches!(
                    arg.name.as_deref(),
                    Some("received_message_type" | "message" | "error_code")
                )
        }
        "openssl_random_pseudo_bytes" => index == 1 || arg.name.as_deref() == Some("strong_result"),
        "pcntl_wait" => index == 0 || arg.name.as_deref() == Some("status"),
        "pcntl_waitpid" => {
            index == 1
                || arg.name.as_deref() == Some("status")
                || index == 3
                || arg.name.as_deref() == Some("resource_usage")
        }
        "preg_match" | "preg_match_all" => index == 2 || arg.name.as_deref() == Some("matches"),
        "preg_filter" | "preg_replace" | "preg_replace_callback" => {
            index == 4 || arg.name.as_deref() == Some("count")
        }
        "preg_replace_callback_array" => index == 3 || arg.name.as_deref() == Some("count"),
        "socket_getpeername" | "socket_getsockname" => {
            index == 1 || index == 2 || matches!(arg.name.as_deref(), Some("address" | "port"))
        }
        "socket_recv" => index == 1 || arg.name.as_deref() == Some("data"),
        _ => false,
    }
}

pub(super) fn is_quiet_dense_by_ref_internal_builtin_arg(
    dense: &DenseBytecodeUnit,
    function: &str,
    index: usize,
    arg: &DenseCallArg,
) -> bool {
    if arg.by_ref_local.is_none() {
        return false;
    }
    let arg_name = arg
        .name
        .and_then(|name| dense.names.get(name as usize).map(String::as_str));

    let function = normalize_function_name(function);
    if generated_internal_builtin_param_is_by_ref(&function, index, arg_name) {
        return true;
    }

    match function.as_str() {
        "apcu_dec" | "apcu_inc" => index == 2 || arg_name == Some("success"),
        "exif_thumbnail" => {
            (1..=3).contains(&index) || matches!(arg_name, Some("width" | "height" | "image_type"))
        }
        "getimagesize" => index == 1 || arg_name == Some("image_info"),
        "is_callable" => index == 2 || arg_name == Some("callable_name"),
        "msg_send" => index == 5 || arg_name == Some("error_code"),
        "msg_receive" => {
            index == 2
                || index == 4
                || index == 7
                || matches!(
                    arg_name,
                    Some("received_message_type" | "message" | "error_code")
                )
        }
        "openssl_random_pseudo_bytes" => index == 1 || arg_name == Some("strong_result"),
        "pcntl_wait" => index == 0 || arg_name == Some("status"),
        "pcntl_waitpid" => {
            index == 1
                || arg_name == Some("status")
                || index == 3
                || arg_name == Some("resource_usage")
        }
        "preg_match" | "preg_match_all" => index == 2 || arg_name == Some("matches"),
        "preg_filter" | "preg_replace" | "preg_replace_callback" => {
            index == 4 || arg_name == Some("count")
        }
        "preg_replace_callback_array" => index == 3 || arg_name == Some("count"),
        "socket_getpeername" | "socket_getsockname" => {
            index == 1 || index == 2 || matches!(arg_name, Some("address" | "port"))
        }
        "socket_recv" => index == 1 || arg_name == Some("data"),
        _ => false,
    }
}

fn generated_internal_builtin_param_is_by_ref(
    function: &str,
    index: usize,
    arg_name: Option<&str>,
) -> bool {
    let Some(metadata) = php_std::arginfo::function_metadata_indexed(function) else {
        return false;
    };
    let param = if let Some(name) = arg_name {
        metadata.params.iter().find(|param| param.name == name)
    } else {
        metadata.params.get(index)
    };
    param.is_some_and(|param| param.by_ref)
}

pub(super) fn iterator_function_temporary_arg_value(
    function: &str,
    ir_args: &[IrCallArg],
    args: &[CallArgument],
) -> Option<(Operand, Value)> {
    if !iterator_function_releases_temporary_arg(function) {
        return None;
    }
    let ir_arg = ir_args.first()?;
    let arg = args.first()?;
    (arg.value_kind == IrCallArgValueKind::IndirectTemporary)
        .then(|| (ir_arg.value, arg.value.clone()))
}

pub(super) fn callable_iterator_function_temporary_arg_value(
    callee: &Value,
    ir_args: &[IrCallArg],
    args: &[CallArgument],
) -> Option<(Operand, Value)> {
    let function = match effective_value(callee) {
        Value::String(name) => String::from_utf8_lossy(name.as_bytes()).into_owned(),
        Value::Callable(callable) => match callable.as_ref() {
            CallableValue::UserFunction { name } | CallableValue::InternalBuiltin { name } => {
                name.clone()
            }
            CallableValue::Closure(_)
            | CallableValue::BoundMethod { .. }
            | CallableValue::MethodPlaceholder { .. }
            | CallableValue::UnresolvedDynamic { .. } => return None,
        },
        _ => return None,
    };
    iterator_function_temporary_arg_value(&function, ir_args, args)
}

fn iterator_function_releases_temporary_arg(function: &str) -> bool {
    matches!(
        normalize_function_name(function).as_str(),
        "iterator_apply" | "iterator_count" | "iterator_to_array"
    )
}

pub(super) fn unset_indirect_temporary_call_arg_registers_at_frame(
    stack: &mut CallStack,
    frame_index: usize,
    args: &[IrCallArg],
) -> Result<(), String> {
    for arg in args {
        if arg.value_kind == IrCallArgValueKind::IndirectTemporary {
            unset_register_operand_at_frame(stack, frame_index, arg.value)?;
        }
    }
    Ok(())
}

pub(super) fn take_method_call_temporary_registers_at_frame(
    stack: &mut CallStack,
    frame_index: usize,
    object: Operand,
    args: &[IrCallArg],
) -> Result<Vec<Value>, String> {
    let mut values = Vec::new();
    let mut taken = HashSet::new();
    if let Operand::Register(id) = object {
        let frame = stack.frame_mut(frame_index).ok_or("no active frame")?;
        let value = frame.registers.take(id)?;
        if !value.is_uninitialized() {
            values.push(value);
        }
        taken.insert(id);
    }
    for arg in args {
        let Operand::Register(id) = arg.value else {
            continue;
        };
        if !taken.insert(id) {
            continue;
        }
        let frame = stack.frame_mut(frame_index).ok_or("no active frame")?;
        let value = frame.registers.take(id)?;
        if !value.is_uninitialized() {
            values.push(value);
        }
    }
    Ok(values)
}

pub(super) fn unset_consumed_call_arg_registers_at_frame(
    stack: &mut CallStack,
    frame_index: usize,
    args: &[IrCallArg],
    preserved: Option<RegId>,
) -> Result<(), String> {
    for arg in args {
        if arg.by_ref_local.is_none()
            && arg.by_ref_dim.is_none()
            && arg.by_ref_property.is_none()
            && arg.by_ref_property_dim.is_none()
        {
            continue;
        }
        let Operand::Register(id) = arg.value else {
            continue;
        };
        if Some(id) == preserved {
            continue;
        }
        unset_register_operand_at_frame(stack, frame_index, arg.value)?;
    }
    Ok(())
}

pub(super) fn unset_consumed_dense_call_arg_registers_at_frame(
    stack: &mut CallStack,
    frame_index: usize,
    args: &[DenseCallArg],
    preserved: Option<RegId>,
) -> Result<(), String> {
    for arg in args {
        if arg.by_ref_local.is_none()
            && arg.by_ref_dim.is_none()
            && arg.by_ref_property.is_none()
            && arg.by_ref_property_dim.is_none()
        {
            continue;
        }
        if arg.value.kind != DenseOperandKind::Register {
            continue;
        }
        let id = RegId::new(arg.value.index);
        if Some(id) == preserved {
            continue;
        }
        let frame = stack.frame_mut(frame_index).ok_or("no active frame")?;
        frame.registers.unset(id)?;
    }
    Ok(())
}

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

/// Determines whether a value is callable, matching PHP's `is_callable`. When
/// `syntax_only` is set, only the structural shape (string, or `[target, name]`
/// array) is checked, not whether the target actually exists.
pub(super) fn value_is_callable(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
    syntax_only: bool,
) -> bool {
    match effective_value(value) {
        // Closures, first-class callables, and resolved function/builtin callables.
        Value::Callable(_) => true,
        Value::Object(object) => {
            lookup_method_in_state(compiled, state, &object.class_name(), "__invoke")
                .map(|method| method.is_some())
                .unwrap_or(false)
        }
        Value::String(name) => {
            let name = name.to_string_lossy();
            if syntax_only {
                return true;
            }
            if let Some((class, method)) = name.split_once("::") {
                lookup_method_in_state(compiled, state, class, method)
                    .map(|method| method.is_some_and(|method| method.flags.is_static))
                    .unwrap_or(false)
            } else {
                let normalized = normalize_function_name(&name);
                compiled.lookup_function(&normalized).is_some()
                    || dynamic_function_in_state(state, &normalized).is_some()
                    || BuiltinRegistry::new().contains(&normalized)
            }
        }
        Value::Array(array) => {
            let (Some(target), Some(method)) =
                (array.get(&ArrayKey::Int(0)), array.get(&ArrayKey::Int(1)))
            else {
                return false;
            };
            let Ok(method) = to_string(method) else {
                return false;
            };
            let Ok(class) = class_name_from_object_or_string(target) else {
                return false;
            };
            if syntax_only {
                return true;
            }
            let method = method.to_string_lossy();
            match effective_value(target) {
                Value::Object(_) => {
                    lookup_method_in_state(compiled, state, &class, &method)
                        .map(|method| method.is_some())
                        .unwrap_or(false)
                        || class_has_public_magic_call_in_state(compiled, state, &class)
                            .unwrap_or(false)
                }
                _ => {
                    lookup_method_in_state(compiled, state, &class, &method)
                        .map(|method| method.is_some_and(|method| method.flags.is_static))
                        .unwrap_or(false)
                        || class_has_public_magic_call_static_in_state(compiled, state, &class)
                            .unwrap_or(false)
                }
            }
        }
        _ => false,
    }
}

pub(super) fn callable_name_for_is_callable(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
) -> Option<String> {
    match effective_value(value) {
        Value::Callable(callable) => match *callable {
            CallableValue::UserFunction { name } => Some(name),
            CallableValue::InternalBuiltin { name } => Some(name),
            CallableValue::Closure(payload) => {
                let function = compiled.unit().functions.get(payload.function as usize)?;
                payload
                    .debug
                    .map(|debug| debug.name)
                    .or_else(|| Some(function.name.clone()))
            }
            CallableValue::MethodPlaceholder { target }
            | CallableValue::UnresolvedDynamic { target } => Some(target),
            CallableValue::BoundMethod { target, method, .. } => {
                let class_name = match target {
                    CallableMethodTarget::Object(object) => {
                        callable_class_display_name(compiled, state, &object.class_name())
                    }
                    CallableMethodTarget::Class(class_name) => {
                        callable_class_display_name(compiled, state, &class_name)
                    }
                };
                Some(format!("{class_name}::{method}"))
            }
        },
        Value::Object(object) => Some(format!(
            "{}::__invoke",
            callable_class_display_name(compiled, state, &object.class_name())
        )),
        Value::String(name) => Some(name.to_string_lossy()),
        Value::Array(array) => callable_array_name_for_is_callable(compiled, state, &array),
        _ => None,
    }
}

fn callable_array_name_for_is_callable(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    array: &PhpArray,
) -> Option<String> {
    let target = array.get(&ArrayKey::Int(0))?;
    let method = array.get(&ArrayKey::Int(1))?;
    let method = to_string(method).ok()?.to_string_lossy();
    match effective_value(target) {
        Value::Object(object) => Some(format!(
            "{}::{method}",
            callable_class_display_name(compiled, state, &object.class_name())
        )),
        Value::String(class_name) => {
            let class_name = class_name.to_string_lossy();
            Some(format!(
                "{}::{method}",
                callable_class_display_name(compiled, state, &class_name)
            ))
        }
        _ => None,
    }
}

pub(super) fn autoload_callback_public_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    callback: &CallableValue,
) -> Value {
    match callback {
        CallableValue::UserFunction { name } => {
            Value::string(user_function_display_name(compiled, state, name))
        }
        CallableValue::InternalBuiltin { name } => Value::string(name.clone()),
        CallableValue::Closure(_) => Value::Callable(Box::new(callback.clone())),
        CallableValue::BoundMethod { target, method, .. } => {
            if method.eq_ignore_ascii_case("__invoke")
                && let CallableMethodTarget::Object(object) = target
            {
                return Value::Object(object.clone());
            }
            let target = match target {
                CallableMethodTarget::Object(object) => Value::Object(object.clone()),
                CallableMethodTarget::Class(class_name) => {
                    Value::string(callable_class_display_name(compiled, state, class_name))
                }
            };
            Value::Array(PhpArray::from_packed(vec![
                target,
                Value::string(method.clone()),
            ]))
        }
        CallableValue::MethodPlaceholder { target }
        | CallableValue::UnresolvedDynamic { target } => Value::string(target.clone()),
    }
}

fn user_function_display_name(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    normalized_name: &str,
) -> String {
    if let Some(function) = compiled.lookup_function(normalized_name)
        && let Some(function) = compiled.unit().functions.get(function.index())
    {
        return function.name.clone();
    }
    if let Some((owner, function)) = dynamic_function_in_state(state, normalized_name)
        && let Some(function) = owner.unit().functions.get(function.index())
    {
        return function.name.clone();
    }
    normalized_name.to_owned()
}

pub(super) fn callable_class_display_name(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> String {
    lookup_class_in_state(compiled, state, class_name)
        .map(|class| class.display_name.clone())
        .or_else(|| {
            php_std::ExtensionRegistry::standard_library()
                .enabled_class(class_name)
                .map(|class| class.name().to_owned())
        })
        .unwrap_or_else(|| class_name.to_owned())
}

pub(super) fn acquire_callable_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    value: Value,
) -> Result<Value, String> {
    let value = effective_value(&value);
    match value {
        Value::Callable(_) => Ok(value),
        Value::String(name) => {
            let name_text = name.to_string_lossy();
            validate_string_callable_acquisition(compiled, state, stack, &name_text)?;
            Ok(Value::String(name))
        }
        Value::Array(array) => acquire_array_callable(compiled, state, stack, &array),
        Value::Object(object) => {
            validate_object_method_callable_acquisition(
                compiled,
                state,
                stack,
                &object.class_name(),
                "__invoke",
                false,
            )?;
            Ok(Value::Object(object))
        }
        other => Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_NOT_CALLABLE: Value of type {} is not callable",
            value_type_name(&other)
        )),
    }
}

fn validate_string_callable_acquisition(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    name: &str,
) -> Result<(), String> {
    if let Some((class_name, method)) = name.split_once("::") {
        return validate_static_method_callable_acquisition(
            compiled, state, stack, class_name, method, true,
        );
    }
    let normalized = normalize_function_name(name);
    if function_callable_exists(compiled, state, &normalized) {
        Ok(())
    } else {
        Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_FUNCTION: Call to undefined function {name}()"
        ))
    }
}

fn acquire_array_callable(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    array: &PhpArray,
) -> Result<Value, String> {
    let (Some(target), Some(method)) = (array.get(&ArrayKey::Int(0)), array.get(&ArrayKey::Int(1)))
    else {
        return Err(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_NOT_CALLABLE: Value of type array is not callable"
                .to_owned(),
        );
    };
    if array.len() != 2 {
        return Err(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_NOT_CALLABLE: Value of type array is not callable"
                .to_owned(),
        );
    }
    let method = to_string(method)
        .map_err(|_| {
            "E_PHP_VM_FIRST_CLASS_CALLABLE_NOT_CALLABLE: Value of type array is not callable"
                .to_owned()
        })?
        .to_string_lossy();
    match effective_value(target) {
        Value::Object(object) => {
            validate_object_method_callable_acquisition(
                compiled,
                state,
                stack,
                &object.class_name(),
                &method,
                true,
            )?;
            Ok(Value::bound_method_callable(
                CallableMethodTarget::Object(object),
                method,
                current_scope_class(compiled, stack),
            ))
        }
        Value::Callable(callable) if method.eq_ignore_ascii_case("__invoke") => {
            Ok(Value::Callable(callable))
        }
        Value::String(class_name) => {
            let class_name = class_name.to_string_lossy();
            validate_static_method_callable_acquisition(
                compiled,
                state,
                stack,
                &class_name,
                &method,
                true,
            )?;
            Ok(Value::bound_method_callable(
                CallableMethodTarget::Class(class_name),
                method,
                current_scope_class(compiled, stack),
            ))
        }
        other => Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_NOT_CALLABLE: Value of type {} is not callable",
            value_type_name(&other)
        )),
    }
}

pub(super) fn validate_object_method_callable_acquisition(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    class_name: &str,
    method: &str,
    allow_magic_call: bool,
) -> Result<(), String> {
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        let class_name = callable_class_display_name(compiled, state, class_name);
        return Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_METHOD: Call to undefined method {class_name}::{method}()"
        ));
    };
    let scope = current_scope_class(compiled, stack);
    let resolved =
        lookup_resolved_method_in_state(compiled, state, class_name, method, scope.as_deref())?;
    let Some(resolved) = resolved else {
        if allow_magic_call && class_has_public_magic_call_in_state(compiled, state, class_name)? {
            return Ok(());
        }
        return Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_METHOD: Call to undefined method {}::{method}()",
            class.display_name
        ));
    };
    validate_method_callable_for_acquisition_in_state(
        compiled,
        state,
        stack,
        &resolved.class,
        &resolved.method,
        method,
    )
}

fn validate_static_method_callable_acquisition(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    class_name: &str,
    method: &str,
    allow_magic_call_static: bool,
) -> Result<(), String> {
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        let class_name = callable_class_display_name(compiled, state, class_name);
        return Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_METHOD: Call to undefined method {class_name}::{method}()"
        ));
    };
    let scope = current_scope_class(compiled, stack);
    let resolved =
        lookup_resolved_method_in_state(compiled, state, class_name, method, scope.as_deref())?;
    let Some(resolved) = resolved else {
        if allow_magic_call_static
            && class_has_public_magic_call_static_in_state(compiled, state, class_name)?
        {
            return Ok(());
        }
        return Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_METHOD: Call to undefined method {}::{method}()",
            class.display_name
        ));
    };
    if !resolved.method.flags.is_static {
        return Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_NON_STATIC_METHOD: Non-static method {}::{}() cannot be called statically",
            resolved.class.display_name, method
        ));
    }
    validate_method_callable_for_acquisition_in_state(
        compiled,
        state,
        stack,
        &resolved.class,
        &resolved.method,
        method,
    )
}

pub(super) fn class_has_public_magic_call_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Result<bool, String> {
    Ok(
        lookup_resolved_method_in_state(compiled, state, class_name, "__call", None)?.is_some_and(
            |resolved| {
                !resolved.method.flags.is_static
                    && !resolved.method.flags.is_private
                    && !resolved.method.flags.is_protected
            },
        ),
    )
}

pub(super) fn class_has_public_magic_call_static_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Result<bool, String> {
    Ok(
        lookup_resolved_method_in_state(compiled, state, class_name, "__callStatic", None)?
            .is_some_and(|resolved| {
                resolved.method.flags.is_static
                    && !resolved.method.flags.is_private
                    && !resolved.method.flags.is_protected
            }),
    )
}

fn validate_method_callable_for_acquisition_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    class: &php_ir::module::ClassEntry,
    method: &php_ir::module::ClassMethodEntry,
    display_method: &str,
) -> Result<(), String> {
    if method.flags.is_abstract {
        return Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_METHOD: Cannot call abstract method {}::{display_method}()",
            class.display_name
        ));
    }
    if method.flags.is_private {
        let scope = current_scope_class(compiled, stack);
        if scope.as_deref() != Some(normalize_class_name(&class.name).as_str()) {
            return Err(format!(
                "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_METHOD: Call to private method {}::{display_method}() from {}",
                class.display_name,
                scope_description(scope.as_deref())
            ));
        }
    }
    if method.flags.is_protected {
        let scope = current_scope_class(compiled, stack);
        let allowed = scope.as_deref().is_some_and(|scope| {
            class_is_a_in_state(compiled, state, scope, &class.name).unwrap_or(false)
        });
        if !allowed {
            return Err(format!(
                "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_METHOD: Call to protected method {}::{display_method}() from {}",
                class.display_name,
                scope_description(scope.as_deref())
            ));
        }
    }
    Ok(())
}

fn function_callable_exists(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    normalized_name: &str,
) -> bool {
    compiled.lookup_function(normalized_name).is_some()
        || dynamic_function_in_state(state, normalized_name).is_some()
        || is_callable_builtin_name(normalized_name)
}

pub(super) fn is_callable_builtin_name(name: &str) -> bool {
    is_supported_builtin(name)
        || is_autoload_builtin_name(name)
        || is_symbol_introspection_builtin_name(name)
        || is_config_builtin_name(name)
        || is_error_handling_builtin_name(name)
        || is_output_buffering_builtin_name(name)
        || is_environment_builtin_name(name)
        || is_process_builtin_name(name)
        || is_filter_callback_builtin_name(name)
        || is_pcre_callback_builtin_name(name)
        || is_array_callback_builtin_name(name)
        || is_array_sort_builtin_name(name)
}

pub(super) fn resolve_callable(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    callable: &CallableKind,
) -> Result<Value, String> {
    match callable {
        CallableKind::FunctionName { name } => {
            if compiled.lookup_function(name).is_some()
                || dynamic_function_in_state(state, name).is_some()
            {
                Ok(Value::user_function_callable(name.clone()))
            } else if is_callable_builtin_name(name) {
                Ok(Value::internal_builtin_callable(name.clone()))
            } else {
                Err(format!(
                    "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_FUNCTION: Call to undefined function {name}()"
                ))
            }
        }
        CallableKind::MethodPlaceholder { target } => {
            Ok(Value::method_callable_placeholder(target.clone()))
        }
        CallableKind::UnresolvedDynamic { target } => Err(format!(
            "E_PHP_VM_FIRST_CLASS_CALLABLE_UNRESOLVED_DYNAMIC: callable {target} could not be resolved"
        )),
    }
}

fn is_supported_builtin(name: &str) -> bool {
    BuiltinRegistry::new().contains(name)
}

pub(super) fn value_is_numeric_string_key_ambiguity(value: &Value) -> bool {
    let Value::String(string) = effective_value(value) else {
        return false;
    };
    php_runtime::numeric_string::array_key_has_numeric_string_ambiguity(&string)
}

pub(super) fn value_needs_vm_string_coercion_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
) -> bool {
    match value {
        Value::Object(object) => object_has_public_to_string_in_state(compiled, state, object),
        Value::Reference(cell) => {
            value_needs_vm_string_coercion_in_state(compiled, state, &cell.get())
        }
        _ => false,
    }
}

pub(super) fn internal_builtin_string_arg_positions(name: &str, arity: usize) -> &'static [usize] {
    match name {
        "addslashes"
        | "bin2hex"
        | "crc32"
        | "hex2bin"
        | "htmlentities"
        | "htmlspecialchars"
        | "htmlspecialchars_decode"
        | "lcfirst"
        | "ltrim"
        | "md5"
        | "ord"
        | "pack"
        | "rawurldecode"
        | "rawurlencode"
        | "rtrim"
        | "sha1"
        | "str_split"
        | "stripslashes"
        | "stripcslashes"
        | "strlen"
        | "strrev"
        | "strtolower"
        | "strtoupper"
        | "strval"
        | "trim"
        | "ucfirst"
        | "urlencode"
        | "urldecode" => &[0],
        "explode" | "str_contains" | "str_ends_with" | "str_starts_with" | "strstr" | "stristr"
        | "strpbrk" | "strpos" | "stripos" | "strrchr" | "strrpos" | "strripos"
        | "substr_count" | "strcmp" | "strcasecmp" | "strnatcmp" | "strnatcasecmp"
        | "version_compare" => &[0, 1],
        "str_repeat" | "str_pad" | "substr" | "substr_compare" | "strncmp" | "strncasecmp"
        | "strspn" | "strcspn" => &[0, 1],
        "strtr" if arity == 2 => &[0],
        "str_replace" | "strtr" | "substr_replace" => &[0, 1, 2],
        "print" | "printf" | "sprintf" | "vprintf" | "vsprintf" => &[0],
        "fprintf" | "vfprintf" => &[1],
        "ucwords" => &[0, 1],
        "unpack" => &[0, 1],
        "preg_grep" | "preg_match" | "preg_match_all" | "preg_split" => &[0, 1],
        "preg_quote" => &[0, 1],
        _ => &[],
    }
}

pub(super) fn internal_function_dispatch_cacheable(name: &str) -> bool {
    name == "count"
        || name == "strlen"
        || name.starts_with("is_")
        || matches!(
            name,
            "array_chunk"
                | "array_column"
                | "array_combine"
                | "array_count_values"
                | "array_fill"
                | "array_flip"
                | "array_is_list"
                | "array_key_exists"
                | "array_keys"
                | "array_merge"
                | "array_pad"
                | "array_pop"
                | "array_push"
                | "array_reverse"
                | "array_search"
                | "array_shift"
                | "array_slice"
                | "array_splice"
                | "array_sum"
                | "array_unshift"
                | "array_values"
                | "implode"
                | "join"
                | "lcfirst"
                | "ltrim"
                | "rtrim"
                | "str_contains"
                | "str_ends_with"
                | "str_repeat"
                | "str_replace"
                | "str_split"
                | "str_starts_with"
                | "strtolower"
                | "strtoupper"
                | "substr"
                | "trim"
        )
}
