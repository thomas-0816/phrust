fn quiet_known_reference_argument_loads(blocks: &mut [RegionBlock]) {
    let quiet_registers = blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
        .flat_map(|call| {
            call.args.iter().enumerate().filter_map(|(index, _)| {
                call.argument_requires_reference_binding(index)
                    .then(|| {
                        call.operands
                            .get(call.argument_operand_offset + index)
                            .copied()
                            .flatten()
                    })
                    .flatten()
                    .and_then(|operand| match operand {
                        RegionOperand::Register(register) => Some(register),
                        _ => None,
                    })
            })
        })
        .collect::<BTreeSet<_>>();
    if quiet_registers.is_empty() {
        return;
    }
    for instruction in blocks.iter_mut().flat_map(|block| &mut block.instructions) {
        if let RegionInstructionKind::LoadLocal { dst, quiet, .. } = &mut instruction.kind
            && quiet_registers.contains(dst)
        {
            *quiet = true;
        }
    }
}

fn collect_exception_regions(ir_function: &php_ir::IrFunction) -> Vec<RegionExceptionRegion> {
    let mut regions = ir_function
        .blocks
        .iter()
        .flat_map(|block| {
            block.instructions.iter().filter_map(move |instruction| {
                let InstructionKind::EnterTry {
                    catch,
                    catch_types,
                    finally,
                    after,
                    exception_local,
                } = &instruction.kind
                else {
                    return None;
                };
                Some(RegionExceptionRegion {
                    block: block.id,
                    protected_blocks: Vec::new(),
                    instruction: instruction.id,
                    span: instruction.span,
                    catch: *catch,
                    catch_types: catch_types.clone(),
                    finally: *finally,
                    after: *after,
                    exception_local: *exception_local,
                })
            })
        })
        .collect::<Vec<_>>();
    for region in &mut regions {
        let descriptor = region.clone();
        region.protected_blocks = ir_function
            .blocks
            .iter()
            .filter(|block| block_in_exception_body(ir_function, &descriptor, block.id))
            .map(|block| block.id)
            .collect();
    }
    regions
}

fn block_in_exception_body(
    function: &php_ir::IrFunction,
    region: &RegionExceptionRegion,
    candidate: BlockId,
) -> bool {
    if candidate == region.block {
        return true;
    }
    let mut pending = ir_block_successors(function, region.block);
    let mut visited = BTreeSet::new();
    while let Some(block) = pending.pop() {
        if Some(block) == region.catch || Some(block) == region.finally || block == region.after {
            continue;
        }
        if block == candidate {
            return true;
        }
        if visited.insert(block) {
            pending.extend(ir_block_successors(function, block));
        }
    }
    false
}

fn ir_block_successors(function: &php_ir::IrFunction, block: BlockId) -> Vec<BlockId> {
    let Some((index, block)) = function
        .blocks
        .iter()
        .enumerate()
        .find(|(_, candidate)| candidate.id == block)
    else {
        return Vec::new();
    };
    let Some(terminator) = &block.terminator else {
        return Vec::new();
    };
    let fallthrough = || function.blocks.get(index + 1).map(|block| block.id);
    match terminator.kind {
        TerminatorKind::Jump { target } => vec![target],
        TerminatorKind::JumpIfFalse { target, .. } | TerminatorKind::JumpIfTrue { target, .. } => {
            [Some(target), fallthrough()]
                .into_iter()
                .flatten()
                .collect()
        }
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => vec![if_true, if_false],
        TerminatorKind::Return { .. } | TerminatorKind::Exit { .. } => Vec::new(),
    }
}

