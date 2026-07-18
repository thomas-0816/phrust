use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
struct NativeFragmentLayout {
    id: u32,
    blocks: BTreeSet<BlockId>,
    normal_entries: BTreeSet<BlockId>,
    external_targets: BTreeSet<BlockId>,
}

#[derive(Clone, Debug)]
struct NativeFunctionFragmentLayout {
    fragments: Vec<NativeFragmentLayout>,
    block_owner: BTreeMap<BlockId, u32>,
    resume_owner: BTreeMap<i32, u32>,
}

#[derive(Clone, Copy)]
struct NativeFragmentDefinition<'a> {
    layout: &'a NativeFunctionFragmentLayout,
    fragment: &'a NativeFragmentLayout,
    functions: &'a BTreeMap<u32, FuncId>,
}

#[derive(Clone, Copy)]
struct NativeFragmentWrapperDefinition<'a> {
    functions: &'a BTreeMap<u32, FuncId>,
    layout: &'a NativeFunctionFragmentLayout,
    relocation_functions: &'a BTreeMap<FunctionId, FuncId>,
}

fn native_fragment_frame_bytes(region: &RegionGraph) -> Result<u32, CraneliftLoweringError> {
    let slots = u64::from(region.local_count)
        .saturating_add(u64::from(region.register_count))
        .saturating_add(8);
    let bytes = slots.saturating_mul(8);
    let bytes = u32::try_from(bytes).map_err(|_| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_FRAGMENT_FRAME_SIZE",
            format!("native fragment frame requires {bytes} bytes"),
        )
    })?;
    if bytes > MAX_NATIVE_SPILL_FRAME_BYTES {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_FRAGMENT_FRAME_LIMIT",
            format!(
                "native fragment frame requires {bytes} bytes; limit is {MAX_NATIVE_SPILL_FRAME_BYTES}"
            ),
        ));
    }
    Ok(bytes.max(16))
}

fn native_fragment_local_offset(local: LocalId) -> i32 {
    i32::try_from(local.index().saturating_mul(8)).unwrap_or(i32::MAX)
}

fn native_fragment_register_offset(region: &RegionGraph, register: RegId) -> i32 {
    i32::try_from(
        (region.local_count as usize)
            .saturating_add(register.index())
            .saturating_mul(8),
    )
    .unwrap_or(i32::MAX)
}

fn native_fragment_pending_status_offset(region: &RegionGraph) -> i32 {
    i32::try_from(
        (region.local_count as usize)
            .saturating_add(region.register_count as usize)
            .saturating_mul(8),
    )
    .unwrap_or(i32::MAX)
}

fn native_fragment_pending_value_offset(region: &RegionGraph) -> i32 {
    native_fragment_pending_status_offset(region).saturating_add(8)
}

fn native_fragment_entry_id_offset(region: &RegionGraph) -> i32 {
    native_fragment_pending_value_offset(region).saturating_add(8)
}

fn native_fragment_arguments_offset(region: &RegionGraph) -> i32 {
    native_fragment_entry_id_offset(region).saturating_add(8)
}

fn native_fragment_result_out_offset(region: &RegionGraph) -> i32 {
    native_fragment_arguments_offset(region).saturating_add(8)
}

fn native_fragment_deopt_out_offset(region: &RegionGraph) -> i32 {
    native_fragment_result_out_offset(region).saturating_add(8)
}

fn native_fragment_resume_id_offset(region: &RegionGraph) -> i32 {
    native_fragment_deopt_out_offset(region).saturating_add(8)
}

fn native_fragment_resume_state_offset(region: &RegionGraph) -> i32 {
    native_fragment_resume_id_offset(region).saturating_add(8)
}

fn region_control_targets(block: &crate::region_ir::RegionBlock) -> BTreeSet<BlockId> {
    let mut targets = native_transition_successors(&block.terminator)
        .into_iter()
        .collect::<BTreeSet<_>>();
    match block.terminator {
        RegionTerminator::Return { finally, .. }
        | RegionTerminator::ReturnReference { finally, .. }
        | RegionTerminator::Exit { finally, .. } => {
            targets.extend(finally);
        }
        RegionTerminator::Jump { .. }
        | RegionTerminator::JumpIfFalse { .. }
        | RegionTerminator::JumpIfTrue { .. }
        | RegionTerminator::JumpIf { .. } => {}
    }
    for instruction in &block.instructions {
        if let RegionInstructionKind::NativeControl(control) = &instruction.kind {
            match control {
                RegionNativeControl::EndFinally {
                    after,
                    outer_finally,
                } => {
                    targets.insert(*after);
                    targets.extend(*outer_finally);
                }
                RegionNativeControl::Throw { catch, finally, .. } => {
                    targets.extend(*catch);
                    targets.extend(*finally);
                }
                RegionNativeControl::EnterTry { .. }
                | RegionNativeControl::LeaveTry
                | RegionNativeControl::MakeException { .. } => {}
            }
        }
    }
    targets
}

impl NativeFunctionFragmentLayout {
    fn for_plan(
        region: &RegionGraph,
        plan: &NativeCompilePlan,
        suspending_functions: &BTreeSet<FunctionId>,
    ) -> Result<Self, CraneliftLoweringError> {
        let mut block_owner = BTreeMap::new();
        for fragment in &plan.fragments {
            for block in &fragment.blocks {
                if block_owner.insert(*block, fragment.id).is_some() {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_DUPLICATE_BLOCK",
                        format!("Region block {} occurs in multiple fragments", block.raw()),
                    ));
                }
            }
        }
        if block_owner.len() != region.blocks.len() {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_FRAGMENT_INCOMPLETE_PLAN",
                format!(
                    "fragment plan owns {} of {} Region blocks",
                    block_owner.len(),
                    region.blocks.len()
                ),
            ));
        }
        let mut fragments = plan
            .fragments
            .iter()
            .map(|fragment| NativeFragmentLayout {
                id: fragment.id,
                blocks: fragment.blocks.iter().copied().collect(),
                normal_entries: BTreeSet::new(),
                external_targets: BTreeSet::new(),
            })
            .collect::<Vec<_>>();
        if let Some(owner) = block_owner.get(&BlockId::new(0)).copied() {
            fragments[owner as usize]
                .normal_entries
                .insert(BlockId::new(0));
        }
        for block in &region.blocks {
            let source_owner = block_owner[&block.id];
            for target in region_control_targets(block) {
                let target_owner = block_owner.get(&target).copied().ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_UNKNOWN_TARGET",
                        format!(
                            "Region block {} targets missing block {}",
                            block.id.raw(),
                            target.raw()
                        ),
                    )
                })?;
                if source_owner != target_owner {
                    fragments[source_owner as usize]
                        .external_targets
                        .insert(target);
                    fragments[target_owner as usize]
                        .normal_entries
                        .insert(target);
                }
            }
        }

        let transition_liveness = native_transition_register_liveness(region, suspending_functions);
        let mut resume_owner = BTreeMap::new();
        let mut insert_resume = |resume_id: i32, block: BlockId| {
            let owner = block_owner[&block];
            match resume_owner.insert(resume_id, owner) {
                Some(previous) if previous != owner => Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_FRAGMENT_RESUME_COLLISION",
                    format!("resume id {resume_id} belongs to fragments {previous} and {owner}"),
                )),
                _ => Ok(()),
            }
        };
        for handler in &region.exception_regions {
            for target in [handler.catch, handler.finally].into_iter().flatten() {
                insert_resume(crate::native_handler_resume_id(target), target)?;
            }
        }
        for block in &region.blocks {
            for instruction in &block.instructions {
                if matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_)) {
                    insert_resume(
                        crate::native_suspension_resume_id(instruction.continuation_id),
                        block.id,
                    )?;
                }
                if instruction_has_native_transition(instruction, suspending_functions)
                    && transition_liveness
                        .get(&instruction.continuation_id)
                        .is_some_and(|live| live.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
                {
                    insert_resume(
                        crate::native_transition_resume_id(instruction.continuation_id),
                        block.id,
                    )?;
                }
            }
        }
        for osr in region.osr_entries() {
            insert_resume(
                i32::try_from(osr.id).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_OSR_ID",
                        format!("OSR id {} does not fit the native resume ABI", osr.id),
                    )
                })?,
                osr.block,
            )?;
        }
        Ok(Self {
            fragments,
            block_owner,
            resume_owner,
        })
    }
}

fn region_contains(
    region: &RegionGraph,
    predicate: impl Fn(&RegionInstructionKind) -> bool,
) -> bool {
    region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| predicate(&instruction.kind))
}

fn native_transition_successors(terminator: &RegionTerminator) -> Vec<BlockId> {
    match terminator {
        RegionTerminator::Jump { target } => vec![*target],
        RegionTerminator::JumpIfFalse {
            target,
            fallthrough,
            ..
        }
        | RegionTerminator::JumpIfTrue {
            target,
            fallthrough,
            ..
        } => vec![*target, *fallthrough],
        RegionTerminator::JumpIf {
            if_true, if_false, ..
        } => vec![*if_true, *if_false],
        RegionTerminator::Return { .. }
        | RegionTerminator::ReturnReference { .. }
        | RegionTerminator::Exit { .. } => Vec::new(),
    }
}

fn native_call_target(call: &RegionNativeCall) -> Option<FunctionId> {
    let RegionCallTarget::Function {
        function: Some(function),
        ..
    } = call.target
    else {
        return None;
    };
    Some(function)
}

fn instruction_has_native_transition(
    instruction: &RegionInstruction,
    suspending_functions: &BTreeSet<FunctionId>,
) -> bool {
    // Checked/native binary operations can request a baseline retry. A call
    // needs an instruction-entry loader only when its statically known target
    // can reach Fiber::suspend: the VM resumes the caller after the nested
    // fiber completes by replaying that call against its completed-call slot.
    // Keeping every other call out of this set preserves the bounded native
    // CFG and metadata shape for ordinary PHP code.
    matches!(instruction.kind, RegionInstructionKind::Binary { .. })
        || matches!(
            &instruction.kind,
            RegionInstructionKind::NativeCall(call)
                if native_call_target(call)
                    .is_some_and(|target| suspending_functions.contains(&target))
        )
}

fn instruction_has_sparse_snapshot(
    instruction: &RegionInstruction,
    suspending_functions: &BTreeSet<FunctionId>,
) -> bool {
    instruction_has_native_transition(instruction, suspending_functions)
        || matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_))
}

fn statically_called_function(
    unit: &IrUnit,
    instruction: &php_ir::InstructionKind,
) -> Option<FunctionId> {
    match instruction {
        php_ir::InstructionKind::CallFunction { name, .. } => unit
            .function_table
            .iter()
            .find(|entry| entry.name == *name)
            .map(|entry| entry.function),
        php_ir::InstructionKind::CallStaticMethod {
            class_name, method, ..
        } if !class_name.eq_ignore_ascii_case("fiber") => {
            unit.classes
                .iter()
                .find(|class| class.name.eq_ignore_ascii_case(class_name))
                .and_then(|class| {
                    class.methods.iter().find(|entry| {
                        entry.name.eq_ignore_ascii_case(method) && entry.flags.is_static
                    })
                })
                .map(|entry| entry.function)
        }
        _ => None,
    }
}

/// Returns only functions reachable from this native region's statically
/// identified call targets that can transitively reach `Fiber::suspend`.
/// This is deliberately narrower than "contains a call": ordinary calls keep
/// the exact transition-free native shape used by the function-on-demand tier.
fn suspending_functions_for_region(unit: &IrUnit, region: &RegionGraph) -> BTreeSet<FunctionId> {
    let roots = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| {
            let RegionInstructionKind::NativeCall(call) = &instruction.kind else {
                return None;
            };
            native_call_target(call)
        })
        .collect::<BTreeSet<_>>();
    if roots.is_empty() {
        return BTreeSet::new();
    }

    let mut reachable = roots;
    let mut pending = reachable.iter().copied().collect::<Vec<_>>();
    let mut callees = BTreeMap::<FunctionId, BTreeSet<FunctionId>>::new();
    while let Some(function) = pending.pop() {
        let Some(ir_function) = unit.functions.get(function.index()) else {
            continue;
        };
        let function_callees = ir_function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| statically_called_function(unit, &instruction.kind))
            .collect::<BTreeSet<_>>();
        for callee in &function_callees {
            if reachable.insert(*callee) {
                pending.push(*callee);
            }
        }
        callees.insert(function, function_callees);
    }

    let mut suspending = reachable
        .iter()
        .copied()
        .filter(|function| {
            unit.functions
                .get(function.index())
                .is_some_and(|function| {
                    !function.flags.is_top_level
                        && function
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .any(|instruction| {
                                matches!(
                                    &instruction.kind,
                                    php_ir::InstructionKind::CallStaticMethod {
                                        class_name,
                                        method,
                                        args,
                                        ..
                                    } if class_name.eq_ignore_ascii_case("fiber")
                                        && method.eq_ignore_ascii_case("suspend")
                                        && args.len() <= 1
                                )
                            })
                })
        })
        .collect::<BTreeSet<_>>();

    loop {
        let callers = callees
            .iter()
            .filter_map(|(caller, targets)| {
                (!suspending.contains(caller)
                    && targets.iter().any(|target| suspending.contains(target)))
                .then_some(*caller)
            })
            .collect::<Vec<_>>();
        if callers.is_empty() {
            break;
        }
        suspending.extend(callers);
    }
    suspending
}

