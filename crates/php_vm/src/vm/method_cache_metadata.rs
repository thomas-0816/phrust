use super::prelude::*;

pub(super) fn normalize_method_name(method: &str) -> String {
    method.to_ascii_lowercase()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn method_call_profile_observation(
    callsite: &str,
    method: &str,
    receiver_class: &str,
    class: &php_ir::module::ClassEntry,
    resolved_method: Option<(
        &CompiledUnit,
        &php_ir::module::ClassEntry,
        &php_ir::module::ClassMethodEntry,
    )>,
    visibility_context: Option<&str>,
    layout_version: InvalidationEpoch,
    has_magic_call: bool,
    magic_call_fallback: bool,
    simple_positional_arguments: bool,
    has_by_ref_argument: bool,
    callee_jit_eligible: bool,
    direct_vm_call_helper_available: bool,
    non_eligible_reasons: Vec<&'static str>,
) -> MethodCallProfileObservation {
    let (
        declaring_class,
        method_id,
        method_slot_index,
        method_is_final,
        method_is_private,
        method_is_static,
    ) = resolved_method.map_or(
        (None, None, None, false, false, false),
        |(_owner, declaring_class, method_entry)| {
            (
                Some(declaring_class.name.clone()),
                Some(method_entry.function.raw()),
                declaring_class.methods.iter().position(|entry| {
                    entry.name == method_entry.name && entry.function == method_entry.function
                }),
                method_entry.flags.is_final,
                method_entry.flags.is_private,
                method_entry.flags.is_static,
            )
        },
    );
    MethodCallProfileObservation {
        callsite: callsite.to_owned(),
        method: method.to_owned(),
        receiver_class: receiver_class.to_owned(),
        class_id: class.id.raw(),
        declaring_class,
        method_id,
        method_slot_index,
        visibility_context: visibility_context.map(str::to_owned),
        override_layout_version: layout_version.raw(),
        method_is_final,
        method_is_private,
        method_is_static,
        has_magic_call,
        magic_call_fallback,
        simple_positional_arguments,
        has_by_ref_argument,
        callee_jit_eligible,
        direct_vm_call_helper_available,
        non_eligible_reasons,
    }
}

pub(super) fn method_call_args_are_simple_positional(args: &[IrCallArg]) -> bool {
    args.iter().all(|arg| arg.name.is_none() && !arg.unpack)
}

pub(super) fn method_call_has_by_ref_argument(
    _args: &[IrCallArg],
    owner: Option<&CompiledUnit>,
    function: Option<FunctionId>,
) -> bool {
    let Some((owner, function)) = owner.zip(function) else {
        return false;
    };
    owner
        .unit()
        .functions
        .get(function.index())
        .is_some_and(|callee| callee.params.iter().any(|param| param.by_ref))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn method_call_guard_metadata(
    args: &[CallArgument],
    receiver_class: &php_ir::module::ClassEntry,
    declaring_class: &php_ir::module::ClassEntry,
    method_entry: &php_ir::module::ClassMethodEntry,
    _visibility_context: Option<&str>,
    epoch: InvalidationEpoch,
    has_magic_call: bool,
    has_by_ref_argument: bool,
) -> MethodCallGuardMetadata {
    MethodCallGuardMetadata {
        receiver_class_id: receiver_class.id,
        class_layout_epoch: epoch.raw(),
        method_table_epoch: epoch.raw(),
        method_slot_index: declaring_class
            .methods
            .iter()
            .position(|entry| {
                entry.name == method_entry.name && entry.function == method_entry.function
            })
            .and_then(|index| index.try_into().ok()),
        method_is_final: method_entry.flags.is_final,
        method_is_private: method_entry.flags.is_private,
        method_is_static: method_entry.flags.is_static,
        receiver_has_override: receiver_class.name == declaring_class.name
            && receiver_class.parent.is_some(),
        argument_shape: method_call_shape(args),
        by_ref_compatible: !has_by_ref_argument,
        has_magic_call,
    }
}

pub(super) fn method_call_cache_target_owner<'a>(
    compiled: &'a CompiledUnit,
    state: &'a ExecutionState,
    target: &MethodCallCacheTarget,
) -> Option<&'a CompiledUnit> {
    match target {
        MethodCallCacheTarget::CurrentUnit { .. } => Some(compiled),
        MethodCallCacheTarget::DynamicUnit { unit_index, .. } => {
            state.dynamic_units.get(*unit_index)
        }
    }
}

pub(super) struct MethodDirectCallEligibility<'a> {
    pub(super) compiled: &'a CompiledUnit,
    pub(super) state: &'a ExecutionState,
    pub(super) target: &'a MethodCallCacheTarget,
    pub(super) class: &'a php_ir::module::ClassEntry,
    pub(super) args: &'a [IrCallArg],
    pub(super) values: &'a [CallArgument],
    pub(super) has_magic_call: bool,
    pub(super) epoch: InvalidationEpoch,
}

