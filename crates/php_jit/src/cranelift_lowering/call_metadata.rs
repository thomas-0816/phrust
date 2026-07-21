use super::*;

pub(super) fn native_call_target_metadata(target: &RegionCallTarget) -> (u32, u32, u64, u64) {
    match target {
        RegionCallTarget::Function { name, function } => (
            crate::JitNativeCallKind::FUNCTION.0,
            function.map_or(u32::MAX, FunctionId::raw),
            stable_call_symbol_hash(name),
            0,
        ),
        RegionCallTarget::Method { method, .. } => (
            crate::JitNativeCallKind::METHOD.0,
            u32::MAX,
            stable_call_symbol_hash(method),
            0,
        ),
        RegionCallTarget::StaticMethod { class_name, method } => (
            crate::JitNativeCallKind::STATIC_METHOD.0,
            u32::MAX,
            stable_call_symbol_hash(method),
            stable_call_symbol_hash(class_name),
        ),
        RegionCallTarget::Closure { .. } => (crate::JitNativeCallKind::CLOSURE.0, u32::MAX, 0, 0),
        RegionCallTarget::Callable { .. } => (crate::JitNativeCallKind::CALLABLE.0, u32::MAX, 0, 0),
        RegionCallTarget::Pipe { .. } => (crate::JitNativeCallKind::PIPE.0, u32::MAX, 0, 0),
        RegionCallTarget::Constructor { class_name, .. } => (
            crate::JitNativeCallKind::CONSTRUCTOR.0,
            u32::MAX,
            0,
            stable_call_symbol_hash(class_name),
        ),
        RegionCallTarget::DynamicConstructor { .. } => (
            crate::JitNativeCallKind::DYNAMIC_CONSTRUCTOR.0,
            u32::MAX,
            0,
            0,
        ),
        RegionCallTarget::Semantic { operation } => (
            crate::JitNativeCallKind::SEMANTIC_OPERATION.0,
            operation.operation_id().raw(),
            0,
            0,
        ),
    }
}

pub(super) fn stable_call_symbol_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub(super) fn stable_builtin_helper_id(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function {
        name,
        function: None,
    } = target
    else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    php_runtime::api::BuiltinRegistry::new()
        .get(&normalized)
        .map(php_runtime::api::BuiltinEntry::helper_id)
        .filter(|helper_id| *helper_id != 0)
}

pub(super) fn stable_builtin_dense_id(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function {
        name,
        function: None,
    } = target
    else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    php_runtime::api::BuiltinRegistry::new()
        .get(&normalized)
        .map(php_runtime::api::BuiltinEntry::dense_id)
}

pub(super) fn stable_builtin_type_predicate(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function {
        name,
        function: None,
    } = target
    else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    match normalized.as_str() {
        "is_null" => Some(0),
        "is_bool" => Some(1),
        "is_int" | "is_integer" | "is_long" => Some(2),
        "is_float" | "is_double" | "is_real" => Some(3),
        "is_string" => Some(4),
        "is_array" => Some(5),
        "is_object" => Some(6),
        "is_resource" => Some(7),
        "is_scalar" => Some(8),
        _ => None,
    }
}

pub(super) fn stable_builtin_length(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function {
        name,
        function: None,
    } = target
    else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    match normalized.as_str() {
        "strlen" => Some(0),
        "count" => Some(1),
        _ => None,
    }
}

pub(super) fn stable_builtin_array_key_exists(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function {
        name,
        function: None,
    } = target
    else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\')
        && (normalized.eq_ignore_ascii_case("array_key_exists")
            || normalized.eq_ignore_ascii_case("key_exists"))
}

pub(super) fn stable_builtin_string_predicate(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function {
        name,
        function: None,
    } = target
    else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "str_contains" => Some(0),
        "str_starts_with" => Some(1),
        "str_ends_with" => Some(2),
        _ => None,
    }
}

/// ASCII-only case conversion builtins whose PHP 8 semantics can be emitted
/// directly over the request-owned native string arena.  The numeric value is
/// an internal lowering selector, never a runtime helper operation ID.
pub(super) fn stable_builtin_ascii_case(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function {
        name,
        function: None,
    } = target
    else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strtolower" => Some(0),
        "strtoupper" => Some(1),
        _ => None,
    }
}