/// Classical SSA live-in sets for the small set of actual native transition
/// safepoints. This deliberately does not equate "defined earlier" with
/// "live now": doing so creates cumulative register prefixes and quadratic
/// Cranelift move/alias pressure in large PHP functions.
fn native_register_live_in(region: &RegionGraph) -> BTreeMap<BlockId, BTreeSet<RegId>> {
    let block_indices = region
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect::<BTreeMap<_, _>>();
    let mut live_in = vec![BTreeSet::<RegId>::new(); region.blocks.len()];
    loop {
        let mut changed = false;
        for (index, block) in region.blocks.iter().enumerate().rev() {
            let mut live = native_transition_successors(&block.terminator)
                .into_iter()
                .filter_map(|successor| block_indices.get(&successor).copied())
                .flat_map(|successor| live_in[successor].iter().copied())
                .collect::<BTreeSet<_>>();
            live.extend(block.terminator.register_uses());
            for instruction in block.instructions.iter().rev() {
                if let Some(defined) = region_instruction_result_register(&instruction.kind) {
                    live.remove(&defined);
                }
                live.extend(instruction.register_uses());
            }
            if live != live_in[index] {
                live_in[index] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    region
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, live_in[index].clone()))
        .collect()
}

fn native_transition_register_liveness(
    region: &RegionGraph,
    suspending_functions: &BTreeSet<FunctionId>,
) -> BTreeMap<u32, Vec<RegId>> {
    let block_live_in = native_register_live_in(region);

    let mut safepoints = BTreeMap::new();
    for block in &region.blocks {
        let mut live = native_transition_successors(&block.terminator)
            .into_iter()
            .filter_map(|successor| block_live_in.get(&successor))
            .flat_map(|registers| registers.iter().copied())
            .collect::<BTreeSet<_>>();
        live.extend(block.terminator.register_uses());
        for instruction in block.instructions.iter().rev() {
            if let Some(defined) = region_instruction_result_register(&instruction.kind) {
                live.remove(&defined);
            }
            live.extend(instruction.register_uses());
            if instruction_has_sparse_snapshot(instruction, suspending_functions) {
                safepoints.insert(instruction.continuation_id, live.iter().copied().collect());
            }
        }
    }
    safepoints
}

fn ir_function_requires_trampoline(function: &php_ir::IrFunction) -> bool {
    function.params.iter().any(|parameter| parameter.by_ref)
        || function.returns_by_ref
        || function.blocks.iter().any(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(
                    instruction.kind,
                    php_ir::InstructionKind::EnterTry { .. }
                        | php_ir::InstructionKind::LeaveTry
                        | php_ir::InstructionKind::Throw { .. }
                        | php_ir::InstructionKind::MakeClosure { .. }
                )
            })
        })
        || function.attributes.iter().any(|attribute| {
            attribute
                .resolved_name
                .as_deref()
                .or(attribute.fallback_name.as_deref())
                .unwrap_or(&attribute.name)
                .trim_start_matches('\\')
                .eq_ignore_ascii_case("deprecated")
        })
}

fn declare_value_operation(
    module: &mut JITModule,
    symbol: &str,
    arity: u8,
    address: usize,
) -> Result<NativeHelper, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I32));
    for _ in 0..arity {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    let import_symbol = native_helper_import_symbol(symbol, address);
    let function = module
        .declare_function(&import_symbol, Linkage::Import, &signature)
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                format!("failed to declare {symbol}: {error}"),
            )
        })?;
    Ok(NativeHelper {
        function,
        terminal_exit: None,
    })
}

fn declare_native_helper(
    module: &mut JITModule,
    symbol: &str,
    signature: &ir::Signature,
    address: usize,
) -> Result<NativeHelper, CraneliftLoweringError> {
    let import_symbol = native_helper_import_symbol(symbol, address);
    let function = module
        .declare_function(&import_symbol, Linkage::Import, signature)
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                format!("failed to declare {symbol}: {error}"),
            )
        })?;
    Ok(NativeHelper {
        function,
        terminal_exit: None,
    })
}