pub(super) fn method_direct_call_target_is_eligible(
    eligibility: MethodDirectCallEligibility<'_>,
) -> bool {
    let MethodDirectCallEligibility {
        compiled,
        state,
        target,
        class,
        args,
        values,
        has_magic_call,
        epoch,
    } = eligibility;
    let resolved = target.resolved_target();
    let guard = &resolved.guard;
    if target.receiver_class_id() != class.id {
        return false;
    }
    if guard.class_layout_epoch != epoch.raw() || guard.method_table_epoch != epoch.raw() {
        return false;
    }
    if guard.has_magic_call != has_magic_call {
        return false;
    }
    if guard.argument_shape != method_call_shape(values) {
        return false;
    }
    if !method_call_args_are_simple_positional(args) {
        return false;
    }
    let owner = method_call_cache_target_owner(compiled, state, target);
    let Some(owner) = owner else {
        return false;
    };
    if method_call_has_by_ref_argument(args, Some(owner), Some(target.function())) {
        return false;
    }
    let Some(declaring_class) = owner.lookup_class(&resolved.declaring_class) else {
        return false;
    };
    let Some(method_entry) = declaring_class
        .methods
        .iter()
        .find(|method| method.function == resolved.function)
    else {
        return false;
    };
    if guard.method_slot_index
        != declaring_class
            .methods
            .iter()
            .position(|entry| {
                entry.name == method_entry.name && entry.function == method_entry.function
            })
            .and_then(|index| index.try_into().ok())
    {
        return false;
    }
    guard.method_is_final == method_entry.flags.is_final
        && guard.method_is_private == method_entry.flags.is_private
        && guard.method_is_static == method_entry.flags.is_static
        && guard.by_ref_compatible
}

pub(super) struct DenseMethodDirectCallEligibility<'a> {
    pub(super) compiled: &'a CompiledUnit,
    pub(super) state: &'a ExecutionState,
    pub(super) target: &'a MethodCallCacheTarget,
    pub(super) class: &'a php_ir::module::ClassEntry,
    pub(super) values: &'a [CallArgument],
    pub(super) has_magic_call: bool,
    pub(super) epoch: InvalidationEpoch,
}

pub(super) fn dense_method_direct_call_target_is_eligible(
    eligibility: DenseMethodDirectCallEligibility<'_>,
) -> bool {
    let DenseMethodDirectCallEligibility {
        compiled,
        state,
        target,
        class,
        values,
        has_magic_call,
        epoch,
    } = eligibility;
    let resolved = target.resolved_target();
    let guard = &resolved.guard;
    if target.receiver_class_id() != class.id
        || guard.class_layout_epoch != epoch.raw()
        || guard.method_table_epoch != epoch.raw()
        || guard.has_magic_call != has_magic_call
        || guard.argument_shape != method_call_shape(values)
        || values.iter().any(|arg| arg.name.is_some())
        || !guard.by_ref_compatible
    {
        return false;
    }
    let Some(owner) = method_call_cache_target_owner(compiled, state, target) else {
        return false;
    };
    let Some(declaring_class) = owner.lookup_class(&resolved.declaring_class) else {
        return false;
    };
    let Some(method_entry) = declaring_class
        .methods
        .iter()
        .find(|method| method.function == resolved.function)
    else {
        return false;
    };
    guard.method_slot_index
        == declaring_class
            .methods
            .iter()
            .position(|entry| {
                entry.name == method_entry.name && entry.function == method_entry.function
            })
            .and_then(|index| index.try_into().ok())
        && guard.method_is_final == method_entry.flags.is_final
        && guard.method_is_private == method_entry.flags.is_private
        && guard.method_is_static == method_entry.flags.is_static
}

pub(super) fn method_callee_shape_is_jit_eligible(
    owner: &CompiledUnit,
    function: FunctionId,
) -> bool {
    owner
        .unit()
        .functions
        .get(function.index())
        .is_some_and(|callee| {
            !callee.flags.is_top_level
                && !callee.flags.is_closure
                && !callee.flags.is_generator
                && !callee.returns_by_ref
                && matches!(
                    callee.return_type.as_ref(),
                    None | Some(
                        IrReturnType::Int
                            | IrReturnType::String
                            | IrReturnType::Bool
                            | IrReturnType::Null
                    )
                )
                && callee.captures.is_empty()
                && callee.params.iter().all(|param| {
                    !param.by_ref
                        && !param.variadic
                        && param.default.is_none()
                        && matches!(
                            param.type_.as_ref(),
                            None | Some(
                                IrReturnType::Int
                                    | IrReturnType::String
                                    | IrReturnType::Bool
                                    | IrReturnType::Null
                            )
                        )
                })
        })
}