fn stable_callable_local_entries(
    unit: &IrUnit,
    function: &php_ir::IrFunction,
) -> Vec<BTreeMap<LocalId, String>> {
    let mut predecessors = vec![Vec::<usize>::new(); function.blocks.len()];
    for block in &function.blocks {
        for successor in ir_block_successors(function, block.id) {
            if let Some(incoming) = predecessors.get_mut(successor.index()) {
                incoming.push(block.id.index());
            }
        }
    }
    for incoming in &mut predecessors {
        incoming.sort_unstable();
        incoming.dedup();
    }

    let mut entries = vec![None::<BTreeMap<LocalId, String>>; function.blocks.len()];
    let mut exits = vec![None::<BTreeMap<LocalId, String>>; function.blocks.len()];
    if !entries.is_empty() {
        entries[0] = Some(BTreeMap::new());
    }
    loop {
        let mut changed = false;
        for (block_index, block) in function.blocks.iter().enumerate() {
            let incoming = if block_index == 0 {
                BTreeMap::new()
            } else {
                let mut reachable = predecessors[block_index]
                    .iter()
                    .filter_map(|predecessor| exits[*predecessor].as_ref());
                let Some(first) = reachable.next() else {
                    continue;
                };
                let mut incoming = first.clone();
                for predecessor in reachable {
                    incoming.retain(|local, name| predecessor.get(local) == Some(name));
                }
                incoming
            };
            if entries[block_index].as_ref() != Some(&incoming) {
                entries[block_index] = Some(incoming.clone());
                changed = true;
            }
            let outgoing = transfer_stable_callable_locals(unit, block, incoming);
            if exits[block_index].as_ref() != Some(&outgoing) {
                exits[block_index] = Some(outgoing);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    entries.into_iter().map(Option::unwrap_or_default).collect()
}

fn transfer_stable_callable_locals(
    unit: &IrUnit,
    block: &php_ir::BasicBlock,
    mut locals: BTreeMap<LocalId, String>,
) -> BTreeMap<LocalId, String> {
    let mut registers = BTreeMap::<RegId, String>::new();
    let operand_name = |operand: Operand,
                        registers: &BTreeMap<RegId, String>,
                        locals: &BTreeMap<LocalId, String>| {
        match operand {
            Operand::Register(register) => registers.get(&register).cloned(),
            Operand::Local(local) => locals.get(&local).cloned(),
            Operand::Constant(constant) => match unit.constants.get(constant.index()) {
                Some(IrConstant::String(value)) => Some(value.clone()),
                _ => None,
            },
        }
    };
    for instruction in &block.instructions {
        match &instruction.kind {
            InstructionKind::LoadConst { dst, constant } => {
                if let Some(IrConstant::String(value)) = unit.constants.get(constant.index()) {
                    registers.insert(*dst, value.clone());
                }
            }
            InstructionKind::ResolveCallable {
                dst,
                callable: CallableKind::FunctionName { name },
            } => {
                registers.insert(*dst, name.clone());
            }
            InstructionKind::Move { dst, src } => {
                if let Some(name) = operand_name(*src, &registers, &locals) {
                    registers.insert(*dst, name);
                }
            }
            InstructionKind::LoadLocal { dst, local }
            | InstructionKind::LoadLocalQuiet { dst, local } => {
                if let Some(name) = locals.get(local) {
                    registers.insert(*dst, name.clone());
                }
            }
            InstructionKind::StoreLocal { local, src } => {
                if let Some(name) = operand_name(*src, &registers, &locals) {
                    locals.insert(*local, name);
                } else {
                    locals.remove(local);
                }
            }
            InstructionKind::BindReference { target, source } => {
                locals.remove(target);
                locals.remove(source);
            }
            InstructionKind::BindGlobal { local, .. }
            | InstructionKind::InitStaticLocal { local, .. }
            | InstructionKind::AssignDim { local, .. }
            | InstructionKind::AppendDim { local, .. }
            | InstructionKind::UnsetLocal { local }
            | InstructionKind::UnsetDim { local, .. }
            | InstructionKind::BindReferenceDim { local, .. }
            | InstructionKind::BindReferenceDimFromProperty { local, .. }
            | InstructionKind::ForeachInitRef { local, .. } => {
                locals.remove(local);
            }
            InstructionKind::BindReferenceFromProperty { target, .. }
            | InstructionKind::BindReferenceFromPropertyDim { target, .. }
            | InstructionKind::BindReferenceFromDim { target, .. }
            | InstructionKind::BindReferenceFromStaticPropertyDim { target, .. }
            | InstructionKind::BindReferenceFromCall { target, .. }
            | InstructionKind::BindReferenceFromMethodCall { target, .. } => {
                locals.remove(target);
            }
            InstructionKind::BindReferenceProperty { source, .. }
            | InstructionKind::BindReferencePropertyDim { source, .. }
            | InstructionKind::BindReferenceStaticProperty { source, .. } => {
                locals.remove(source);
            }
            InstructionKind::ForeachNextRef { value_local, .. } => {
                locals.remove(value_local);
            }
            _ => {}
        }
    }
    locals
}

fn annotate_native_finally_control(blocks: &mut [RegionBlock], handlers: &[RegionExceptionRegion]) {
    if blocks.is_empty() || handlers.is_empty() {
        return;
    }
    let mut entry_stacks = vec![None::<Vec<u32>>; blocks.len()];
    entry_stacks[0] = Some(Vec::new());
    let mut changed = true;
    while changed {
        changed = false;
        for block in blocks.iter() {
            let Some(mut stack) = entry_stacks[block.id.index()].clone() else {
                continue;
            };
            for instruction in &block.instructions {
                match instruction.kind {
                    RegionInstructionKind::NativeControl(RegionNativeControl::EnterTry {
                        handler_index,
                    }) => {
                        if let Some(handler) = handlers.get(handler_index as usize) {
                            for target in [handler.catch, handler.finally].into_iter().flatten() {
                                changed |=
                                    merge_handler_stack(&mut entry_stacks[target.index()], &stack);
                            }
                        }
                        stack.push(handler_index);
                    }
                    RegionInstructionKind::NativeControl(RegionNativeControl::LeaveTry) => {
                        let _ = stack.pop();
                    }
                    _ => {}
                }
            }
            for target in block.terminator.targets() {
                changed |= merge_handler_stack(&mut entry_stacks[target.index()], &stack);
            }
        }
    }

    for block in blocks {
        let mut stack = entry_stacks[block.id.index()].clone().unwrap_or_default();
        for instruction in &mut block.instructions {
            match &mut instruction.kind {
                RegionInstructionKind::NativeControl(RegionNativeControl::EnterTry {
                    handler_index,
                }) => stack.push(*handler_index),
                RegionInstructionKind::NativeControl(RegionNativeControl::LeaveTry) => {
                    let _ = stack.pop();
                }
                RegionInstructionKind::NativeControl(RegionNativeControl::EndFinally {
                    outer_finally,
                    ..
                }) => {
                    let stack_outer = stack
                        .iter()
                        .rev()
                        .filter_map(|index| handlers.get(*index as usize))
                        .find_map(|handler| handler.finally);
                    let static_outer = handlers
                        .iter()
                        .position(|handler| handler.finally == Some(block.id))
                        .and_then(|current_index| {
                            let current_block = handlers[current_index].block;
                            handlers[..current_index]
                                .iter()
                                .rev()
                                .find(|handler| handler.protected_blocks.contains(&current_block))
                                .and_then(|handler| handler.finally)
                        });
                    *outer_finally = static_outer.or(stack_outer);
                }
                _ => {}
            }
        }
        let stack_finally = stack
            .iter()
            .rev()
            .filter_map(|index| handlers.get(*index as usize))
            .find_map(|handler| handler.finally);
        // Data-flow joins deliberately retain only a common handler-stack
        // prefix. A return in a nested protected body can therefore lose its
        // inner handler when another path reaches the same block. The static
        // exception regions retain the precise nesting for protected blocks;
        // prefer their innermost handler and use the stack for returns from a
        // finally body itself.
        let pending_finally = handlers
            .iter()
            .rev()
            .find(|handler| handler.protected_blocks.contains(&block.id))
            .and_then(|handler| handler.finally)
            .or(stack_finally);
        match &mut block.terminator {
            RegionTerminator::Return { finally, .. }
            | RegionTerminator::ReturnReference { finally, .. }
            | RegionTerminator::Exit { finally, .. } => *finally = pending_finally,
            _ => {}
        }
    }
}

fn merge_handler_stack(slot: &mut Option<Vec<u32>>, candidate: &[u32]) -> bool {
    let Some(existing) = slot else {
        *slot = Some(candidate.to_vec());
        return true;
    };
    let common = existing
        .iter()
        .zip(candidate)
        .take_while(|(lhs, rhs)| lhs == rhs)
        .count();
    if common == existing.len() {
        return false;
    }
    existing.truncate(common);
    true
}