pub(super) fn compile_region_graph_native(
    unit: &IrUnit,
    region: &RegionGraph,
    plan: NativeCompilePlan,
    runtime_helpers: crate::JitRuntimeHelperAddresses,
    request: &JitCompileRequest,
) -> Result<NativeScalarRegionCompileResult, CraneliftLoweringError> {
    validate_region_native_coverage(region)?;
    let suspending_functions = suspending_functions_for_region(unit, region);
    let mut regions = collect_region_graphs(region)?;
    for candidate in regions.values_mut() {
        if candidate.compile_metadata.tier == NativeCompilerTier::Optimizing {
            if plan.permits_whole_region_optimization() {
                let _ = crate::region_ir::opt::optimize_executable_region(candidate);
            }
            // Fragmented functions retain the optimizing tier, but whole-graph
            // rewrites are limited to plans that explicitly fit the bounded
            // whole-region budget. Every generated fragment still crosses the
            // explicit frame ABI for its live locals and registers.
        }
    }
    if let Some(fragment) = plan
        .fragments
        .iter()
        .find(|fragment| !fragment.is_within_budget())
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_FRAGMENT_BUDGET",
            format!(
                "fragment {} exceeds the pre-Cranelift budget: blocks={} instructions={} estimated_clif_blocks={}",
                fragment.id,
                fragment.blocks.len(),
                fragment.ir_instructions,
                fragment.estimated_clif_blocks
            ),
        ));
    }
    let fragment_layout = (plan.fragments.len() > 1)
        .then(|| NativeFunctionFragmentLayout::for_plan(region, &plan, &suspending_functions))
        .transpose()?;
    let ssa_metrics = regions
        .values()
        .filter(|candidate| candidate.compile_metadata.tier == NativeCompilerTier::Optimizing)
        .map(|candidate| {
            let flow = crate::region_ir::analyze_executable_value_flow(candidate, &unit.constants);
            (
                flow.promoted_local_count() as u64,
                flow.promoted_register_count() as u64,
                flow.ownership_move_count() as u64,
            )
        })
        .fold((0_u64, 0_u64, 0_u64), |total, metrics| {
            (
                total.0.saturating_add(metrics.0),
                total.1.saturating_add(metrics.1),
                total.2.saturating_add(metrics.2),
            )
        });
    let function = region.function;
    let arity = region_arity(region)?;
    let fast_path_hits = regions
        .values()
        .map(|region| region.fast_path_operations)
        .sum();
    let has_control_flow = regions.values().any(RegionGraph::has_control_flow);
    let mut trampoline_functions = regions
        .iter()
        .filter_map(|(function, region)| {
            (!region.exception_regions.is_empty()
                || region.params.iter().any(|parameter| parameter.by_ref)
                || region.returns_by_ref
                || region_contains(region, |kind| {
                    matches!(
                        kind,
                        RegionInstructionKind::NativeControl(RegionNativeControl::Throw { .. })
                            | RegionInstructionKind::NativeDynamicCode(
                                RegionNativeDynamicCode::MakeClosure { .. }
                            )
                    )
                })
                || region.attributes.iter().any(|attribute| {
                    attribute
                        .resolved_name
                        .as_deref()
                        .or(attribute.fallback_name.as_deref())
                        .unwrap_or(&attribute.name)
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case("deprecated")
                }))
            .then_some(*function)
        })
        .collect::<BTreeSet<_>>();
    loop {
        let callers = regions
            .iter()
            .filter_map(|(function, region)| {
                region
                    .direct_callees()
                    .iter()
                    .any(|callee| trampoline_functions.contains(callee))
                    .then_some(*function)
            })
            .collect::<Vec<_>>();
        let previous = trampoline_functions.len();
        trampoline_functions.extend(callers);
        if trampoline_functions.len() == previous {
            break;
        }
    }
    let needs_call_trampoline = regions.values().any(|region| {
        region.has_native_trampoline_calls()
            || region
                .direct_callees()
                .iter()
                .any(|callee| !regions.contains_key(callee))
            || region_contains(region, |kind| {
                matches!(
                    kind,
                    RegionInstructionKind::NativeCall(call)
                        if matches!(call.result, RegionCallResult::ReferenceLocal(_))
                            || call.args.iter().any(|argument| {
                                argument.name.is_some() || argument.unpack
                            })
                            || call.direct_compiled_target().is_some_and(|target| {
                                trampoline_functions.contains(&target)
                            })
                )
            })
    });
    let needs_function_resolver = runtime_helpers.native_function_resolve != 0
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                let RegionInstructionKind::NativeCall(call) = kind else {
                    return false;
                };
                !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                    && call
                        .args
                        .iter()
                        .all(|argument| argument.name.is_none() && !argument.unpack)
                    && call.direct_compiled_target().is_some_and(|target| {
                        !regions.contains_key(&target)
                            && unit
                                .functions
                                .get(target.index())
                                .is_some_and(|function| !ir_function_requires_trampoline(function))
                    })
            })
        });
    let needs_frame_arena = runtime_helpers.native_frame_alloc != 0
        && runtime_helpers.native_frame_release != 0
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(_))
            })
        });
    if needs_call_trampoline && runtime_helpers.native_call_dispatch == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_CALL_TRAMPOLINE",
            "dynamic or complex call requires the typed native dispatch trampoline",
        ));
    }
    let needs_dynamic_code = regions.values().any(RegionGraph::has_native_dynamic_code);
    if needs_dynamic_code && runtime_helpers.native_dynamic_code == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_DYNAMIC_CODE",
            "include, eval, or runtime declaration requires the native dynamic-code compiler",
        ));
    }
    let native_call_symbol = NATIVE_CALL_DISPATCH_SYMBOL.to_owned();
    let native_function_resolve_symbol = NATIVE_FUNCTION_RESOLVE_SYMBOL.to_owned();
    let native_dynamic_code_symbol = NATIVE_DYNAMIC_CODE_SYMBOL.to_owned();
    let needs_unary = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::Unary { .. })
        })
    });
    let needs_binary = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::Binary { .. })
        })
    });
    let needs_compare = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Compare { .. } | RegionInstructionKind::IssetDim { .. }
            )
        })
    });
    let needs_cast = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::Cast { .. })
        })
    });
    let needs_echo = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::Echo { .. })
        })
    });
    let needs_local_fetch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::LoadLocal { .. }
                    | RegionInstructionKind::FetchDim {
                        array: RegionOperand::Local(_),
                        ..
                    }
                    | RegionInstructionKind::AssignDim { .. }
                    | RegionInstructionKind::AppendDim { .. }
                    | RegionInstructionKind::UnsetDim { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
                    | RegionInstructionKind::IssetDim { .. }
                    | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::IssetLocal { .. }
                    | RegionInstructionKind::EmptyLocal { .. }
            )
        })
    });
    let needs_local_store = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::StoreLocal { .. }
                    | RegionInstructionKind::AssignLocalResult { .. }
                    | RegionInstructionKind::AssignDim { .. }
                    | RegionInstructionKind::AppendDim { .. }
                    | RegionInstructionKind::UnsetDim { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
            )
        })
    });
    let needs_value_lifecycle = true;
    // Local publication is part of the native frame ABI, not just explicit
    // PHP reference syntax.  Stores, unsets, foreach-by-reference and array
    // root updates can all publish a local through the same helper.  Keep the
    // helper available for every executable region so adding publication to a
    // lowering cannot accidentally make an otherwise supported function
    // uncompilable.
    let needs_reference_bind = true;
    let needs_argument_check = regions.values().any(|region| {
        region
            .params
            .iter()
            .any(|parameter| parameter.type_.is_some())
    }) || (needs_function_resolver
        && unit.functions.iter().any(|function| {
            function
                .params
                .iter()
                .any(|parameter| parameter.type_.is_some())
        }));
    let _has_explicit_reference_bind = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::BindReference { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
                    | RegionInstructionKind::BindReferenceIntoDim { .. }
                    | RegionInstructionKind::BindReferenceProperty { .. }
                    | RegionInstructionKind::BindReferenceFromProperty { .. }
                    | RegionInstructionKind::BindReferenceFromPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
                    | RegionInstructionKind::InitStaticLocal { .. }
            ) || matches!(kind, RegionInstructionKind::NativeCall(call) if
                call.needs_local_reference_binding()
                    || call.direct_compiled_target().is_some_and(|target| {
                        regions.get(&target).is_some_and(|callee| {
                            callee.params.iter().any(|parameter| parameter.by_ref)
                        })
                    })
            )
        })
    });
    let needs_return_check = true;
    let needs_exception_new = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::NativeControl(RegionNativeControl::MakeException { .. })
            )
        })
    });
    let needs_array_new = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NewArray { .. })
                || matches!(kind, RegionInstructionKind::NativeCall(call) if call.variadic)
        })
    });
    let needs_object_new = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NewObject { .. })
        })
    });
    let needs_property_fetch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::FetchProperty { .. }
                    | RegionInstructionKind::FetchDynamicStaticProperty { .. }
                    | RegionInstructionKind::FetchObjectClassName { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
            )
        })
    });
    let needs_property_assign = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::AssignProperty { .. }
                    | RegionInstructionKind::BindReferenceProperty { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
            )
        })
    });
    let needs_object_clone = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::CloneObject { .. })
        })
    });
    let needs_object_clone_with = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::CloneWith { .. })
        })
    });
    let needs_array_insert = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::ArrayInsert { .. }
                    | RegionInstructionKind::AssignDim { .. }
                    | RegionInstructionKind::AppendDim { .. }
                    | RegionInstructionKind::UnsetDim { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
                    | RegionInstructionKind::BindReferenceIntoDim { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
            ) || matches!(kind, RegionInstructionKind::NativeCall(call) if call.variadic)
        })
    });
    let needs_array_fetch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::FetchDim { .. }
                    | RegionInstructionKind::AssignDim { .. }
                    | RegionInstructionKind::AppendDim { .. }
                    | RegionInstructionKind::IssetDim { .. }
                    | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::UnsetDim { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
                    | RegionInstructionKind::BindReferenceIntoDim { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
            )
        })
    });
    let needs_array_unset = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::UnsetDim { .. })
        })
    });
    let needs_array_spread = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::ArraySpread { .. })
        })
    });
    let needs_foreach_init = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::ForeachInit { .. }
                    | RegionInstructionKind::ForeachInitRef { .. }
            )
        })
    });
    let needs_foreach_next = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::ForeachNext { .. }
                    | RegionInstructionKind::ForeachNextRef { .. }
            )
        })
    });
    let needs_foreach_cleanup = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::ForeachCleanup { .. })
        })
    });
    let needs_constant_fetch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::FetchConst { .. })
        })
    });
    let needs_truthy = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Unary {
                    op: crate::region_ir::RegionUnaryOp::Not,
                    ..
                } | RegionInstructionKind::Cast {
                    op: crate::region_ir::RegionCastOp::Bool,
                    ..
                } | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::EmptyLocal { .. }
            )
        }) || region.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                RegionTerminator::JumpIfFalse { .. }
                    | RegionTerminator::JumpIfTrue { .. }
                    | RegionTerminator::JumpIf { .. }
            )
        })
    });
    let needs_type_predicate = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call) if stable_builtin_type_predicate(&call.target).is_some())
        })
    });
    let needs_stable_length = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::EmptyDim { .. } | RegionInstructionKind::EmptyLocal { .. }
            ) || matches!(kind, RegionInstructionKind::NativeCall(call) if stable_builtin_length(&call.target).is_some())
        })
    });
    let needs_runtime_fatal = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::RuntimeFatal { .. })
        })
    });
    let needs_execution_poll = regions
        .values()
        .any(|region| !region.osr_entries().is_empty());
    let mut imports = vec![(
        "region-runtime-helper-abi".to_owned(),
        region.compile_metadata.helper_abi_hash as usize,
    )];
    if needs_call_trampoline {
        imports.push((
            native_call_symbol.clone(),
            runtime_helpers.native_call_dispatch,
        ));
    }
    if needs_function_resolver {
        imports.push((
            native_function_resolve_symbol.clone(),
            runtime_helpers.native_function_resolve,
        ));
    }
    if needs_frame_arena {
        imports.push((
            "phrust_native_frame_alloc".to_owned(),
            runtime_helpers.native_frame_alloc,
        ));
        imports.push((
            "phrust_native_frame_release".to_owned(),
            runtime_helpers.native_frame_release,
        ));
    }
    if needs_dynamic_code {
        imports.push((
            native_dynamic_code_symbol.clone(),
            runtime_helpers.native_dynamic_code,
        ));
    }
    for (needed, configured, fallback, symbol) in [
        (
            needs_unary,
            runtime_helpers.native_unary,
            test_native_unary_fallback as *const () as usize,
            "phrust_native_unary",
        ),
        (
            needs_binary,
            runtime_helpers.native_binary,
            test_native_binary_fallback as *const () as usize,
            "phrust_native_binary",
        ),
        (
            needs_compare,
            runtime_helpers.native_compare,
            test_native_compare_fallback as *const () as usize,
            "phrust_native_compare",
        ),
        (
            needs_cast,
            runtime_helpers.native_cast,
            test_native_cast_fallback as *const () as usize,
            "phrust_native_cast",
        ),
        (
            needs_echo,
            runtime_helpers.native_echo,
            test_native_echo_fallback as *const () as usize,
            "phrust_native_echo",
        ),
        (
            needs_local_fetch,
            runtime_helpers.native_local_fetch,
            test_native_local_fetch_fallback as *const () as usize,
            "phrust_native_local_fetch",
        ),
        (
            needs_local_store,
            runtime_helpers.native_local_store,
            test_native_local_store_fallback as *const () as usize,
            "phrust_native_local_store",
        ),
        (
            needs_value_lifecycle,
            runtime_helpers.native_value_lifecycle,
            test_native_value_lifecycle_fallback as *const () as usize,
            "phrust_native_value_lifecycle",
        ),
        (
            needs_reference_bind,
            runtime_helpers.native_reference_bind,
            test_native_reference_bind_fallback as *const () as usize,
            "phrust_native_reference_bind",
        ),
        (
            needs_argument_check,
            runtime_helpers.native_argument_check,
            test_native_argument_check_fallback as *const () as usize,
            "phrust_native_argument_check",
        ),
        (
            needs_return_check,
            runtime_helpers.native_return_check,
            test_native_return_check_fallback as *const () as usize,
            "phrust_native_return_check",
        ),
        (
            needs_exception_new,
            runtime_helpers.native_exception_new,
            test_native_exception_new_fallback as *const () as usize,
            "phrust_native_exception_new",
        ),
        (
            needs_array_new,
            runtime_helpers.native_array_new,
            test_native_array_new_fallback as *const () as usize,
            "phrust_native_array_new",
        ),
        (
            needs_object_new,
            runtime_helpers.native_object_new,
            test_native_object_new_fallback as *const () as usize,
            "phrust_native_object_new",
        ),
        (
            needs_property_fetch,
            runtime_helpers.native_property_fetch,
            test_native_property_fetch_fallback as *const () as usize,
            "phrust_native_property_fetch",
        ),
        (
            needs_property_assign,
            runtime_helpers.native_property_assign,
            test_native_property_assign_fallback as *const () as usize,
            "phrust_native_property_assign",
        ),
        (
            needs_object_clone,
            runtime_helpers.native_object_clone,
            test_native_object_clone_fallback as *const () as usize,
            "phrust_native_object_clone",
        ),
        (
            needs_object_clone_with,
            runtime_helpers.native_object_clone_with,
            test_native_object_clone_with_fallback as *const () as usize,
            "phrust_native_object_clone_with",
        ),
        (
            needs_array_insert,
            runtime_helpers.native_array_insert,
            test_native_array_insert_fallback as *const () as usize,
            "phrust_native_array_insert",
        ),
        (
            needs_array_fetch,
            runtime_helpers.native_array_fetch,
            test_native_array_fetch_fallback as *const () as usize,
            "phrust_native_array_fetch",
        ),
        (
            needs_array_unset,
            runtime_helpers.native_array_unset,
            test_native_array_unset_fallback as *const () as usize,
            "phrust_native_array_unset",
        ),
        (
            needs_array_spread,
            runtime_helpers.native_array_spread,
            test_native_array_spread_fallback as *const () as usize,
            "phrust_native_array_spread",
        ),
        (
            needs_foreach_init,
            runtime_helpers.native_foreach_init,
            test_native_foreach_init_fallback as *const () as usize,
            "phrust_native_foreach_init",
        ),
        (
            needs_foreach_next,
            runtime_helpers.native_foreach_next,
            test_native_foreach_next_fallback as *const () as usize,
            "phrust_native_foreach_next",
        ),
        (
            needs_foreach_cleanup,
            runtime_helpers.native_foreach_cleanup,
            test_native_foreach_cleanup_fallback as *const () as usize,
            "phrust_native_foreach_cleanup",
        ),
        (
            needs_constant_fetch,
            runtime_helpers.native_constant_fetch,
            test_native_constant_fetch_fallback as *const () as usize,
            "phrust_native_constant_fetch",
        ),
        (
            needs_truthy,
            runtime_helpers.native_truthy,
            test_native_truthy_fallback as *const () as usize,
            "phrust_native_truthy",
        ),
        (
            needs_type_predicate,
            runtime_helpers.native_type_predicate,
            test_native_type_predicate_fallback as *const () as usize,
            "phrust_native_type_predicate",
        ),
        (
            needs_stable_length,
            runtime_helpers.native_stable_length,
            test_native_stable_length_fallback as *const () as usize,
            "phrust_native_stable_length",
        ),
        (
            needs_runtime_fatal,
            runtime_helpers.native_runtime_fatal,
            test_native_runtime_fatal_fallback as *const () as usize,
            "phrust_native_runtime_fatal",
        ),
        (
            needs_execution_poll,
            runtime_helpers.native_execution_poll,
            test_native_execution_poll_fallback as *const () as usize,
            "phrust_native_execution_poll",
        ),
    ] {
        if needed {
            let address = if configured == 0 {
                fallback
            } else {
                configured
            };
            imports.push((symbol.to_owned(), address));
        }
    }
    #[cfg(test)]
    {
        let aliases = imports
            .iter()
            .skip(1)
            .map(|(symbol, address)| (native_helper_import_symbol(symbol, *address), *address))
            .collect::<Vec<_>>();
        imports.extend(aliases);
    }
    let import_refs = imports
        .iter()
        .map(|(name, address)| (name.as_str(), *address))
        .collect::<Vec<_>>();
    let function_key = native_function_key(
        request
            .ir_fingerprint
            .clone()
            .unwrap_or_else(|| crate::stable_ir_fingerprint(unit)),
        function.raw(),
        unit.functions[function.index()].params.len(),
        region.local_count,
        request.opt_level != 0,
        request.invalidation_generation,
    );
    let compiled_clif_blocks = std::cell::Cell::new(None);
    let compiled_maximum_pre_regalloc = std::cell::Cell::new(None);
    let compiled = compile_managed_native(
        request,
        function,
        function_key,
        BASELINE_FUNCTION_SPECIALIZATION,
        &import_refs,
        |module, codegen_context, builder_context, name| {
            let helper_address = |symbol: &str| {
                imports
                    .iter()
                    .find_map(|(name, address)| (name == symbol).then_some(*address))
                    .expect("required native helper address must be imported")
            };
            let native_call_helper = if needs_call_trampoline {
                let pointer_type = module.target_config().pointer_type();
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                Some(declare_native_helper(
                    module,
                    &native_call_symbol,
                    &signature,
                    helper_address(&native_call_symbol),
                )?)
            } else {
                None
            };
            let native_dynamic_code_helper = if needs_dynamic_code {
                let pointer_type = module.target_config().pointer_type();
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                Some(declare_native_helper(
                    module,
                    &native_dynamic_code_symbol,
                    &signature,
                    helper_address(&native_dynamic_code_symbol),
                )?)
            } else {
                None
            };
            let mut native_operations = NativeOperationFunctions::default();
            let pointer_type = module.target_config().pointer_type();
            if needs_function_resolver {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.function_resolve = Some(declare_native_helper(
                    module,
                    &native_function_resolve_symbol,
                    &signature,
                    helper_address(&native_function_resolve_symbol),
                )?);
            }
            if needs_frame_arena {
                let mut alloc_signature = module.make_signature();
                alloc_signature.params.push(AbiParam::new(types::I64));
                alloc_signature.params.push(AbiParam::new(types::I64));
                alloc_signature.params.push(AbiParam::new(types::I64));
                alloc_signature.returns.push(AbiParam::new(pointer_type));
                native_operations.frame_alloc = Some(declare_native_helper(
                    module,
                    "phrust_native_frame_alloc",
                    &alloc_signature,
                    helper_address("phrust_native_frame_alloc"),
                )?);
                let mut release_signature = module.make_signature();
                release_signature.params.push(AbiParam::new(types::I64));
                release_signature.params.push(AbiParam::new(pointer_type));
                release_signature.returns.push(AbiParam::new(types::I32));
                native_operations.frame_release = Some(declare_native_helper(
                    module,
                    "phrust_native_frame_release",
                    &release_signature,
                    helper_address("phrust_native_frame_release"),
                )?);
            }
            if needs_unary {
                native_operations.unary = Some(declare_value_operation(
                    module,
                    "phrust_native_unary",
                    1,
                    helper_address("phrust_native_unary"),
                )?);
            }
            if needs_binary {
                native_operations.binary = Some(declare_value_operation(
                    module,
                    "phrust_native_binary",
                    4,
                    helper_address("phrust_native_binary"),
                )?);
            }
            if needs_compare {
                native_operations.compare = Some(declare_value_operation(
                    module,
                    "phrust_native_compare",
                    2,
                    helper_address("phrust_native_compare"),
                )?);
            }
            if needs_cast {
                native_operations.cast = Some(declare_value_operation(
                    module,
                    "phrust_native_cast",
                    1,
                    helper_address("phrust_native_cast"),
                )?);
            }
            if needs_echo {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.echo = Some(declare_native_helper(
                    module,
                    "phrust_native_echo",
                    &signature,
                    helper_address("phrust_native_echo"),
                )?);
            }
            if needs_local_fetch {
                native_operations.local_fetch = Some(declare_value_operation(
                    module,
                    "phrust_native_local_fetch",
                    5,
                    helper_address("phrust_native_local_fetch"),
                )?);
            }
            if needs_local_store {
                native_operations.local_store = Some(declare_value_operation(
                    module,
                    "phrust_native_local_store",
                    4,
                    helper_address("phrust_native_local_store"),
                )?);
            }
            if needs_value_lifecycle {
                native_operations.value_lifecycle = Some(declare_value_operation(
                    module,
                    "phrust_native_value_lifecycle",
                    1,
                    helper_address("phrust_native_value_lifecycle"),
                )?);
            }
            if needs_reference_bind {
                native_operations.reference_bind = Some(declare_value_operation(
                    module,
                    "phrust_native_reference_bind",
                    3,
                    helper_address("phrust_native_reference_bind"),
                )?);
            }
            if needs_argument_check {
                native_operations.argument_check = Some(declare_value_operation(
                    module,
                    "phrust_native_argument_check",
                    5,
                    helper_address("phrust_native_argument_check"),
                )?);
            }
            if needs_return_check {
                native_operations.return_check = Some(declare_value_operation(
                    module,
                    "phrust_native_return_check",
                    2,
                    helper_address("phrust_native_return_check"),
                )?);
            }
            if needs_exception_new {
                native_operations.exception_new = Some(declare_value_operation(
                    module,
                    "phrust_native_exception_new",
                    3,
                    helper_address("phrust_native_exception_new"),
                )?);
            }
            if needs_array_new {
                native_operations.array_new = Some(declare_value_operation(
                    module,
                    "phrust_native_array_new",
                    0,
                    helper_address("phrust_native_array_new"),
                )?);
            }
            if needs_object_new {
                native_operations.object_new = Some(declare_value_operation(
                    module,
                    "phrust_native_object_new",
                    0,
                    helper_address("phrust_native_object_new"),
                )?);
            }
            if needs_property_fetch {
                native_operations.property_fetch = Some(declare_value_operation(
                    module,
                    "phrust_native_property_fetch",
                    3,
                    helper_address("phrust_native_property_fetch"),
                )?);
            }
            if needs_property_assign {
                native_operations.property_assign = Some(declare_value_operation(
                    module,
                    "phrust_native_property_assign",
                    4,
                    helper_address("phrust_native_property_assign"),
                )?);
            }
            if needs_object_clone {
                native_operations.object_clone = Some(declare_value_operation(
                    module,
                    "phrust_native_object_clone",
                    1,
                    helper_address("phrust_native_object_clone"),
                )?);
            }
            if needs_object_clone_with {
                native_operations.object_clone_with = Some(declare_value_operation(
                    module,
                    "phrust_native_object_clone_with",
                    2,
                    helper_address("phrust_native_object_clone_with"),
                )?);
            }
            if needs_array_insert {
                native_operations.array_insert = Some(declare_value_operation(
                    module,
                    "phrust_native_array_insert",
                    3,
                    helper_address("phrust_native_array_insert"),
                )?);
            }
            if needs_array_fetch {
                native_operations.array_fetch = Some(declare_value_operation(
                    module,
                    "phrust_native_array_fetch",
                    2,
                    helper_address("phrust_native_array_fetch"),
                )?);
            }
            if needs_array_unset {
                native_operations.array_unset = Some(declare_value_operation(
                    module,
                    "phrust_native_array_unset",
                    2,
                    helper_address("phrust_native_array_unset"),
                )?);
            }
            if needs_array_spread {
                native_operations.array_spread = Some(declare_value_operation(
                    module,
                    "phrust_native_array_spread",
                    2,
                    helper_address("phrust_native_array_spread"),
                )?);
            }
            if needs_foreach_init {
                native_operations.foreach_init = Some(declare_value_operation(
                    module,
                    "phrust_native_foreach_init",
                    3,
                    helper_address("phrust_native_foreach_init"),
                )?);
            }
            if needs_foreach_next {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.foreach_next = Some(declare_native_helper(
                    module,
                    "phrust_native_foreach_next",
                    &signature,
                    helper_address("phrust_native_foreach_next"),
                )?);
            }
            if needs_foreach_cleanup {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.foreach_cleanup = Some(declare_native_helper(
                    module,
                    "phrust_native_foreach_cleanup",
                    &signature,
                    helper_address("phrust_native_foreach_cleanup"),
                )?);
            }
            if needs_constant_fetch {
                native_operations.constant_fetch = Some(declare_value_operation(
                    module,
                    "phrust_native_constant_fetch",
                    2,
                    helper_address("phrust_native_constant_fetch"),
                )?);
            }
            if needs_truthy {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.truthy = Some(declare_native_helper(
                    module,
                    "phrust_native_truthy",
                    &signature,
                    helper_address("phrust_native_truthy"),
                )?);
            }
            if needs_type_predicate {
                native_operations.type_predicate = Some(declare_value_operation(
                    module,
                    "phrust_native_type_predicate",
                    1,
                    helper_address("phrust_native_type_predicate"),
                )?);
            }
            if needs_stable_length {
                native_operations.stable_length = Some(declare_value_operation(
                    module,
                    "phrust_native_stable_length",
                    3,
                    helper_address("phrust_native_stable_length"),
                )?);
            }
            if needs_runtime_fatal {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.runtime_fatal = Some(declare_native_helper(
                    module,
                    "phrust_native_runtime_fatal",
                    &signature,
                    helper_address("phrust_native_runtime_fatal"),
                )?);
            }
            if needs_execution_poll {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.execution_poll = Some(declare_native_helper(
                    module,
                    "phrust_native_execution_poll",
                    &signature,
                    helper_address("phrust_native_execution_poll"),
                )?);
            }
            let mut functions = BTreeMap::new();
            for candidate in regions.values() {
                let symbol = if candidate.function == function {
                    name.to_owned()
                } else {
                    format!("{name}.callee.{}", candidate.function.raw())
                };
                let signature = region_graph_signature(module, candidate)?;
                let func_id = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare executable region {symbol}: {error}"),
                        )
                    })?;
                functions.insert(candidate.function, func_id);
            }
            let mut fragment_functions = BTreeMap::<u32, FuncId>::new();
            let mut fragment_symbols = BTreeMap::<u32, FunctionId>::new();
            if let Some(layout) = &fragment_layout {
                let synthetic_base = u32::try_from(unit.functions.len()).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                        "source unit function count does not fit the fragment symbol space",
                    )
                })?;
                for fragment in &layout.fragments {
                    let synthetic = FunctionId::new(
                        synthetic_base.checked_add(fragment.id).ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                                "native fragment symbol id overflowed",
                            )
                        })?,
                    );
                    let symbol = format!("{name}.fragment.{}", fragment.id);
                    let signature = region_fragment_signature(module, region)?;
                    let func_id = module
                        .declare_function(&symbol, Linkage::Local, &signature)
                        .map_err(|error| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_DECLARE_FRAGMENT",
                                format!("failed to declare native fragment {symbol}: {error}"),
                            )
                        })?;
                    fragment_functions.insert(fragment.id, func_id);
                    fragment_symbols.insert(fragment.id, synthetic);
                    functions.insert(synthetic, func_id);
                }
            }
            let inline_constants = collect_bounded_inline_values(unit, &regions);
            let tail_forwards = regions
                .values()
                .flat_map(|candidate| {
                    candidate.blocks.iter().filter_map(|block| {
                        let (continuation, target) =
                            bounded_tail_forward_target(candidate, block, &regions)?;
                        (!trampoline_functions.contains(&target))
                            .then_some(((candidate.function, continuation), target))
                    })
                })
                .collect::<BTreeMap<_, _>>();

            let mut code_bytes = 0_u64;
            let mut clif_blocks = 0_usize;
            let mut maximum_pre_regalloc = PreRegallocMetrics::default();
            let mut native_pc_ranges = Vec::new();
            let mut relocatable_bytes = Vec::new();
            let mut relocatable_functions = Vec::new();
            let mut relocatable_relocations = Vec::new();
            let mut function_code_metrics = BTreeMap::new();
            // Keep parameter metadata for every function in the source unit,
            // including callees deliberately omitted from a bounded local
            // call graph. The typed trampoline still needs the declared
            // by-reference contract for those functions; otherwise ordinary
            // lvalue arguments (such as `$this->property`) are conservatively
            // rebound as references before dispatch.
            let mut function_params = unit
                .functions
                .iter()
                .enumerate()
                .filter_map(|(index, function)| {
                    let function_id = u32::try_from(index).ok().map(FunctionId::new)?;
                    let native_arity =
                        crate::region_ir::native_function_parameter_locals(unit, function_id)?
                            .len();
                    Some((
                        function_id,
                        (
                            function.name.clone(),
                            function.params.clone(),
                            ir_function_requires_trampoline(function),
                            native_arity,
                        ),
                    ))
                })
                .collect::<BTreeMap<_, _>>();
            for (index, signature) in request.external_function_signatures.iter().enumerate() {
                let Ok(index) = u32::try_from(index) else {
                    break;
                };
                let function_id = FunctionId::new(u32::MAX.saturating_sub(index));
                if function_params.contains_key(&function_id) {
                    continue;
                }
                let params = signature
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| php_ir::IrParam {
                        name: parameter.name.clone(),
                        local: LocalId::new(u32::try_from(index).unwrap_or(u32::MAX)),
                        required: false,
                        default: None,
                        type_: None,
                        by_ref: parameter.by_ref,
                        variadic: parameter.variadic,
                        attributes: Vec::new(),
                    })
                    .collect::<Vec<_>>();
                function_params.insert(
                    function_id,
                    (signature.name.clone(), params, true, signature.params.len()),
                );
            }
            function_params.extend(regions.iter().map(|(function, region)| {
                (
                    *function,
                    (
                        unit.functions[function.index()].name.clone(),
                        region.params.clone(),
                        trampoline_functions.contains(function),
                        region.arity(),
                    ),
                )
            }));
            let mut append_defined = |symbol: FunctionId,
                                      arity: u8,
                                      local_count: u32,
                                      mut defined: DefinedRegionFunction|
             -> Result<(u64, u32), CraneliftLoweringError> {
                let alignment = usize::try_from(defined.alignment).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CACHE_ALIGNMENT",
                        "native function alignment does not fit usize",
                    )
                })?;
                let padding = if alignment == 0 {
                    0
                } else {
                    (alignment - relocatable_bytes.len() % alignment) % alignment
                };
                relocatable_bytes.resize(relocatable_bytes.len().saturating_add(padding), 0);
                let code_offset = relocatable_bytes.len() as u64;
                let candidate_bytes = defined.code.len() as u64;
                clif_blocks = clif_blocks.saturating_add(defined.clif_blocks);
                maximum_pre_regalloc.max_assign(defined.pre_regalloc);
                relocatable_bytes.extend_from_slice(&defined.code);
                for relocation in &mut defined.relocations {
                    relocation.offset = relocation.offset.saturating_add(code_offset);
                }
                relocatable_relocations.append(&mut defined.relocations);
                relocatable_functions.push(crate::JitRelocatableFunction {
                    function: symbol,
                    code_offset,
                    code_len: candidate_bytes,
                    arity,
                    local_count,
                });
                code_bytes = code_bytes.saturating_add(candidate_bytes);
                native_pc_ranges.append(&mut defined.native_pc_ranges);
                Ok((candidate_bytes, defined.native_stack_bytes))
            };
            // A compile group may contain many bounded native fragments. Reuse
            // Cranelift's allocation-heavy translation scratch sequentially;
            // `clear_context` preserves its backing allocations after every
            // fragment while regalloc still sees only one fragment at a time.
            for candidate in regions.values() {
                if let Some(layout) = &fragment_layout {
                    let mut function_bytes = 0_u64;
                    let mut maximum_stack = 0_u32;
                    for fragment in &layout.fragments {
                        let defined = define_region_graph_function(
                            module,
                            codegen_context,
                            builder_context,
                            candidate,
                            &unit.constants,
                            fragment_functions[&fragment.id],
                            &functions,
                            &inline_constants,
                            &tail_forwards,
                            &function_params,
                            native_call_helper,
                            native_dynamic_code_helper,
                            native_operations,
                            &suspending_functions,
                            Some(NativeFragmentDefinition {
                                layout,
                                fragment,
                                functions: &fragment_functions,
                            }),
                        )?;
                        let (bytes, stack) = append_defined(
                            fragment_symbols[&fragment.id],
                            0,
                            candidate.local_count,
                            defined,
                        )?;
                        function_bytes = function_bytes.saturating_add(bytes);
                        maximum_stack = maximum_stack.max(stack);
                    }
                    let wrapper = define_region_fragment_wrapper(
                        module,
                        codegen_context,
                        builder_context,
                        candidate,
                        functions[&candidate.function],
                        NativeFragmentWrapperDefinition {
                            functions: &fragment_functions,
                            layout,
                            relocation_functions: &functions,
                        },
                    )?;
                    let (bytes, stack) = append_defined(
                        candidate.function,
                        region_arity(candidate)?,
                        candidate.local_count,
                        wrapper,
                    )?;
                    function_bytes = function_bytes.saturating_add(bytes);
                    maximum_stack = maximum_stack.max(stack);
                    function_code_metrics
                        .insert(candidate.function, (function_bytes, maximum_stack));
                } else {
                    let defined = define_region_graph_function(
                        module,
                        codegen_context,
                        builder_context,
                        candidate,
                        &unit.constants,
                        functions[&candidate.function],
                        &functions,
                        &inline_constants,
                        &tail_forwards,
                        &function_params,
                        native_call_helper,
                        native_dynamic_code_helper,
                        native_operations,
                        &suspending_functions,
                        None,
                    )?;
                    let metrics = append_defined(
                        candidate.function,
                        region_arity(candidate)?,
                        candidate.local_count,
                        defined,
                    )?;
                    function_code_metrics.insert(candidate.function, metrics);
                }
            }
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                module
                    .finalize_definitions()
                    .map_err(|error| error.to_string())
            }))
            .map_err(|payload| {
                let message = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| {
                        payload
                            .downcast_ref::<&str>()
                            .map(|value| (*value).to_owned())
                    })
                    .unwrap_or_else(|| "Cranelift finalization panicked".to_owned());
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize executable region call graph: {message}"),
                )
            })?
            .map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize executable region call graph: {error}"),
                )
            })?;
            let function_entries = regions
                .values()
                .map(|candidate| {
                    let (function_code_bytes, native_stack_bytes) =
                        function_code_metrics[&candidate.function];
                    Ok(crate::JitNativeFunctionEntryMetadata {
                        function: candidate.function,
                        address: module.get_finalized_function(functions[&candidate.function])
                            as usize,
                        arity: region_arity(candidate)?,
                        code_bytes: function_code_bytes,
                        native_stack_bytes,
                        local_count: candidate.local_count,
                        direct_call_sites: candidate
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .filter(|instruction| {
                                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call)
                                if call.direct_compiled_target().is_some_and(|target| {
                                    (regions.contains_key(&target) || needs_function_resolver)
                                        && function_params
                                            .get(&target)
                                            .is_some_and(|(_, _, requires_trampoline, _)| {
                                                !requires_trampoline
                                            })
                                        && !matches!(
                                            call.result,
                                            RegionCallResult::ReferenceLocal(_)
                                        )
                                        && call.args.iter().all(|argument| {
                                            argument.name.is_none() && !argument.unpack
                                        })
                                        && !inline_constants
                                            .get(&target)
                                            .copied()
                                            .and_then(|value| {
                                                bounded_inline_call_operand(call, value)
                                            })
                                            .is_some()
                                }))
                            })
                            .count() as u64,
                        direct_method_call_sites: candidate
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .filter(|instruction| {
                                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call)
                                if call.argument_operand_offset == 1
                                    && call.direct_compiled_target().is_some_and(|target| {
                                        (regions.contains_key(&target) || needs_function_resolver)
                                            && function_params
                                                .get(&target)
                                                .is_some_and(|(_, _, requires_trampoline, _)| {
                                                    !requires_trampoline
                                                })
                                            && !matches!(
                                                call.result,
                                                RegionCallResult::ReferenceLocal(_)
                                            )
                                            && call.args.iter().all(|argument| {
                                                argument.name.is_none() && !argument.unpack
                                            })
                                    }))
                            })
                            .count() as u64,
                        inlined_call_sites: candidate
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .filter(|instruction| {
                                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call)
                                if call.direct_compiled_target().is_some_and(|target| {
                                    inline_constants
                                        .get(&target)
                                        .copied()
                                        .and_then(|value| {
                                            bounded_inline_call_operand(call, value)
                                        })
                                        .is_some()
                                }))
                            })
                            .count() as u64,
                        inline_bytes_added: candidate
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .filter(|instruction| {
                                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call)
                                if call.direct_compiled_target().is_some_and(|target| {
                                    inline_constants
                                        .get(&target)
                                        .copied()
                                        .and_then(|value| {
                                            bounded_inline_call_operand(call, value)
                                        })
                                        .is_some()
                                }))
                            })
                            .count() as u64
                            * 8,
                        tail_call_sites: tail_forwards
                            .keys()
                            .filter(|(function, _)| *function == candidate.function)
                            .count() as u64,
                        inline_rejected_by_reason: inline_rejection_counts(candidate, &regions),
                    })
                })
                .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
            let root = functions[&function];
            let address = module.get_finalized_function(root) as usize;
            let region_state_metadata = region_graph_metadata(
                function,
                region.local_count,
                regions.values(),
                native_pc_ranges,
                function_entries,
                &suspending_functions,
            );
            let mut handle = JitFunctionHandle::i64_status_out_native(
                u64::from(function.raw()) + 1,
                request.region_id.clone(),
                CraneliftCompilerIdentity,
                address,
                arity,
                code_bytes,
                0,
                fast_path_hits,
                region_state_metadata,
            );
            handle.bind_relocatable_code(crate::JitRelocatableCode {
                root: function,
                code: relocatable_bytes,
                functions: relocatable_functions,
                relocations: relocatable_relocations,
            });
            compiled_clif_blocks.set(Some(clif_blocks));
            compiled_maximum_pre_regalloc.set(Some(maximum_pre_regalloc));
            Ok((handle, code_bytes))
        },
    )?;
    let mut handle = compiled.handle;
    handle.bind_ssa_metrics(ssa_metrics.0, ssa_metrics.1, ssa_metrics.2);
    Ok(NativeScalarRegionCompileResult {
        handle,
        code_bytes: compiled.code_bytes,
        clif_blocks: compiled_clif_blocks.get(),
        maximum_pre_regalloc: compiled_maximum_pre_regalloc.get(),
        fast_path_hits,
        has_control_flow,
        plan,
    })
}