pub(super) fn method_tiny_inline_rejection_reason(
    owner: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    method_entry: &php_ir::module::ClassMethodEntry,
    args: &[IrCallArg],
    has_magic_call: bool,
) -> Option<&'static str> {
    if has_magic_call {
        return Some("magic_call_present");
    }
    if method_entry.flags.is_static {
        return Some("static_method");
    }
    if !(method_entry.flags.is_final || method_entry.flags.is_private || class.flags.is_final) {
        return Some("not_final_or_private");
    }
    if !method_call_args_are_simple_positional(args) {
        return Some("named_or_unpacked_arguments");
    }
    let Some(function) = owner.unit().functions.get(method_entry.function.index()) else {
        return Some("method_body_unavailable");
    };
    if function.flags.is_generator {
        return Some("generator_method");
    }
    if function.returns_by_ref
        || function
            .params
            .iter()
            .any(|param| param.by_ref || param.variadic)
    {
        return Some("refs_or_variadics");
    }
    if method_body_has_inline_blocker(function) {
        return Some("control_flow_or_escape");
    }
    if method_body_returns_scalar_constant(owner, function)
        || method_body_returns_this_property(function)
    {
        None
    } else {
        Some("not_tiny_leaf_return")
    }
}

pub(super) fn method_body_has_inline_blocker(function: &IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::CallFunction { .. }
                    | InstructionKind::CallMethod { .. }
                    | InstructionKind::CallStaticMethod { .. }
                    | InstructionKind::CallClosure { .. }
                    | InstructionKind::CallCallable { .. }
                    | InstructionKind::Pipe { .. }
                    | InstructionKind::EnterTry { .. }
                    | InstructionKind::LeaveTry
                    | InstructionKind::EndFinally { .. }
                    | InstructionKind::Throw { .. }
                    | InstructionKind::Yield { .. }
                    | InstructionKind::YieldFrom { .. }
                    | InstructionKind::Include { .. }
                    | InstructionKind::Eval { .. }
                    | InstructionKind::AssignProperty { .. }
                    | InstructionKind::AssignPropertyDim { .. }
                    | InstructionKind::AssignStaticProperty { .. }
                    | InstructionKind::BindReferenceProperty { .. }
                    | InstructionKind::BindReferenceStaticProperty { .. }
                    | InstructionKind::UnsetProperty { .. }
                    | InstructionKind::UnsetPropertyDim { .. }
            )
        })
    })
}

pub(super) fn method_body_returns_scalar_constant(
    owner: &CompiledUnit,
    function: &IrFunction,
) -> bool {
    let Some(block) = single_return_block(function) else {
        return false;
    };
    let Some(value) = block
        .terminator
        .as_ref()
        .and_then(|terminator| match terminator.kind {
            TerminatorKind::Return { value, .. } => value,
            _ => None,
        })
    else {
        return false;
    };
    match value {
        Operand::Constant(constant) => owner
            .unit()
            .constants
            .get(constant.index())
            .is_some_and(is_tiny_inline_scalar_constant),
        Operand::Register(register) => block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::LoadConst { dst, constant }
                    if dst == register
                        && owner
                            .unit()
                            .constants
                            .get(constant.index())
                            .is_some_and(is_tiny_inline_scalar_constant)
            )
        }),
        Operand::Local(_) => false,
    }
}

pub(super) fn method_body_returns_this_property(function: &IrFunction) -> bool {
    let Some(block) = single_return_block(function) else {
        return false;
    };
    let Some(Operand::Register(return_register)) =
        block
            .terminator
            .as_ref()
            .and_then(|terminator| match terminator.kind {
                TerminatorKind::Return { value, .. } => value,
                _ => None,
            })
    else {
        return false;
    };
    block.instructions.iter().any(|instruction| {
        matches!(
            &instruction.kind,
            InstructionKind::FetchProperty {
                dst,
                object: Operand::Local(local),
                ..
            } if *dst == return_register
                && function
                    .locals
                    .get(local.index())
                    .is_some_and(|name| name == "this")
        )
    })
}

pub(super) fn single_return_block(function: &IrFunction) -> Option<&php_ir::block::BasicBlock> {
    (function.blocks.len() == 1)
        .then(|| function.blocks.first())
        .flatten()
        .filter(|block| {
            matches!(
                block.terminator.as_ref().map(|terminator| &terminator.kind),
                Some(TerminatorKind::Return { .. })
            )
        })
}

pub(super) fn is_tiny_inline_scalar_constant(constant: &IrConstant) -> bool {
    matches!(
        constant,
        IrConstant::Null
            | IrConstant::Bool(_)
            | IrConstant::Int(_)
            | IrConstant::Float(_)
            | IrConstant::String(_)
            | IrConstant::StringBytes(_)
    )
}