pub(super) fn native_argument_flags(argument: &php_ir::instruction::IrCallArg) -> u32 {
    let mut flags = crate::JitNativeArgFlags::default();
    if argument.name.is_some() {
        flags = flags.union(crate::JitNativeArgFlags::NAMED);
    }
    if argument.unpack {
        flags = flags.union(crate::JitNativeArgFlags::UNPACK);
    }
    if argument.by_ref_local.is_some()
        || argument.by_ref_dim.is_some()
        || argument.by_ref_property.is_some()
        || argument.by_ref_property_dim.is_some()
    {
        flags = flags.union(crate::JitNativeArgFlags::BY_REFERENCE);
    }
    if argument.value_kind == php_ir::instruction::IrCallArgValueKind::IndirectTemporary {
        flags = flags.union(crate::JitNativeArgFlags::INDIRECT_TEMPORARY);
    }
    flags.0
}

pub(super) fn native_argument_has_location(argument: &php_ir::instruction::IrCallArg) -> bool {
    argument.by_ref_local.is_some()
        || argument.by_ref_dim.is_some()
        || argument.by_ref_property.is_some()
        || argument.by_ref_property_dim.is_some()
}

pub(super) fn known_user_argument_requires_reference(
    call: &RegionNativeCall,
    index: usize,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    caller: FunctionId,
) -> Option<bool> {
    let argument = call.args.get(index)?;
    if let RegionCallTarget::Function {
        name,
        function: None,
    } = &call.target
    {
        let normalized = name.trim_start_matches('\\');
        let has_local_metadata = function_params.values().any(|(candidate, ..)| {
            candidate
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(normalized)
        });
        if !has_local_metadata {
            let signature = external_function_signatures.iter().find(|signature| {
                signature
                    .name
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(normalized)
            })?;
            let parameter = argument.name.as_deref().map_or_else(
                || {
                    signature.params.get(index).or_else(|| {
                        signature
                            .params
                            .last()
                            .filter(|parameter| parameter.variadic)
                    })
                },
                |name| {
                    signature
                        .params
                        .iter()
                        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
                        .or_else(|| {
                            signature
                                .params
                                .last()
                                .filter(|parameter| parameter.variadic)
                        })
                },
            );
            return Some(parameter.is_some_and(|parameter| parameter.by_ref));
        }
    }
    let method_matches = |candidate: &str, method: &str| {
        candidate
            .rsplit_once("::")
            .is_some_and(|(_, candidate_method)| candidate_method.eq_ignore_ascii_case(method))
    };
    let metadata = match &call.target {
        RegionCallTarget::Function {
            name,
            function: None,
        } => {
            let normalized = name.trim_start_matches('\\');
            vec![function_params.values().find(|(candidate, ..)| {
                candidate
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(normalized)
            })?]
        }
        RegionCallTarget::Function {
            function: Some(function),
            ..
        } => vec![function_params.get(function)?],
        RegionCallTarget::StaticMethod { class_name, method } => {
            let resolved_class = if matches!(class_name.as_str(), "self" | "static") {
                function_params
                    .get(&caller)
                    .and_then(|(name, ..)| name.rsplit_once("::").map(|(class, _)| class))
            } else {
                Some(class_name.trim_start_matches('\\'))
            };
            let exact = resolved_class.and_then(|class| {
                function_params.values().find(|(candidate, ..)| {
                    candidate.rsplit_once("::").is_some_and(
                        |(candidate_class, candidate_method)| {
                            candidate_class
                                .trim_start_matches('\\')
                                .eq_ignore_ascii_case(class)
                                && candidate_method.eq_ignore_ascii_case(method)
                        },
                    )
                })
            });
            exact.map_or_else(
                || {
                    function_params
                        .values()
                        .filter(|(candidate, ..)| method_matches(candidate, method))
                        .collect()
                },
                |metadata| vec![metadata],
            )
        }
        RegionCallTarget::Method { method, .. } => function_params
            .values()
            .filter(|(candidate, ..)| method_matches(candidate, method))
            .collect(),
        RegionCallTarget::Constructor { class_name, .. } => function_params
            .values()
            .filter(|(candidate, ..)| {
                candidate.rsplit_once("::").is_some_and(|(class, method)| {
                    class
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case(class_name.trim_start_matches('\\'))
                        && method.eq_ignore_ascii_case("__construct")
                })
            })
            .collect(),
        _ => return None,
    };
    let mut requirements = metadata.into_iter().map(|metadata| {
        let parameters = &metadata.1;
        argument
            .name
            .as_deref()
            .map_or_else(
                || {
                    parameters
                        .get(index)
                        .or_else(|| parameters.last().filter(|parameter| parameter.variadic))
                },
                |name| {
                    parameters
                        .iter()
                        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
                        .or_else(|| parameters.last().filter(|parameter| parameter.variadic))
                },
            )
            .is_some_and(|parameter| parameter.by_ref)
    });
    let requirement = requirements.next()?;
    requirements
        .all(|candidate| candidate == requirement)
        .then_some(requirement)
}