fn collect_region_graphs(
    root: &RegionGraph,
) -> Result<BTreeMap<FunctionId, RegionGraph>, CraneliftLoweringError> {
    let regions = BTreeMap::from([(root.function, root.clone())]);
    for region in regions.values() {
        validate_region_native_coverage(region)?;
        region.verify().map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_REGION_VERIFY", error.to_string())
        })?;
    }
    Ok(regions)
}

fn validate_region_native_coverage(region: &RegionGraph) -> Result<(), CraneliftLoweringError> {
    if region.local_count as usize > crate::JIT_DEOPT_MAX_SLOTS {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_MISSING_DEOPT_SLOT_LOWERING",
            format!(
                "function {} has {} locals; native state ABI supports {}",
                region.function_name,
                region.local_count,
                crate::JIT_DEOPT_MAX_SLOTS
            ),
        ));
    }
    for block in &region.blocks {
        for instruction in &block.instructions {
            if let RegionInstructionKind::CompileTimeFatal { diagnostic_id } = &instruction.kind {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_IR_COMPILE_FATAL",
                    format!(
                        "function={} diagnostic={} span={}:{}-{}",
                        region.function_name,
                        diagnostic_id,
                        instruction.span.file.raw(),
                        instruction.span.start,
                        instruction.span.end
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn region_arity(region: &RegionGraph) -> Result<u8, CraneliftLoweringError> {
    region.arity().try_into().map_err(|_| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_REGION_ARITY",
            "executable Region IR arity does not fit the native ABI",
        )
    })
}

fn region_instruction_result_register(kind: &RegionInstructionKind) -> Option<RegId> {
    match kind {
        RegionInstructionKind::Move { dst, .. }
        | RegionInstructionKind::LoadLocal { dst, .. }
        | RegionInstructionKind::AssignLocalResult { dst, .. }
        | RegionInstructionKind::Binary { dst, .. }
        | RegionInstructionKind::Unary { dst, .. }
        | RegionInstructionKind::Compare { dst, .. }
        | RegionInstructionKind::Cast { dst, .. }
        | RegionInstructionKind::NewArray { dst }
        | RegionInstructionKind::NewObject { dst, .. }
        | RegionInstructionKind::FetchProperty { dst, .. }
        | RegionInstructionKind::FetchDynamicStaticProperty { dst, .. }
        | RegionInstructionKind::FetchObjectClassName { dst, .. }
        | RegionInstructionKind::AssignProperty { dst, .. }
        | RegionInstructionKind::CloneObject { dst, .. }
        | RegionInstructionKind::CloneWith { dst, .. }
        | RegionInstructionKind::FetchDim { dst, .. }
        | RegionInstructionKind::FetchConst { dst }
        | RegionInstructionKind::AssignDim { dst, .. }
        | RegionInstructionKind::AppendDim { dst, .. }
        | RegionInstructionKind::IssetDim { dst, .. }
        | RegionInstructionKind::EmptyDim { dst, .. }
        | RegionInstructionKind::IssetLocal { dst, .. }
        | RegionInstructionKind::EmptyLocal { dst, .. }
        | RegionInstructionKind::ForeachInit { iterator: dst, .. }
        | RegionInstructionKind::ForeachInitRef { iterator: dst, .. }
        | RegionInstructionKind::ForeachNext { has_value: dst, .. }
        | RegionInstructionKind::ForeachNextRef { has_value: dst, .. } => Some(*dst),
        RegionInstructionKind::RuntimeFatal { dst: Some(dst), .. } => Some(*dst),
        RegionInstructionKind::NativeCall(RegionNativeCall {
            result: RegionCallResult::Register(dst),
            ..
        }) => Some(*dst),
        RegionInstructionKind::NativeControl(RegionNativeControl::MakeException {
            dst, ..
        }) => Some(*dst),
        RegionInstructionKind::NativeSuspend(
            RegionNativeSuspend::GeneratorYield { dst, .. }
            | RegionNativeSuspend::GeneratorDelegate { dst, .. }
            | RegionNativeSuspend::FiberSuspend { dst, .. },
        ) => Some(*dst),
        RegionInstructionKind::NativeDynamicCode(
            RegionNativeDynamicCode::Include { dst, .. }
            | RegionNativeDynamicCode::Eval { dst, .. }
            | RegionNativeDynamicCode::MakeClosure { dst, .. },
        ) => Some(*dst),
        RegionInstructionKind::Nop
        | RegionInstructionKind::StoreLocal { .. }
        | RegionInstructionKind::BindReference { .. }
        | RegionInstructionKind::BindReferenceDim { .. }
        | RegionInstructionKind::BindReferenceIntoDim { .. }
        | RegionInstructionKind::BindReferenceProperty { .. }
        | RegionInstructionKind::BindReferenceFromProperty { .. }
        | RegionInstructionKind::BindReferenceFromPropertyDim { .. }
        | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
        | RegionInstructionKind::BindReferenceDimFromProperty { .. }
        | RegionInstructionKind::BindReferenceStaticProperty { .. }
        | RegionInstructionKind::InitStaticLocal { .. }
        | RegionInstructionKind::Discard { .. }
        | RegionInstructionKind::Echo { .. }
        | RegionInstructionKind::ArrayInsert { .. }
        | RegionInstructionKind::ArraySpread { .. }
        | RegionInstructionKind::UnsetDim { .. }
        | RegionInstructionKind::UnsetLocal { .. }
        | RegionInstructionKind::ForeachCleanup { .. }
        | RegionInstructionKind::NativeCall(RegionNativeCall {
            result: RegionCallResult::ReferenceLocal(_) | RegionCallResult::Discard,
            ..
        })
        | RegionInstructionKind::NativeControl(_)
        | RegionInstructionKind::NativeDynamicCode(_)
        | RegionInstructionKind::RuntimeFatal { dst: None, .. }
        | RegionInstructionKind::CompileTimeFatal { .. } => None,
    }
}

fn region_register_types(region: &RegionGraph) -> BTreeMap<RegId, ir::Type> {
    region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .flat_map(|instruction| {
            let mut registers = region_instruction_result_register(&instruction.kind)
                .into_iter()
                .collect::<Vec<_>>();
            if let RegionInstructionKind::ForeachNext { key, value, .. } = &instruction.kind {
                registers.extend(*key);
                registers.push(*value);
            }
            registers.into_iter().map(|register| (register, types::I64))
        })
        .collect()
}

/// Deliberately tiny first inlining tier. It handles only a scalar constant
/// return or a simple untyped positional-argument wrapper. The callee body is
/// never recursively traversed, so code growth remains fixed per callsite.
fn bounded_inline_return(region: &RegionGraph) -> Option<BoundedInlineValue> {
    if region.return_type.is_some()
        || region.returns_by_ref
        || region.flags.is_method
        || region.flags.is_closure
        || region.flags.is_generator
        || region.blocks.len() != 1
    {
        return None;
    }
    let block = &region.blocks[0];
    let RegionTerminator::Return {
        value,
        finally: None,
    } = block.terminator
    else {
        return None;
    };
    match block.instructions.as_slice() {
        [] if region.params.is_empty()
            && matches!(value, RegionOperand::I64(_) | RegionOperand::Constant(_)) =>
        {
            Some(BoundedInlineValue::Constant(value))
        }
        [
            RegionInstruction {
                kind: RegionInstructionKind::Move { dst, src },
                ..
            },
        ] if value == RegionOperand::Register(*dst)
            && matches!(src, RegionOperand::I64(_) | RegionOperand::Constant(_)) =>
        {
            Some(BoundedInlineValue::Constant(*src))
        }
        [
            RegionInstruction {
                kind:
                    RegionInstructionKind::LoadLocal {
                        dst,
                        local,
                        quiet: false,
                    },
                ..
            },
        ] if value == RegionOperand::Register(*dst)
            && region.params.iter().all(|parameter| {
                parameter.required
                    && parameter.default.is_none()
                    && parameter.type_.is_none()
                    && !parameter.by_ref
                    && !parameter.variadic
            }) =>
        {
            region
                .parameter_locals
                .iter()
                .position(|parameter| parameter == local)
                .map(|index| BoundedInlineValue::Argument {
                    index,
                    arity: region.params.len(),
                })
        }
        _ => None,
    }
}

fn collect_bounded_inline_values(
    unit: &IrUnit,
    roots: &BTreeMap<FunctionId, RegionGraph>,
) -> BTreeMap<FunctionId, BoundedInlineValue> {
    if !roots
        .values()
        .any(|region| region.compile_metadata.tier == NativeCompilerTier::Optimizing)
    {
        return BTreeMap::new();
    }
    roots
        .values()
        .flat_map(RegionGraph::direct_callees)
        .filter(|callee| !roots.contains_key(callee))
        .filter_map(|callee| {
            crate::region_ir::build_baseline_region(unit, callee)
                .ok()
                .and_then(|region| bounded_inline_return(&region))
                .map(|value| (callee, value))
        })
        .collect()
}

fn bounded_inline_rejection(region: &RegionGraph) -> &'static str {
    if !region.params.is_empty() {
        "arguments"
    } else if region.flags.is_method || region.flags.is_closure {
        "receiver-or-closure-environment"
    } else if region.flags.is_generator {
        "suspension"
    } else if region.return_type.is_some() {
        "return-type-check"
    } else if region.blocks.len() != 1 {
        "control-flow-complexity"
    } else {
        "not-constant-wrapper"
    }
}

fn inline_rejection_counts(
    caller: &RegionGraph,
    regions: &BTreeMap<FunctionId, RegionGraph>,
) -> BTreeMap<String, u64> {
    let mut reasons = BTreeMap::new();
    for call in caller
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
    {
        let Some(target) = call.direct_compiled_target() else {
            continue;
        };
        let Some(callee) = regions.get(&target) else {
            continue;
        };
        if bounded_inline_return(callee)
            .and_then(|value| bounded_inline_call_operand(call, value))
            .is_some()
        {
            continue;
        }
        let reason = if call.operands.is_empty() {
            bounded_inline_rejection(callee)
        } else {
            "arguments-or-receiver"
        };
        let count = reasons.entry(reason.to_owned()).or_insert(0_u64);
        *count = count.saturating_add(1);
    }
    reasons
}

/// Selects the deliberately small tail-call subset whose callee can consume
/// the caller's packed argument buffer directly. This avoids allocating a
/// second arena frame and transfers the caller's argument ownership exactly
/// once. More general tail calls need an owned-frame transfer protocol.
fn bounded_tail_forward_target(
    region: &RegionGraph,
    block: &crate::region_ir::RegionBlock,
    regions: &BTreeMap<FunctionId, RegionGraph>,
) -> Option<(u32, FunctionId)> {
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (region, block, regions);
        return None;
    }

    #[cfg(target_arch = "x86_64")]
    {
        let RegionTerminator::Return {
            value: RegionOperand::Register(returned),
            finally: None,
        } = &block.terminator
        else {
            return None;
        };
        let (last, prefix) = block.instructions.split_last()?;
        let RegionInstructionKind::NativeCall(call) = &last.kind else {
            return None;
        };
        let RegionCallResult::Register(destination) = call.result else {
            return None;
        };
        let target = call.direct_compiled_target()?;
        let callee = regions.get(&target)?;
        if destination != *returned
            || target == region.function
            || call.argument_operand_offset != 0
            || call.variadic
            || call.returns_by_reference
            || region.returns_by_ref
            || callee.returns_by_ref
            || region.params != callee.params
            || region.return_type != callee.return_type
            || !region.exception_regions.is_empty()
            || !callee.exception_regions.is_empty()
            || region.flags.is_generator
            || region.flags.is_closure
            || region.flags.is_method
            || callee.flags.is_generator
            || callee.flags.is_closure
            || callee.flags.is_method
            || prefix.len() != region.parameter_locals.len()
            || call.operands.len() != region.parameter_locals.len()
            || !callee
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .all(|instruction| {
                    matches!(
                        instruction.kind,
                        RegionInstructionKind::Nop
                            | RegionInstructionKind::Move { .. }
                            | RegionInstructionKind::LoadLocal { .. }
                    )
                })
        {
            return None;
        }
        for (((instruction, local), operand), parameter) in prefix
            .iter()
            .zip(&region.parameter_locals)
            .zip(&call.operands)
            .zip(&call.args)
        {
            let RegionInstructionKind::LoadLocal {
                dst,
                local: loaded,
                quiet: false,
            } = &instruction.kind
            else {
                return None;
            };
            if *loaded != *local
                || *operand != Some(RegionOperand::Register(*dst))
                || parameter.name.is_some()
                || parameter.unpack
                || parameter.by_ref_local.is_some()
                || parameter.by_ref_dim.is_some()
                || parameter.by_ref_property.is_some()
                || parameter.by_ref_property_dim.is_some()
            {
                return None;
            }
        }
        Some((last.continuation_id, target))
    }
}

fn region_graph_signature(
    module: &JITModule,
    region: &RegionGraph,
) -> Result<Signature, CraneliftLoweringError> {
    region_arity(region)?;
    Ok(native_php_entry_signature(module))
}

fn region_fragment_signature(
    module: &JITModule,
    region: &RegionGraph,
) -> Result<Signature, CraneliftLoweringError> {
    region_arity(region)?;
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    #[cfg(target_arch = "x86_64")]
    {
        signature.call_conv = CallConv::Tail;
    }
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    Ok(signature)
}

struct DefinedRegionFunction {
    code: Vec<u8>,
    clif_blocks: usize,
    alignment: u64,
    relocations: Vec<crate::JitRelocatableRelocation>,
    native_pc_ranges: Vec<crate::JitNativePcRange>,
    native_stack_bytes: u32,
    pre_regalloc: PreRegallocMetrics,
}

const MAX_NATIVE_SPILL_FRAME_BYTES: u32 = 1024 * 1024;
const MAX_FRAGMENT_CLIF_BLOCKS: usize = 2_048;
const MAX_FRAGMENT_CLIF_VALUES: usize = 32_768;
const MAX_FRAGMENT_CLIF_INSTRUCTIONS: usize = 65_536;
const MAX_FRAGMENT_BLOCK_PARAMETERS: usize = 8_192;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct PreRegallocMetrics {
    pub(super) blocks: usize,
    pub(super) values: usize,
    pub(super) instructions: usize,
    pub(super) block_parameters: usize,
}

impl PreRegallocMetrics {
    fn max_assign(&mut self, other: Self) {
        self.blocks = self.blocks.max(other.blocks);
        self.values = self.values.max(other.values);
        self.instructions = self.instructions.max(other.instructions);
        self.block_parameters = self.block_parameters.max(other.block_parameters);
    }
}

pub(super) fn validate_pre_regalloc_structure(
    function: &ir::Function,
    region: &RegionGraph,
    fragment: Option<u32>,
) -> Result<PreRegallocMetrics, CraneliftLoweringError> {
    let blocks = function.layout.blocks().count();
    let values = function.dfg.num_values();
    let instructions = function
        .layout
        .blocks()
        .map(|block| function.layout.block_insts(block).count())
        .sum::<usize>();
    let block_parameters = function
        .layout
        .blocks()
        .map(|block| function.dfg.block_params(block).len())
        .sum::<usize>();
    if blocks > MAX_FRAGMENT_CLIF_BLOCKS
        || values > MAX_FRAGMENT_CLIF_VALUES
        || instructions > MAX_FRAGMENT_CLIF_INSTRUCTIONS
        || block_parameters > MAX_FRAGMENT_BLOCK_PARAMETERS
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_PRE_REGALLOC_BUDGET",
            format!(
                "function {} fragment={} exceeds the pre-regalloc ceiling: clif_blocks={blocks}/{MAX_FRAGMENT_CLIF_BLOCKS} clif_values={values}/{MAX_FRAGMENT_CLIF_VALUES} clif_instructions={instructions}/{MAX_FRAGMENT_CLIF_INSTRUCTIONS} block_parameters={block_parameters}/{MAX_FRAGMENT_BLOCK_PARAMETERS}",
                region.function_name,
                fragment.map_or_else(|| "whole".to_owned(), |id| id.to_string()),
            ),
        ));
    }
    Ok(PreRegallocMetrics {
        blocks,
        values,
        instructions,
        block_parameters,
    })
}

fn define_region_fragment_wrapper(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    region: &RegionGraph,
    func_id: FuncId,
    definition: NativeFragmentWrapperDefinition<'_>,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    ctx.func.signature = region_graph_signature(module, region)?;
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let params = builder.block_params(entry).to_vec();
        let arguments = params[0];
        let result_out = params[1];
        let deopt_out = params[2];
        let resume_id = params[3];
        let resume_state = params[4];
        let frame_bytes = native_fragment_frame_bytes(region)?;
        let frame_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            frame_bytes,
            3,
        ));
        let frame = builder.ins().stack_addr(pointer_type, frame_slot, 0);
        let uninitialized = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        for local in 0..region.local_count {
            builder.ins().store(
                MemFlagsData::new(),
                uninitialized,
                frame,
                native_fragment_local_offset(LocalId::new(local)),
            );
        }
        for (index, local) in region.parameter_locals.iter().enumerate() {
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                arguments,
                i32::try_from(index.saturating_mul(8)).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_ARITY",
                        "fragment wrapper argument offset does not fit the native ABI",
                    )
                })?,
            );
            builder.ins().store(
                MemFlagsData::new(),
                value,
                frame,
                native_fragment_local_offset(*local),
            );
        }
        let continue_status = builder
            .ins()
            .iconst(types::I32, i64::from(crate::JitCallStatus::CONTINUE.0));
        let empty = builder.ins().iconst(types::I64, 0);
        builder.ins().store(
            MemFlagsData::new(),
            continue_status,
            frame,
            native_fragment_pending_status_offset(region),
        );
        builder.ins().store(
            MemFlagsData::new(),
            empty,
            frame,
            native_fragment_pending_value_offset(region),
        );
        for (value, offset) in [
            (arguments, native_fragment_arguments_offset(region)),
            (result_out, native_fragment_result_out_offset(region)),
            (deopt_out, native_fragment_deopt_out_offset(region)),
            (resume_state, native_fragment_resume_state_offset(region)),
        ] {
            builder
                .ins()
                .store(MemFlagsData::new(), value, frame, offset);
        }
        builder.ins().store(
            MemFlagsData::new(),
            resume_id,
            frame,
            native_fragment_resume_id_offset(region),
        );

        let call_blocks = definition
            .layout
            .fragments
            .iter()
            .map(|fragment| (fragment.id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let root_entry = builder.create_block();
        let mut resume_switch = Switch::new();
        for (encoded_resume, fragment_id) in &definition.layout.resume_owner {
            resume_switch.set_entry(u128::from(*encoded_resume as u32), call_blocks[fragment_id]);
        }
        resume_switch.emit(&mut builder, resume_id, root_entry);
        builder.switch_to_block(root_entry);
        let root_fragment = definition.layout.block_owner[&BlockId::new(0)];
        builder.ins().jump(call_blocks[&root_fragment], &[]);

        for fragment in &definition.layout.fragments {
            builder.switch_to_block(call_blocks[&fragment.id]);
            let callee =
                module.declare_func_in_func(definition.functions[&fragment.id], builder.func);
            let entry_block = fragment
                .normal_entries
                .iter()
                .next()
                .copied()
                .unwrap_or(BlockId::new(0));
            let entry_id = builder
                .ins()
                .iconst(types::I32, i64::from(entry_block.raw()));
            builder.ins().store(
                MemFlagsData::new(),
                entry_id,
                frame,
                native_fragment_entry_id_offset(region),
            );
            let call = builder.ins().call(callee, &[frame]);
            let status = builder.inst_results(call)[0];
            builder.ins().return_(&[status]);
        }
        builder.seal_all_blocks();
        builder.finalize();
    }
    let pre_regalloc = validate_pre_regalloc_structure(&ctx.func, region, None)?;
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_FRAGMENT_WRAPPER",
            format!("Cranelift verifier rejected fragment wrapper: {error}"),
        )
    })?;
    let clif_blocks = ctx.func.layout.blocks().count();
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_FRAGMENT_WRAPPER",
            format!("failed to define native fragment wrapper: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_FRAGMENT_WRAPPER",
            "Cranelift returned no fragment-wrapper machine code",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |frame| frame.frame_to_fp_offset);
    if native_stack_bytes > MAX_NATIVE_SPILL_FRAME_BYTES {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_FRAGMENT_WRAPPER_STACK_LIMIT",
            format!(
                "fragment wrapper requires {native_stack_bytes} native stack bytes; limit is {MAX_NATIVE_SPILL_FRAME_BYTES}"
            ),
        ));
    }
    let code = compiled.code_buffer().to_vec();
    let alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    let relocations = compiled
        .buffer
        .relocs()
        .iter()
        .map(|relocation| {
            capture_relocation(
                module,
                ModuleReloc::from_mach_reloc(relocation, &ctx.func, func_id),
                definition.relocation_functions,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        code,
        clif_blocks,
        alignment,
        relocations,
        native_pc_ranges: Vec::new(),
        native_stack_bytes,
        pre_regalloc,
    })
}

fn supported_relocation_kind(kind: Reloc) -> Option<crate::JitRelocatableKind> {
    match kind {
        Reloc::Abs8 => Some(crate::JitRelocatableKind::Abs64),
        Reloc::X86PCRel4 => Some(crate::JitRelocatableKind::X86PcRel4),
        Reloc::X86CallPCRel4 | Reloc::X86CallPLTRel4 => {
            Some(crate::JitRelocatableKind::X86CallPcRel4)
        }
        Reloc::Arm64Call => Some(crate::JitRelocatableKind::Arm64Call),
        _ => None,
    }
}

fn stable_helper_import_name(name: &str) -> String {
    #[cfg(test)]
    {
        if let Some((base, suffix)) = name.rsplit_once('_')
            && suffix.len() == 16
            && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return base.to_owned();
        }
    }
    name.to_owned()
}

fn capture_relocation(
    module: &JITModule,
    relocation: ModuleReloc,
    functions: &BTreeMap<FunctionId, FuncId>,
) -> Result<crate::JitRelocatableRelocation, CraneliftLoweringError> {
    let kind = supported_relocation_kind(relocation.kind).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_RELOCATION",
            format!(
                "Cranelift emitted unsupported restart-cache relocation {:?}",
                relocation.kind
            ),
        )
    })?;
    let internal_function = |func_id: FuncId| {
        functions
            .iter()
            .find_map(|(function, candidate)| (*candidate == func_id).then_some(*function))
    };
    let (target, extra_addend) = match relocation.name {
        ModuleRelocTarget::User {
            namespace: 0,
            index,
        } => {
            let func_id = FuncId::from_u32(index);
            if let Some(function) = internal_function(func_id) {
                (crate::JitRelocatableTarget::InternalFunction(function), 0)
            } else {
                let declaration = module.declarations().get_function_decl(func_id);
                if declaration.linkage != Linkage::Import {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CACHE_SYMBOL",
                        format!("relocation target {func_id} is neither graph-local nor imported"),
                    ));
                }
                let name = declaration.name.as_deref().ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CACHE_SYMBOL",
                        format!("imported relocation target {func_id} has no stable name"),
                    )
                })?;
                (
                    crate::JitRelocatableTarget::Helper(stable_helper_import_name(name)),
                    0,
                )
            }
        }
        ModuleRelocTarget::FunctionOffset(func_id, offset) => {
            let function = internal_function(func_id).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CACHE_SYMBOL",
                    format!("function-offset relocation target {func_id} is not graph-local"),
                )
            })?;
            (
                crate::JitRelocatableTarget::InternalFunction(function),
                i64::from(offset),
            )
        }
        other => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_CACHE_SYMBOL",
                format!("unsupported restart-cache relocation target {other}"),
            ));
        }
    };
    Ok(crate::JitRelocatableRelocation {
        offset: u64::from(relocation.offset),
        kind,
        target,
        addend: relocation.addend.saturating_add(extra_addend),
    })
}

#[allow(clippy::too_many_arguments)]
fn define_region_graph_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    region: &RegionGraph,
    constants: &[IrConstant],
    func_id: FuncId,
    functions: &BTreeMap<FunctionId, FuncId>,
    inline_constants: &BTreeMap<FunctionId, BoundedInlineValue>,
    tail_forwards: &BTreeMap<(FunctionId, u32), FunctionId>,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    native_call_helper: Option<NativeHelper>,
    native_dynamic_code_helper: Option<NativeHelper>,
    native_operations: NativeOperationFunctions,
    suspending_functions: &BTreeSet<FunctionId>,
    fragment: Option<NativeFragmentDefinition<'_>>,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    let value_flow = if region.compile_metadata.tier == NativeCompilerTier::Optimizing {
        let flow = crate::region_ir::analyze_executable_value_flow(region, constants);
        flow.verify_ownership(region).map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_OWNERSHIP", error)
        })?;
        flow
    } else {
        crate::region_ir::analyze_baseline_executable_ownership(region)
    };
    let pointer_type = module.target_config().pointer_type();
    ctx.func.signature = if fragment.is_some() {
        region_fragment_signature(module, region)?
    } else {
        region_graph_signature(module, region)?
    };
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        let owned_blocks = region
            .blocks
            .iter()
            .filter(|block| {
                fragment.is_none_or(|fragment| fragment.fragment.blocks.contains(&block.id))
            })
            .collect::<Vec<_>>();
        let blocks = if let Some(fragment) = fragment {
            fragment
                .fragment
                .blocks
                .iter()
                .chain(&fragment.fragment.external_targets)
                .map(|block| (*block, builder.create_block()))
                .collect::<BTreeMap<_, _>>()
        } else {
            create_region_cranelift_blocks(&mut builder, region)?
        };
        // Only true resumable native transitions need an instruction-entry
        // block. Ordinary Region instructions are lowered directly into their
        // PHP CFG block (or the continuation block created by a fallible
        // helper). Creating an entry block for every instruction turns a
        // large but ordinary PHP function into a pathological Cranelift CFG
        // before regalloc2 sees it.
        let transition_blocks = owned_blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                instruction_has_native_transition(instruction, suspending_functions)
            })
            .map(|instruction| (instruction.continuation_id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let suspension_blocks = owned_blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_))
            })
            .map(|instruction| (instruction.continuation_id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let terminal_exit = builder.create_block();
        builder.set_cold_block(terminal_exit);
        builder.append_block_param(terminal_exit, types::I32);
        builder.append_block_param(terminal_exit, types::I64);
        let normal_entry = blocks.values().next().copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_CONTROL_FLOW",
                "executable region requires at least one block",
            )
        })?;
        let native_entry = builder.create_block();
        builder.append_block_params_for_function_params(native_entry);
        builder.switch_to_block(native_entry);
        let params = builder.block_params(native_entry).to_vec();
        let fragment_frame = fragment.map(|_| params[0]);
        let (arguments, result_out, deopt_out, resume_id, resume_state, fragment_entry_id) =
            if let Some(frame) = fragment_frame {
                let arguments = builder.ins().load(
                    pointer_type,
                    MemFlagsData::new(),
                    frame,
                    native_fragment_arguments_offset(region),
                );
                let result_out = builder.ins().load(
                    pointer_type,
                    MemFlagsData::new(),
                    frame,
                    native_fragment_result_out_offset(region),
                );
                let deopt_out = builder.ins().load(
                    pointer_type,
                    MemFlagsData::new(),
                    frame,
                    native_fragment_deopt_out_offset(region),
                );
                let resume_id = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    frame,
                    native_fragment_resume_id_offset(region),
                );
                let resume_state = builder.ins().load(
                    pointer_type,
                    MemFlagsData::new(),
                    frame,
                    native_fragment_resume_state_offset(region),
                );
                let entry_id = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    frame,
                    native_fragment_entry_id_offset(region),
                );
                (
                    arguments,
                    result_out,
                    deopt_out,
                    resume_id,
                    resume_state,
                    Some(entry_id),
                )
            } else {
                (params[0], params[1], params[2], params[3], params[4], None)
            };
        let native_operations = native_operations.with_terminal_exit(NativeTerminalExit {
            block: terminal_exit,
        });
        let mut locals = BTreeMap::new();
        for local_index in 0..region.local_count {
            locals.insert(LocalId::new(local_index), builder.declare_var(types::I64));
        }
        let register_types = region_register_types(region);
        let register_live_in = native_register_live_in(region);
        let transition_register_liveness =
            native_transition_register_liveness(region, suspending_functions);
        let register_variables = (0..region.register_count)
            .map(|index| {
                let register = RegId::new(index);
                let type_ = register_types.get(&register).copied().unwrap_or(types::I64);
                (register, builder.declare_var(type_))
            })
            .collect::<BTreeMap<_, _>>();
        let pending_status = builder.declare_var(types::I32);
        let pending_value = builder.declare_var(types::I64);
        let continue_status = builder
            .ins()
            .iconst(types::I32, i64::from(crate::JitCallStatus::CONTINUE.0));
        let empty_value = builder.ins().iconst(types::I64, 0);
        let native_version =
            u32::from(region.compile_metadata.tier == NativeCompilerTier::Optimizing);
        builder.def_var(pending_status, continue_status);
        builder.def_var(pending_value, empty_value);
        if let Some(frame) = fragment_frame {
            let status = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                frame,
                native_fragment_pending_status_offset(region),
            );
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                frame,
                native_fragment_pending_value_offset(region),
            );
            builder.def_var(pending_status, status);
            builder.def_var(pending_value, value);
        }
        let uninitialized_value = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        for variable in locals.values().copied() {
            builder.def_var(variable, uninitialized_value);
        }
        if fragment.is_none() {
            for (index, param) in region.parameter_locals.iter().enumerate() {
                let value = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    arguments,
                    i32::try_from(index.saturating_mul(8)).map_err(|_| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_REGION_ARITY",
                            "packed region argument offset does not fit the native ABI",
                        )
                    })?,
                );
                builder.def_var(local_variable(&locals, *param)?, value);
            }
        }
        let handler_resume_blocks = region
            .exception_regions
            .iter()
            .flat_map(|handler| [handler.catch, handler.finally])
            .flatten()
            .filter(|target| {
                fragment.is_none_or(|fragment| fragment.fragment.blocks.contains(target))
            })
            .collect::<std::collections::BTreeSet<_>>();
        let handler_exception_locals = region
            .exception_regions
            .iter()
            .filter_map(|handler| Some((handler.catch?, handler.exception_local?)))
            .fold(
                BTreeMap::<BlockId, std::collections::BTreeSet<LocalId>>::new(),
                |mut locals, (block, local)| {
                    locals.entry(block).or_default().insert(local);
                    locals
                },
            );
        let handler_resume_loaders = handler_resume_blocks
            .iter()
            .map(|target| (*target, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let suspension_resume_loaders = owned_blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_))
            })
            .map(|instruction| (instruction.continuation_id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let transition_resume_loaders = owned_blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                transition_register_liveness
                    .get(&instruction.continuation_id)
                    .is_some_and(|registers| {
                        instruction_has_native_transition(instruction, suspending_functions)
                            && registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS
                    })
            })
            .map(|instruction| (instruction.continuation_id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let osr_entries = region
            .osr_entries()
            .into_iter()
            .filter(|entry| {
                fragment.is_none_or(|fragment| fragment.fragment.blocks.contains(&entry.block))
            })
            .collect::<Vec<_>>();
        let osr_resume_loaders = osr_entries
            .iter()
            .map(|entry| (entry.id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let has_resume_entries = !handler_resume_loaders.is_empty()
            || !suspension_resume_loaders.is_empty()
            || !transition_resume_loaders.is_empty()
            || !osr_resume_loaders.is_empty();
        let resume_default = has_resume_entries.then(|| builder.create_block());
        let mut resume_switch = Switch::new();
        for (target, loader) in &handler_resume_loaders {
            resume_switch.set_entry(
                u128::from(crate::native_handler_resume_id(*target) as u32),
                *loader,
            );
        }
        for (continuation, loader) in &suspension_resume_loaders {
            resume_switch.set_entry(
                u128::from(crate::native_suspension_resume_id(*continuation) as u32),
                *loader,
            );
        }
        for (continuation, loader) in &transition_resume_loaders {
            resume_switch.set_entry(
                u128::from(crate::native_transition_resume_id(*continuation) as u32),
                *loader,
            );
        }
        for (id, loader) in &osr_resume_loaders {
            resume_switch.set_entry(u128::from(*id), *loader);
        }
        if let Some(resume_default) = resume_default {
            resume_switch.emit(&mut builder, resume_id, resume_default);
        }

        for target in handler_resume_blocks {
            let loader = handler_resume_loaders[&target];
            builder.switch_to_block(loader);
            let status = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                resume_state,
                std::mem::offset_of!(crate::JitDeoptState, control_status) as i32,
            );
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                resume_state,
                std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
            );
            builder.def_var(pending_status, status);
            builder.def_var(pending_value, value);
            let target_block = region.blocks.get(target.index()).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_HANDLER",
                    format!("native handler block {} is missing", target.raw()),
                )
            })?;
            let resume_locals = target_block
                .entry_live_locals
                .iter()
                .copied()
                .chain(
                    handler_exception_locals
                        .get(&target)
                        .into_iter()
                        .flatten()
                        .copied(),
                )
                .collect::<std::collections::BTreeSet<_>>();
            for local in resume_locals {
                let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                    .saturating_add(local.index().saturating_mul(8));
                let local_value = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    resume_state,
                    offset as i32,
                );
                builder.def_var(local_variable(&locals, local)?, local_value);
            }
            builder.ins().jump(cranelift_block(&blocks, target)?, &[]);
        }
        for region_block in &owned_blocks {
            for instruction in &region_block.instructions {
                if !matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_)) {
                    continue;
                }
                let loader = suspension_resume_loaders[&instruction.continuation_id];
                builder.switch_to_block(loader);
                let control_status = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    resume_state,
                    std::mem::offset_of!(crate::JitDeoptState, control_status) as i32,
                );
                let control_value = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    resume_state,
                    std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
                );
                builder.def_var(pending_status, control_status);
                builder.def_var(pending_value, control_value);
                for local in &instruction.live_locals {
                    let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                        .saturating_add(local.index().saturating_mul(8));
                    let value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        resume_state,
                        offset as i32,
                    );
                    builder.def_var(local_variable(&locals, *local)?, value);
                }
                builder.ins().jump(
                    *suspension_blocks
                        .get(&instruction.continuation_id)
                        .expect("suspension block was predeclared"),
                    &[],
                );
            }
        }
        for region_block in &owned_blocks {
            for instruction in &region_block.instructions {
                if let Some(live_registers) = transition_register_liveness
                    .get(&instruction.continuation_id)
                    .filter(|_| {
                        instruction_has_native_transition(instruction, suspending_functions)
                    })
                    .filter(|registers| registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
                {
                    let loader = transition_resume_loaders[&instruction.continuation_id];
                    builder.switch_to_block(loader);
                    let control_status = builder.ins().load(
                        types::I32,
                        MemFlagsData::new(),
                        resume_state,
                        std::mem::offset_of!(crate::JitDeoptState, control_status) as i32,
                    );
                    let control_value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        resume_state,
                        std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
                    );
                    builder.def_var(pending_status, control_status);
                    builder.def_var(pending_value, control_value);
                    for local in &instruction.live_locals {
                        let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                            .saturating_add(local.index().saturating_mul(8));
                        let value = builder.ins().load(
                            types::I64,
                            MemFlagsData::new(),
                            resume_state,
                            offset as i32,
                        );
                        builder.def_var(local_variable(&locals, *local)?, value);
                    }
                    for (snapshot_slot, register) in live_registers.iter().enumerate() {
                        let variable = register_variables[register];
                        let type_ = register_types.get(register).copied().unwrap_or(types::I64);
                        let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
                            .saturating_add(snapshot_slot.saturating_mul(8));
                        let value = builder.ins().load(
                            types::I64,
                            MemFlagsData::new(),
                            resume_state,
                            offset as i32,
                        );
                        let value = if type_ == types::I64 {
                            value
                        } else {
                            builder.ins().ireduce(type_, value)
                        };
                        builder.def_var(variable, value);
                    }
                    builder
                        .ins()
                        .jump(transition_blocks[&instruction.continuation_id], &[]);
                }
            }
        }
        for osr_entry in &osr_entries {
            let loader = osr_resume_loaders[&osr_entry.id];
            builder.switch_to_block(loader);
            for local in &osr_entry.live_locals {
                let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                    .saturating_add(local.index().saturating_mul(8));
                let value = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    resume_state,
                    offset as i32,
                );
                builder.def_var(local_variable(&locals, *local)?, value);
            }
            builder
                .ins()
                .jump(cranelift_block(&blocks, osr_entry.block)?, &[]);
        }
        if let Some(resume_default) = resume_default {
            builder.switch_to_block(resume_default);
        }
        if let Some(fragment) = fragment {
            let frame = fragment_frame.expect("fragment signature has a native frame");
            let entry_id = fragment_entry_id.expect("fragment signature has an entry id");
            let invalid_entry = builder.create_block();
            let entry_loaders = fragment
                .fragment
                .normal_entries
                .iter()
                .map(|entry| (*entry, builder.create_block()))
                .collect::<BTreeMap<_, _>>();
            let mut entry_switch = Switch::new();
            for (entry, loader) in &entry_loaders {
                entry_switch.set_entry(u128::from(entry.raw()), *loader);
            }
            entry_switch.emit(&mut builder, entry_id, invalid_entry);
            for entry in &fragment.fragment.normal_entries {
                let loader = entry_loaders[entry];
                builder.switch_to_block(loader);
                let entry_block = region.blocks.get(entry.index()).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_ENTRY",
                        format!("fragment entry block {} is missing", entry.raw()),
                    )
                })?;
                let mut entry_locals = entry_block
                    .entry_state_locals
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>();
                if entry.raw() == 0 {
                    entry_locals.extend(region.parameter_locals.iter().copied());
                }
                for local in entry_locals {
                    let value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        frame,
                        native_fragment_local_offset(local),
                    );
                    builder.def_var(local_variable(&locals, local)?, value);
                }
                for register in register_live_in.get(entry).into_iter().flatten() {
                    let type_ = register_types.get(register).copied().unwrap_or(types::I64);
                    let value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        frame,
                        native_fragment_register_offset(region, *register),
                    );
                    let value = if type_ == types::I64 {
                        value
                    } else {
                        builder.ins().ireduce(type_, value)
                    };
                    builder.def_var(register_variables[register], value);
                }
                builder.ins().jump(cranelift_block(&blocks, *entry)?, &[]);
            }
            builder.switch_to_block(invalid_entry);
            builder.set_cold_block(invalid_entry);
            let invalid = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::RUNTIME_ERROR.0));
            builder.ins().return_(&[invalid]);
        } else {
            builder.ins().jump(normal_entry, &[]);
        }

        let loop_headers = region
            .osr_entries()
            .into_iter()
            .filter(|entry| {
                fragment.is_none_or(|fragment| fragment.fragment.blocks.contains(&entry.block))
            })
            .map(|entry| entry.block)
            .collect::<BTreeSet<_>>();

        for region_block in &owned_blocks {
            let mut registers = register_variables.clone();
            builder.switch_to_block(cranelift_block(&blocks, region_block.id)?);
            if loop_headers.contains(&region_block.id)
                && let Some(helper) = native_operations.execution_poll
            {
                let count = builder.create_block();
                let poll = builder.create_block();
                let continue_loop = builder.create_block();
                let view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
                let counter = builder.ins().load(
                    pointer_type,
                    MemFlagsData::new(),
                    deopt_out,
                    view_offset
                        + std::mem::offset_of!(crate::JitNativeRuntimeView, poll_counter) as i32,
                );
                let has_counter = builder.ins().icmp_imm(IntCC::NotEqual, counter, 0);
                builder.ins().brif(has_counter, count, &[], poll, &[]);

                builder.switch_to_block(count);
                let visits = builder
                    .ins()
                    .load(types::I32, MemFlagsData::new(), counter, 0);
                let visits = builder.ins().iadd_imm(visits, 1);
                builder.ins().store(MemFlagsData::new(), visits, counter, 0);
                let cadence = builder.ins().band_imm(visits, 63);
                let due = builder.ins().icmp_imm(IntCC::Equal, cadence, 0);
                builder.ins().brif(due, poll, &[], continue_loop, &[]);

                builder.switch_to_block(poll);
                let context = builder.ins().iconst(types::I64, 0);
                let call = call_native_helper(module, &mut builder, helper, &[context]);
                let status = builder.inst_results(call)[0];
                require_native_operation_ok(&mut builder, status, helper.terminal_exit()?)?;
                builder.ins().jump(continue_loop, &[]);

                builder.switch_to_block(continue_loop);
            }
            let mut terminated = false;
            for instruction in &region_block.instructions {
                if let Some(transition_block) =
                    transition_blocks.get(&instruction.continuation_id).copied()
                {
                    builder.ins().jump(transition_block, &[]);
                    builder.switch_to_block(transition_block);
                }
                builder.set_srcloc(ir::SourceLoc::new(
                    instruction.continuation_id.saturating_add(1),
                ));
                if let Some(target) = tail_forwards
                    .get(&(region.function, instruction.continuation_id))
                    .and_then(|target| functions.get(target))
                {
                    let callee = module.declare_func_in_func(*target, builder.func);
                    builder.ins().return_call(
                        callee,
                        &[arguments, result_out, deopt_out, resume_id, resume_state],
                    );
                    terminated = true;
                    break;
                }
                lower_region_instruction(
                    module,
                    &mut builder,
                    functions,
                    inline_constants,
                    function_params,
                    native_call_helper,
                    native_dynamic_code_helper,
                    native_operations,
                    &register_variables,
                    &blocks,
                    &suspension_blocks,
                    &locals,
                    &mut registers,
                    region_block.source_block,
                    instruction,
                    transition_register_liveness
                        .get(&instruction.continuation_id)
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                    constants,
                    &value_flow,
                    result_out,
                    deopt_out,
                    resume_state,
                    pending_status,
                    pending_value,
                    region.function,
                    region.flags.is_top_level,
                    region.local_count,
                    native_version,
                    pointer_type,
                )?;
                if matches!(instruction.kind, RegionInstructionKind::RuntimeFatal { .. }) {
                    terminated = true;
                    break;
                }
            }
            if terminated {
                continue;
            }
            builder.set_srcloc(ir::SourceLoc::new(
                region_block.terminator_continuation_id.saturating_add(1),
            ));
            lower_region_terminator(
                &mut builder,
                &blocks,
                &locals,
                &registers,
                result_out,
                pending_status,
                pending_value,
                module,
                native_operations,
                region.function,
                region.return_type.is_some(),
                &region_block.terminator,
                constants,
                &value_flow,
            )?;
        }
        if let Some(fragment) = fragment {
            let frame = fragment_frame.expect("fragment signature has a native frame");
            for target in &fragment.fragment.external_targets {
                builder.switch_to_block(cranelift_block(&blocks, *target)?);
                let target_block = region.blocks.get(target.index()).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_EXIT_TARGET",
                        format!("fragment exit target {} is missing", target.raw()),
                    )
                })?;
                for local in &target_block.entry_state_locals {
                    let value = use_local_variable(&mut builder, &locals, *local)?;
                    builder.ins().store(
                        MemFlagsData::new(),
                        value,
                        frame,
                        native_fragment_local_offset(*local),
                    );
                }
                for register in register_live_in.get(target).into_iter().flatten() {
                    let value = builder.try_use_var(register_variables[register]).map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_FRAGMENT_LIVE_REGISTER",
                            format!(
                                "fragment {} cannot materialize live register {} for block {}: {error}",
                                fragment.fragment.id,
                                register.raw(),
                                target.raw()
                            ),
                        )
                    })?;
                    let value = if builder.func.dfg.value_type(value) == types::I64 {
                        value
                    } else {
                        builder.ins().uextend(types::I64, value)
                    };
                    builder.ins().store(
                        MemFlagsData::new(),
                        value,
                        frame,
                        native_fragment_register_offset(region, *register),
                    );
                }
                let status = builder.use_var(pending_status);
                let value = builder.use_var(pending_value);
                builder.ins().store(
                    MemFlagsData::new(),
                    status,
                    frame,
                    native_fragment_pending_status_offset(region),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    value,
                    frame,
                    native_fragment_pending_value_offset(region),
                );
                let target_fragment = fragment.layout.block_owner[target];
                let callee =
                    module.declare_func_in_func(fragment.functions[&target_fragment], builder.func);
                let no_resume = builder.ins().iconst(types::I32, -1);
                let entry = builder.ins().iconst(types::I32, i64::from(target.raw()));
                builder.ins().store(
                    MemFlagsData::new(),
                    entry,
                    frame,
                    native_fragment_entry_id_offset(region),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    no_resume,
                    frame,
                    native_fragment_resume_id_offset(region),
                );
                builder.ins().return_call(callee, &[frame]);
            }
        }
        builder.switch_to_block(terminal_exit);
        let terminal_status = builder.block_params(terminal_exit)[0];
        let terminal_value = builder.block_params(terminal_exit)[1];
        builder
            .ins()
            .store(MemFlagsData::new(), terminal_value, result_out, 0);
        builder.ins().return_(&[terminal_status]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    let pre_regalloc = validate_pre_regalloc_structure(
        &ctx.func,
        region,
        fragment.map(|fragment| fragment.fragment.id),
    )?;
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("Cranelift verifier rejected executable Region IR: {error}"),
        )
    })?;
    let clif_blocks = ctx.func.layout.blocks().count();
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define native function: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            "Cranelift returned no compiled machine-code buffer",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |layout| layout.frame_to_fp_offset);
    if native_stack_bytes > MAX_NATIVE_SPILL_FRAME_BYTES {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_NATIVE_STACK_LIMIT",
            format!(
                "function {} requires {native_stack_bytes} native stack bytes; limit is {MAX_NATIVE_SPILL_FRAME_BYTES}",
                region.function_name
            ),
        ));
    }
    let code = compiled.code_buffer().to_vec();
    let alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    let relocations = compiled
        .buffer
        .relocs()
        .iter()
        .map(|relocation| {
            capture_relocation(
                module,
                ModuleReloc::from_mach_reloc(relocation, &ctx.func, func_id),
                functions,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let native_pc_ranges = ctx
        .compiled_code()
        .into_iter()
        .flat_map(|compiled| compiled.buffer.get_srclocs_sorted())
        .filter_map(|range| {
            let source = range.loc.bits();
            (source != 0 && source != u32::MAX).then_some(crate::JitNativePcRange {
                function: region.function,
                start: range.start,
                end: range.end,
                continuation_id: source - 1,
            })
        })
        .collect();
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        code,
        clif_blocks,
        alignment,
        relocations,
        native_pc_ranges,
        native_stack_bytes,
        pre_regalloc,
    })
}

fn region_graph_metadata<'a>(
    root: FunctionId,
    root_local_count: u32,
    regions: impl Iterator<Item = &'a RegionGraph>,
    native_pc_ranges: Vec<crate::JitNativePcRange>,
    function_entries: Vec<crate::JitNativeFunctionEntryMetadata>,
    suspending_functions: &BTreeSet<FunctionId>,
) -> crate::JitRegionStateMetadata {
    let regions = regions.collect::<Vec<_>>();
    let continuations = regions
        .iter()
        .flat_map(|region| {
            region.blocks.iter().flat_map(move |block| {
                block
                    .instructions
                    .iter()
                    .map(move |instruction| crate::JitContinuationMetadata {
                        id: instruction.continuation_id,
                        function: region.function,
                        block: block.id,
                        instruction: Some(instruction.id),
                        span: instruction.span,
                        live_locals: instruction.live_locals.clone(),
                    })
                    .chain(std::iter::once(crate::JitContinuationMetadata {
                        id: block.terminator_continuation_id,
                        function: region.function,
                        block: block.id,
                        instruction: None,
                        span: block.terminator_span,
                        live_locals: block.terminator_live_locals.clone(),
                    }))
            })
        })
        .collect();
    let osr_entries = regions
        .iter()
        .flat_map(|region| {
            region
                .osr_entries()
                .into_iter()
                .map(move |entry| crate::JitOsrEntryMetadata {
                    id: entry.id,
                    function: region.function,
                    block: entry.block,
                    continuation_id: entry.continuation_id,
                    live_locals: entry.live_locals,
                })
        })
        .collect();
    let root_direct_call_sites = function_entries
        .iter()
        .find(|entry| entry.function == root)
        .map_or(0, |entry| entry.direct_call_sites);
    let root_direct_method_call_sites = function_entries
        .iter()
        .find(|entry| entry.function == root)
        .map_or(0, |entry| entry.direct_method_call_sites);
    let root_inlining = function_entries
        .iter()
        .find(|entry| entry.function == root)
        .map(|entry| {
            (
                entry.inlined_call_sites,
                entry.inline_bytes_added,
                entry.tail_call_sites,
                entry.inline_rejected_by_reason.clone(),
            )
        })
        .unwrap_or_default();
    crate::JitRegionStateMetadata {
        local_count: root_local_count,
        compiler_tier: regions
            .first()
            .map(|region| region.compile_metadata.tier)
            .unwrap_or_default(),
        native_version: u32::from(
            regions.first().is_some_and(|region| {
                region.compile_metadata.tier == NativeCompilerTier::Optimizing
            }),
        ),
        compiled_to_compiled_call_sites: root_direct_call_sites,
        compiled_to_compiled_method_call_sites: root_direct_method_call_sites,
        inlined_call_sites: root_inlining.0,
        inline_bytes_added: root_inlining.1,
        tail_call_sites: root_inlining.2,
        inline_rejected_by_reason: root_inlining.3,
        continuations,
        native_pc_ranges,
        osr_entries,
        exception_handlers: regions
            .iter()
            .flat_map(|region| {
                region.exception_regions.iter().filter_map(move |handler| {
                    let enter_continuation = region
                        .blocks
                        .get(handler.block.index())?
                        .instructions
                        .iter()
                        .find(|instruction| instruction.id == handler.instruction)?
                        .continuation_id;
                    Some(crate::JitExceptionHandlerMetadata {
                        function: region.function,
                        enter_continuation,
                        protected_blocks: handler.protected_blocks.clone(),
                        catch: handler.catch,
                        catch_types: handler.catch_types.clone(),
                        finally: handler.finally,
                        after: handler.after,
                        exception_local: handler.exception_local,
                    })
                })
            })
            .collect(),
        safepoints: regions
            .iter()
            .flat_map(|region| {
                region.blocks.iter().flat_map(move |block| {
                    block
                        .instructions
                        .iter()
                        .filter(move |instruction| {
                            crate::region_ir::baseline_instruction_lowering(
                                &instruction.source_kind,
                            )
                            .requires_safepoint
                        })
                        .map(move |instruction| crate::JitNativeSafepointMetadata {
                            function: region.function,
                            continuation_id: instruction.continuation_id,
                            baseline_frame_slots: instruction.live_locals.clone(),
                            optimized_roots_required: region.compile_metadata.tier
                                == NativeCompilerTier::Optimizing,
                        })
                })
            })
            .collect(),
        suspensions: regions
            .iter()
            .flat_map(|region| {
                region.blocks.iter().flat_map(move |block| {
                    block.instructions.iter().filter_map(move |instruction| {
                        let RegionInstructionKind::NativeSuspend(suspend) = &instruction.kind
                        else {
                            return None;
                        };
                        let kind = match suspend {
                            RegionNativeSuspend::GeneratorYield { .. } => {
                                crate::JitNativeSuspendKind::GENERATOR_YIELD
                            }
                            RegionNativeSuspend::GeneratorDelegate { .. } => {
                                crate::JitNativeSuspendKind::GENERATOR_DELEGATE
                            }
                            RegionNativeSuspend::FiberSuspend { .. } => {
                                crate::JitNativeSuspendKind::FIBER_SUSPEND
                            }
                        };
                        Some(crate::JitNativeSuspensionMetadata {
                            function: region.function,
                            continuation_id: instruction.continuation_id,
                            resume_id: crate::native_suspension_resume_id(
                                instruction.continuation_id,
                            ),
                            kind,
                            span: instruction.span,
                            live_locals: instruction.live_locals.clone(),
                            owning_generation_required: true,
                        })
                    })
                })
            })
            .collect(),
        dynamic_code: regions
            .iter()
            .flat_map(|region| {
                region.blocks.iter().flat_map(move |block| {
                    block.instructions.iter().filter_map(move |instruction| {
                        let RegionInstructionKind::NativeDynamicCode(operation) = &instruction.kind
                        else {
                            return None;
                        };
                        let (kind, declared_function) = match operation {
                            RegionNativeDynamicCode::Include { kind, .. } => (
                                match kind {
                                    php_ir::instruction::IncludeKind::Include => {
                                        crate::JitNativeDynamicCodeKind::INCLUDE
                                    }
                                    php_ir::instruction::IncludeKind::IncludeOnce => {
                                        crate::JitNativeDynamicCodeKind::INCLUDE_ONCE
                                    }
                                    php_ir::instruction::IncludeKind::Require => {
                                        crate::JitNativeDynamicCodeKind::REQUIRE
                                    }
                                    php_ir::instruction::IncludeKind::RequireOnce => {
                                        crate::JitNativeDynamicCodeKind::REQUIRE_ONCE
                                    }
                                },
                                None,
                            ),
                            RegionNativeDynamicCode::Eval { .. } => {
                                (crate::JitNativeDynamicCodeKind::EVAL, None)
                            }
                            RegionNativeDynamicCode::DeclareFunction { function, .. } => (
                                crate::JitNativeDynamicCodeKind::DECLARE_FUNCTION,
                                Some(*function),
                            ),
                            RegionNativeDynamicCode::DeclareClass { .. } => {
                                (crate::JitNativeDynamicCodeKind::DECLARE_CLASS, None)
                            }
                            RegionNativeDynamicCode::RegisterConstant { .. } => {
                                (crate::JitNativeDynamicCodeKind::REGISTER_CONSTANT, None)
                            }
                            RegionNativeDynamicCode::EmitDiagnostic => {
                                (crate::JitNativeDynamicCodeKind::EMIT_DIAGNOSTIC, None)
                            }
                            RegionNativeDynamicCode::MakeClosure { function, .. } => (
                                crate::JitNativeDynamicCodeKind::MAKE_CLOSURE,
                                Some(*function),
                            ),
                        };
                        Some(crate::JitNativeDynamicCodeMetadata {
                            function: region.function,
                            continuation_id: instruction.continuation_id,
                            kind,
                            declared_function,
                            span: instruction.span,
                            process_cache: true,
                            restart_cache: true,
                        })
                    })
                })
            })
            .collect(),
        native_transitions: regions
            .iter()
            .flat_map(|region| {
                let liveness = native_transition_register_liveness(region, suspending_functions);
                region
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .filter_map(|instruction| {
                        if !instruction_has_native_transition(instruction, suspending_functions) {
                            return None;
                        }
                        let live_registers = liveness.get(&instruction.continuation_id)?;
                        (live_registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS).then(|| {
                            crate::JitNativeTransitionMetadata {
                                function: region.function,
                                native_version: u32::from(
                                    region.compile_metadata.tier == NativeCompilerTier::Optimizing,
                                ),
                                continuation_id: instruction.continuation_id,
                                resume_id: crate::native_transition_resume_id(
                                    instruction.continuation_id,
                                ),
                                span: instruction.span,
                                live_locals: instruction.live_locals.clone(),
                                live_registers: live_registers.clone(),
                                result_register: region_instruction_result_register(
                                    &instruction.kind,
                                ),
                            }
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect(),
        function_entries,
    }
}

#[cfg(test)]
mod tests {
    // Module-layout invariants are tested in `module_layout`; executable tests
    // exercise the resulting multi-function publication and invocation path.
}
