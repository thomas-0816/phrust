use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
struct NativeFragmentLayout {
    id: u32,
    blocks: BTreeSet<BlockId>,
    normal_entries: BTreeSet<BlockId>,
    external_targets: BTreeSet<BlockId>,
    locals: BTreeSet<LocalId>,
    registers: BTreeSet<RegId>,
    stored_registers: BTreeSet<RegId>,
}

#[derive(Clone, Debug)]
struct NativeFunctionFragmentLayout {
    fragments: Vec<NativeFragmentLayout>,
    block_owner: BTreeMap<BlockId, u32>,
    resume_owner: BTreeMap<i32, u32>,
    frame: NativeFragmentFrameLayout,
    register_liveness: NativeRegisterLiveness,
}

#[derive(Clone, Debug)]
struct NativeFragmentFrameLayout {
    local_slots: BTreeMap<LocalId, usize>,
    register_slots: BTreeMap<(u32, RegId), usize>,
    shared_register_slots: usize,
    scratch_register_slots: usize,
    value_slots: usize,
}

#[derive(Clone, Copy)]
struct NativeFragmentDefinition<'a> {
    layout: &'a NativeFunctionFragmentLayout,
    fragment: &'a NativeFragmentLayout,
    functions: &'a BTreeMap<u32, FuncId>,
}

impl NativeFragmentFrameLayout {
    fn for_fragments(
        region: &RegionGraph,
        fragments: &[NativeFragmentLayout],
        shared_registers: &BTreeSet<RegId>,
    ) -> Self {
        let mut locals = (0..region.local_count)
            .map(LocalId::new)
            .collect::<BTreeSet<_>>();
        for block in &region.blocks {
            locals.extend(block.entry_state_locals.iter().copied());
            locals.extend(block.terminator_state_locals.iter().copied());
            locals.extend(block.terminator_live_locals.iter().copied());
            for instruction in &block.instructions {
                locals.extend(instruction.live_locals.iter().copied());
            }
        }
        let local_slots = locals
            .into_iter()
            .enumerate()
            .map(|(slot, local)| (local, slot))
            .collect::<BTreeMap<_, _>>();
        let shared_base = local_slots.len();
        let shared_slots = shared_registers
            .iter()
            .enumerate()
            .map(|(slot, register)| (*register, shared_base.saturating_add(slot)))
            .collect::<BTreeMap<_, _>>();
        let scratch_base = shared_base.saturating_add(shared_slots.len());
        let mut register_slots = BTreeMap::new();
        let mut scratch_register_slots = 0_usize;
        for fragment in fragments {
            let mut next_scratch = 0_usize;
            for register in &fragment.stored_registers {
                let slot = shared_slots.get(register).copied().unwrap_or_else(|| {
                    let slot = scratch_base.saturating_add(next_scratch);
                    next_scratch = next_scratch.saturating_add(1);
                    slot
                });
                register_slots.insert((fragment.id, *register), slot);
            }
            scratch_register_slots = scratch_register_slots.max(next_scratch);
        }
        let value_slots = scratch_base.saturating_add(scratch_register_slots);
        Self {
            local_slots,
            register_slots,
            shared_register_slots: shared_slots.len(),
            scratch_register_slots,
            value_slots,
        }
    }

    fn frame_bytes(&self) -> Result<u32, CraneliftLoweringError> {
        let slots = u64::try_from(self.value_slots)
            .unwrap_or(u64::MAX)
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

    fn local_offset(&self, local: LocalId) -> Result<i32, CraneliftLoweringError> {
        self.local_slots
            .get(&local)
            .copied()
            .and_then(|slot| i32::try_from(slot.saturating_mul(8)).ok())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_FRAGMENT_LOCAL_SLOT",
                    format!("local {} has no compact fragment-frame slot", local.raw()),
                )
            })
    }

    fn register_offset(
        &self,
        fragment: u32,
        register: RegId,
    ) -> Result<i32, CraneliftLoweringError> {
        self.register_slots
            .get(&(fragment, register))
            .copied()
            .and_then(|slot| i32::try_from(slot.saturating_mul(8)).ok())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_FRAGMENT_REGISTER_SLOT",
                    format!(
                        "register {} has no compact slot in fragment {fragment}",
                        register.raw(),
                    ),
                )
            })
    }

    fn register_offset_if_present(&self, fragment: u32, register: RegId) -> Option<i32> {
        self.register_slots
            .get(&(fragment, register))
            .copied()
            .and_then(|slot| i32::try_from(slot.saturating_mul(8)).ok())
    }

    fn control_offset(&self, index: usize) -> i32 {
        i32::try_from(self.value_slots.saturating_add(index).saturating_mul(8)).unwrap_or(i32::MAX)
    }

    fn pending_status_offset(&self) -> i32 {
        self.control_offset(0)
    }
    fn pending_value_offset(&self) -> i32 {
        self.control_offset(1)
    }
    fn entry_id_offset(&self) -> i32 {
        self.control_offset(2)
    }
    fn arguments_offset(&self) -> i32 {
        self.control_offset(3)
    }
    fn result_out_offset(&self) -> i32 {
        self.control_offset(4)
    }
    fn deopt_out_offset(&self) -> i32 {
        self.control_offset(5)
    }
    fn resume_id_offset(&self) -> i32 {
        self.control_offset(6)
    }
    fn resume_state_offset(&self) -> i32 {
        self.control_offset(7)
    }
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
                RegionNativeControl::Throw { .. } => {}
                RegionNativeControl::EnterTry { .. }
                | RegionNativeControl::LeaveTry
                | RegionNativeControl::MakeException { .. } => {}
            }
        }
    }
    targets
}

fn region_block_entry_continuation(block: &crate::region_ir::RegionBlock) -> u32 {
    block.entry_continuation_id
}

impl NativeFunctionFragmentLayout {
    fn for_plan(
        region: &RegionGraph,
        plan: &NativeCompilePlan,
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
        let register_liveness = NativeRegisterLiveness::analyze(region);
        let register_live_in = &register_liveness.block_live_in;
        let mut fragments = plan
            .fragments
            .iter()
            .map(|fragment| {
                // Locals carry PHP reference/destructor semantics and every
                // write must remain observable even when classical liveness
                // says the value is dead. Keep the bounded function-local set
                // until the semantic local-access table can distinguish frame
                // cleanup roots from ordinary values. Registers, which drive
                // the pathological regalloc graph, are fragment-local below.
                let mut locals = (0..region.local_count)
                    .map(LocalId::new)
                    .collect::<BTreeSet<_>>();
                let mut registers = BTreeSet::new();
                let mut stored_registers = BTreeSet::new();
                for block_id in &fragment.blocks {
                    let block = &region.blocks[block_id.index()];
                    let mut block_definitions = BTreeSet::new();
                    locals.extend(block.entry_state_locals.iter().copied());
                    locals.extend(block.terminator_state_locals.iter().copied());
                    locals.extend(block.terminator_live_locals.iter().copied());
                    registers.extend(block.terminator.register_uses());
                    registers.extend(
                        register_live_in
                            .get(block_id)
                            .into_iter()
                            .flatten()
                            .copied(),
                    );
                    stored_registers.extend(
                        register_live_in
                            .get(block_id)
                            .into_iter()
                            .flatten()
                            .copied(),
                    );
                    for instruction in &block.instructions {
                        locals.extend(instruction.live_locals.iter().copied());
                        let uses = instruction.register_uses();
                        registers.extend(uses.iter().copied());
                        // Region liveness deliberately models semantic CFG
                        // state, but executable lowering also contains
                        // synthesized/path-dependent operands. Materialize
                        // every use not dominated by a definition in this
                        // real block; same-block definitions remain cached.
                        stored_registers.extend(
                            uses.into_iter()
                                .filter(|register| !block_definitions.contains(register)),
                        );
                        if instruction_has_sparse_snapshot(
                            instruction,
                            region.compile_metadata.tier,
                        ) {
                            stored_registers.extend(
                                register_liveness
                                    .transition_live
                                    .get(&instruction.continuation_id)
                                    .into_iter()
                                    .flatten()
                                    .copied(),
                            );
                        }
                        block_definitions
                            .extend(region_instruction_defined_registers(&instruction.kind));
                    }
                    stored_registers.extend(
                        block
                            .terminator
                            .register_uses()
                            .into_iter()
                            .filter(|register| !block_definitions.contains(register)),
                    );
                    if block_terminator_has_native_transition(block, region.compile_metadata.tier) {
                        stored_registers.extend(
                            register_liveness
                                .transition_live
                                .get(&block.terminator_continuation_id)
                                .into_iter()
                                .flatten()
                                .copied(),
                        );
                    }
                }
                // Region lowering can synthesize results that do not exist in
                // the source IR (for example the discarded result of a
                // property unset). Declare the executable definitions even
                // when their first use is outside this fragment.
                for block_id in &fragment.blocks {
                    for instruction in &region.blocks[block_id.index()].instructions {
                        registers.extend(region_instruction_defined_registers(&instruction.kind));
                    }
                }
                NativeFragmentLayout {
                    id: fragment.id,
                    blocks: fragment.blocks.iter().copied().collect(),
                    normal_entries: BTreeSet::new(),
                    external_targets: BTreeSet::new(),
                    locals,
                    registers,
                    stored_registers,
                }
            })
            .collect::<Vec<_>>();
        if let Some(owner) = block_owner.get(&BlockId::new(0)).copied() {
            fragments[owner as usize]
                .normal_entries
                .insert(BlockId::new(0));
        }
        let mut shared_registers = BTreeSet::new();
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
                    shared_registers
                        .extend(register_live_in.get(&target).into_iter().flatten().copied());
                }
                fragments[source_owner as usize]
                    .stored_registers
                    .extend(register_live_in.get(&target).into_iter().flatten().copied());
            }
        }

        let transition_liveness = &register_liveness.transition_live;
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
            if region.compile_metadata.tier == NativeCompilerTier::Optimizing {
                insert_resume(
                    crate::native_optimizing_continuation_resume_id(
                        region_block_entry_continuation(block),
                    ),
                    block.id,
                )?;
            }
            if block_terminator_has_native_transition(block, region.compile_metadata.tier)
                && transition_liveness
                    .get(&block.terminator_continuation_id)
                    .is_some_and(|live| live.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
            {
                insert_resume(
                    crate::native_transition_resume_id(block.terminator_continuation_id),
                    block.id,
                )?;
            }
            for instruction in &block.instructions {
                if matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_)) {
                    insert_resume(
                        crate::native_suspension_resume_id(instruction.continuation_id),
                        block.id,
                    )?;
                }
                if instruction_has_native_resume_entry(instruction, region.compile_metadata.tier)
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
        let frame = NativeFragmentFrameLayout::for_fragments(region, &fragments, &shared_registers);
        Ok(Self {
            fragments,
            block_owner,
            resume_owner,
            frame,
            register_liveness,
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

pub(super) fn instruction_has_native_transition(
    instruction: &RegionInstruction,
    tier: NativeCompilerTier,
) -> bool {
    if tier == NativeCompilerTier::Optimizing {
        return instruction.optimizer_transition_entry;
    }
    // Baseline must publish the exact entry used by an optimizing island
    // exit, including the first instruction of a baseline-only family. The
    // old hand-maintained allow-list covered direct guards but omitted such
    // island heads (for example a static-local operation), so valid optimized
    // code produced a state the corresponding baseline artifact could not
    // enter.
    if instruction.optimizer_transition_entry {
        return true;
    }
    // Checked binary operations can request a baseline retry. A userland call
    // also needs a caller continuation when its callee suspends (for example a
    // Fiber::suspend nested below the call); throw and exit still unwind
    // terminally through the handler table. These are real resumable
    // safepoints, not instruction-per-resume entries.
    matches!(
        instruction.kind,
        RegionInstructionKind::Binary { .. }
            | RegionInstructionKind::Unary { .. }
            | RegionInstructionKind::LoadLocal { .. }
            | RegionInstructionKind::StoreLocal { .. }
            | RegionInstructionKind::AssignLocalResult { .. }
            | RegionInstructionKind::Discard { .. }
            | RegionInstructionKind::IssetLocal { .. }
            | RegionInstructionKind::EmptyLocal { .. }
            | RegionInstructionKind::UnsetLocal { .. }
            | RegionInstructionKind::NewArray { .. }
            | RegionInstructionKind::ArrayInsert { .. }
            | RegionInstructionKind::AppendDim { .. }
            | RegionInstructionKind::IssetDim { .. }
            | RegionInstructionKind::EmptyDim { .. }
            | RegionInstructionKind::FetchDim {
                mode: php_ir::instruction::DimFetchMode::Read,
                ..
            }
            | RegionInstructionKind::ForeachInit { .. }
            | RegionInstructionKind::ForeachNext { .. }
            | RegionInstructionKind::ForeachCleanup { .. }
            | RegionInstructionKind::FetchProperty { .. }
            | RegionInstructionKind::NativeCall(_)
            | RegionInstructionKind::NativeDynamicCode(_)
    )
}

fn optimizing_instruction_family_is_direct(kind: &RegionInstructionKind) -> bool {
    match kind {
        RegionInstructionKind::AssignDim { keys, .. }
        | RegionInstructionKind::UnsetDim { keys, .. } => !keys.is_empty(),
        RegionInstructionKind::AppendDim { .. } => true,
        RegionInstructionKind::ArrayInsert {
            key, by_ref_local, ..
        } => key.is_none() && by_ref_local.is_none(),
        RegionInstructionKind::Nop
        | RegionInstructionKind::Move { .. }
        | RegionInstructionKind::LoadLocal { .. }
        | RegionInstructionKind::StoreLocal { .. }
        | RegionInstructionKind::AssignLocalResult { .. }
        | RegionInstructionKind::Discard { .. }
        | RegionInstructionKind::Binary { .. }
        | RegionInstructionKind::Unary { .. }
        | RegionInstructionKind::Compare { .. }
        | RegionInstructionKind::Cast { .. }
        | RegionInstructionKind::NewArray { .. }
        | RegionInstructionKind::FetchDim {
            mode: php_ir::instruction::DimFetchMode::Read,
            ..
        }
        | RegionInstructionKind::IssetDim { .. }
        | RegionInstructionKind::EmptyDim { .. }
        | RegionInstructionKind::IssetLocal { .. }
        | RegionInstructionKind::EmptyLocal { .. }
        | RegionInstructionKind::UnsetLocal { .. }
        | RegionInstructionKind::ForeachInit { .. }
        | RegionInstructionKind::ForeachNext { .. }
        | RegionInstructionKind::ForeachCleanup { .. }
        | RegionInstructionKind::FetchProperty { .. }
        | RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::MakeClosure {
            ..
        })
        | RegionInstructionKind::NativeCall(_) => true,
        _ => false,
    }
}

fn optimizing_direct_instruction_may_transition(kind: &RegionInstructionKind) -> bool {
    !matches!(
        kind,
        RegionInstructionKind::Nop | RegionInstructionKind::Move { .. }
    )
}

fn prepare_optimizing_baseline_islands(mut region: RegionGraph) -> RegionGraph {
    let boundaries = region
        .blocks
        .iter()
        .filter_map(|block| {
            let mut previous = None;
            let boundaries = block
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(index, instruction)| {
                    let direct = optimizing_instruction_family_is_direct(&instruction.kind);
                    let changed = previous.is_some_and(|previous| previous != direct);
                    previous = Some(direct);
                    (index != 0 && changed).then_some(index)
                })
                .collect::<BTreeSet<_>>();
            (!boundaries.is_empty()).then_some((block.id, boundaries))
        })
        .collect::<BTreeMap<_, _>>();
    region = super::module_layout::split_region_blocks_at_boundaries(region, &boundaries);
    for block in &mut region.blocks {
        let direct = block
            .instructions
            .first()
            .is_none_or(|instruction| optimizing_instruction_family_is_direct(&instruction.kind));
        for (index, instruction) in block.instructions.iter_mut().enumerate() {
            instruction.optimizer_transition_entry = if direct {
                optimizing_direct_instruction_may_transition(&instruction.kind)
            } else {
                index == 0
            };
        }
    }
    region
}

fn instruction_has_sparse_snapshot(
    instruction: &RegionInstruction,
    tier: NativeCompilerTier,
) -> bool {
    instruction_has_native_transition(instruction, tier)
        || matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_))
}

/// Whether this artifact can be entered again at the instruction after it
/// has already started executing. Guard failures in optimizing code exit to
/// the baseline artifact; they are not optimizer resume entries. Conflating
/// those two directions forced the normal optimizing path through a distinct
/// CLIF block for every guardable PHP instruction.
fn instruction_has_native_resume_entry(
    instruction: &RegionInstruction,
    tier: NativeCompilerTier,
) -> bool {
    match tier {
        NativeCompilerTier::Baseline => instruction_has_native_transition(instruction, tier),
        NativeCompilerTier::Optimizing => {
            matches!(instruction.kind, RegionInstructionKind::NativeCall(_))
        }
    }
}

fn terminator_has_native_transition(terminator: &RegionTerminator) -> bool {
    !matches!(terminator, RegionTerminator::Jump { .. })
}

fn block_terminator_has_native_transition(
    block: &crate::region_ir::RegionBlock,
    _tier: NativeCompilerTier,
) -> bool {
    terminator_has_native_transition(&block.terminator)
        && !block.instructions.iter().any(|instruction| {
            matches!(instruction.kind, RegionInstructionKind::RuntimeFatal { .. })
        })
}

/// Restore the sparse local portion of a native continuation into the compact
/// streaming frame.  The initialization masks are already part of the
/// transition ABI, so one cold loop can serve every handler, suspension, OSR,
/// and tier-transition entry in the fragment.  Emitting the same local-copy
/// sequence into every resume loader made cold state reconstruction dominate
/// the machine code of large baseline functions.
fn emit_streaming_local_restore_loop(
    builder: &mut FunctionBuilder<'_>,
    pointer_type: ir::Type,
    state: ir::Value,
    frame: ir::Value,
    local_count: u32,
    continuation: ir::Block,
) {
    if local_count == 0 {
        builder.ins().jump(continuation, &[]);
        return;
    }

    let header = builder.create_block();
    let test = builder.create_block();
    let copy = builder.create_block();
    let next = builder.create_block();
    for block in [header, test, copy, next] {
        builder.set_cold_block(block);
    }
    builder.append_block_param(header, types::I64);
    builder.append_block_param(test, types::I64);
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(next, types::I64);

    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(header, &[zero.into()]);

    builder.switch_to_block(header);
    let index = builder.block_params(header)[0];
    let in_range = builder
        .ins()
        .icmp_imm(IntCC::UnsignedLessThan, index, i64::from(local_count));
    builder
        .ins()
        .brif(in_range, test, &[index.into()], continuation, &[]);

    builder.switch_to_block(test);
    let index = builder.block_params(test)[0];
    let word = builder.ins().ushr_imm(index, 6);
    let word_bytes = builder.ins().ishl_imm(word, 3);
    let word_bytes = if pointer_type == types::I64 {
        word_bytes
    } else {
        builder.ins().ireduce(pointer_type, word_bytes)
    };
    let mask_base = builder.ins().iadd_imm(
        state,
        std::mem::offset_of!(crate::JitDeoptState, initialized_mask) as i64,
    );
    let mask_address = builder.ins().iadd(mask_base, word_bytes);
    let mask = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), mask_address, 0);
    let bit_index = builder.ins().band_imm(index, 63);
    let one = builder.ins().iconst(types::I64, 1);
    let bit = builder.ins().ishl(one, bit_index);
    let initialized = builder.ins().band(mask, bit);
    let initialized = builder.ins().icmp_imm(IntCC::NotEqual, initialized, 0);
    builder
        .ins()
        .brif(initialized, copy, &[index.into()], next, &[index.into()]);

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let slot_bytes = builder.ins().ishl_imm(index, 3);
    let slot_bytes = if pointer_type == types::I64 {
        slot_bytes
    } else {
        builder.ins().ireduce(pointer_type, slot_bytes)
    };
    let state_slots = builder.ins().iadd_imm(
        state,
        std::mem::offset_of!(crate::JitDeoptState, slots) as i64,
    );
    let state_slot = builder.ins().iadd(state_slots, slot_bytes);
    let frame_slot = builder.ins().iadd(frame, slot_bytes);
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), state_slot, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), value, frame_slot, 0);
    builder.ins().jump(next, &[index.into()]);

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(header, &[index.into()]);
}

/// Publish sparse baseline locals from the compact fragment frame. Every
/// callsite supplies only its static live mask; one cold loop performs the
/// actual copy for all call side exits in the fragment.
fn emit_streaming_local_snapshot_loop(
    builder: &mut FunctionBuilder<'_>,
    pointer_type: ir::Type,
    state: ir::Value,
    frame: ir::Value,
    local_count: u32,
    continuation: ir::Block,
) {
    if local_count == 0 {
        builder.ins().jump(continuation, &[]);
        return;
    }

    let header = builder.create_block();
    let test = builder.create_block();
    let copy = builder.create_block();
    let next = builder.create_block();
    for block in [header, test, copy, next] {
        builder.set_cold_block(block);
    }
    builder.append_block_param(header, types::I64);
    builder.append_block_param(test, types::I64);
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(next, types::I64);

    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(header, &[zero.into()]);

    builder.switch_to_block(header);
    let index = builder.block_params(header)[0];
    let in_range = builder
        .ins()
        .icmp_imm(IntCC::UnsignedLessThan, index, i64::from(local_count));
    builder
        .ins()
        .brif(in_range, test, &[index.into()], continuation, &[]);

    builder.switch_to_block(test);
    let index = builder.block_params(test)[0];
    let word = builder.ins().ushr_imm(index, 6);
    let word_bytes = builder.ins().ishl_imm(word, 3);
    let word_bytes = if pointer_type == types::I64 {
        word_bytes
    } else {
        builder.ins().ireduce(pointer_type, word_bytes)
    };
    let mask_base = builder.ins().iadd_imm(
        state,
        std::mem::offset_of!(crate::JitDeoptState, initialized_mask) as i64,
    );
    let mask_address = builder.ins().iadd(mask_base, word_bytes);
    let mask = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), mask_address, 0);
    let bit_index = builder.ins().band_imm(index, 63);
    let one = builder.ins().iconst(types::I64, 1);
    let bit = builder.ins().ishl(one, bit_index);
    let initialized = builder.ins().band(mask, bit);
    let initialized = builder.ins().icmp_imm(IntCC::NotEqual, initialized, 0);
    builder
        .ins()
        .brif(initialized, copy, &[index.into()], next, &[index.into()]);

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let slot_bytes = builder.ins().ishl_imm(index, 3);
    let slot_bytes = if pointer_type == types::I64 {
        slot_bytes
    } else {
        builder.ins().ireduce(pointer_type, slot_bytes)
    };
    let state_slots = builder.ins().iadd_imm(
        state,
        std::mem::offset_of!(crate::JitDeoptState, slots) as i64,
    );
    let state_slot = builder.ins().iadd(state_slots, slot_bytes);
    let frame_slot = builder.ins().iadd(frame, slot_bytes);
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), frame_slot, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), value, state_slot, 0);
    builder.ins().jump(next, &[index.into()]);

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(header, &[index.into()]);
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
                for defined in region_instruction_defined_registers(&instruction.kind) {
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

#[derive(Clone, Debug)]
struct NativeRegisterLiveness {
    block_live_in: BTreeMap<BlockId, BTreeSet<RegId>>,
    transition_live: BTreeMap<u32, Vec<RegId>>,
}

impl NativeRegisterLiveness {
    fn analyze(region: &RegionGraph) -> Self {
        let block_live_in = native_register_live_in(region);
        let mut transition_live = BTreeMap::new();
        for block in &region.blocks {
            let mut live = native_transition_successors(&block.terminator)
                .into_iter()
                .filter_map(|successor| block_live_in.get(&successor))
                .flat_map(|registers| registers.iter().copied())
                .collect::<BTreeSet<_>>();
            live.extend(block.terminator.register_uses());
            if block_terminator_has_native_transition(block, region.compile_metadata.tier) {
                transition_live.insert(
                    block.terminator_continuation_id,
                    live.iter().copied().collect(),
                );
            }
            for instruction in block.instructions.iter().rev() {
                for defined in region_instruction_defined_registers(&instruction.kind) {
                    live.remove(&defined);
                }
                live.extend(instruction.register_uses());
                if instruction_has_sparse_snapshot(instruction, region.compile_metadata.tier) {
                    transition_live
                        .insert(instruction.continuation_id, live.iter().copied().collect());
                }
            }
        }
        Self {
            block_live_in,
            transition_live,
        }
    }
}

fn ir_function_requires_trampoline(function: &php_ir::IrFunction) -> bool {
    function.params.iter().any(|parameter| parameter.by_ref)
        || function.returns_by_ref
        || ir_function_requires_non_reference_trampoline(function)
}

fn ir_function_requires_non_reference_trampoline(function: &php_ir::IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                php_ir::InstructionKind::EnterTry { .. }
                    | php_ir::InstructionKind::LeaveTry
                    | php_ir::InstructionKind::Throw { .. }
                    | php_ir::InstructionKind::MakeClosure { .. }
                    | php_ir::InstructionKind::Yield { .. }
                    | php_ir::InstructionKind::YieldFrom { .. }
            )
        })
    }) || function.attributes.iter().any(|attribute| {
        attribute
            .resolved_name
            .as_deref()
            .or(attribute.fallback_name.as_deref())
            .unwrap_or(&attribute.name)
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("deprecated")
    })
}

fn declare_baseline_value_operation(
    module: &mut JITModule,
    symbol: &str,
    arity: u8,
    address: usize,
) -> Result<NativeHelper, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I32));
    for _ in 0..arity {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    declare_native_helper(module, symbol, &signature, address)
}

fn declare_native_helper(
    module: &mut JITModule,
    symbol: &str,
    signature: &ir::Signature,
    address: usize,
) -> Result<NativeHelper, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = signature.clone();
    signature.params.insert(0, AbiParam::new(pointer_type));
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
        inline_runtime_view: false,
        runtime: None,
    })
}

pub(super) fn compile_region_graph_native(
    unit: &IrUnit,
    region: RegionGraph,
    plan: NativeCompilePlan,
    runtime_helpers: crate::JitRuntimeHelperAddresses,
    request: &JitCompileRequest,
) -> Result<NativeScalarRegionCompileResult, CraneliftLoweringError> {
    validate_region_native_coverage(&region)?;
    region.verify().map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_REGION_VERIFY", error.to_string())
    })?;
    let function = region.function;
    let runtime_unit_identity = if request.deployment_runtime_identity == 0 {
        u64::from(unit.id.raw())
    } else {
        request.deployment_runtime_identity
    };
    let mut regions = BTreeMap::from([(function, region)]);
    for candidate in regions.values_mut() {
        select_native_region_tier(candidate, &plan, &unit.constants);
    }
    // Admission can deliberately downgrade an optimizing request when even
    // one instruction family still belongs to the baseline-native runtime.
    // The incoming plan was built for the requested tier and may therefore
    // contain one large whole-region job. Re-plan the resulting graph before
    // any CLIF construction so the downgrade cannot bypass baseline fragment
    // ceilings or fail a valid PHP unit merely because its stale optimizing
    // plan was oversized.
    let replanned = split_oversized_region_blocks(
        regions
            .remove(&function)
            .expect("compile group owns its requested function"),
    );
    regions.insert(function, replanned);
    let plan = NativeCompilePlan::for_region(&regions[&function]);
    if regions[&function].compile_metadata.tier == NativeCompilerTier::Baseline
        && let Some(fragment) = plan
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
    let region = &regions[&function];
    let compilation_mode = crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
        region.compile_metadata.tier,
    )
    .mode();
    let baseline_helper_imports = compilation_mode
        == crate::cranelift_lowering::baseline_streaming::NativeCompilationMode::StreamingBaseline;
    let fragment_layout = (plan.fragments.len() > 1
        || regions
            .values()
            .any(|candidate| candidate.compile_metadata.tier == NativeCompilerTier::Baseline))
    .then(|| NativeFunctionFragmentLayout::for_plan(region, &plan))
    .transpose()?;
    let selected_plan = std::cell::RefCell::new(plan.clone());
    let selected_fragment_layout = std::cell::RefCell::new(fragment_layout.clone());
    // Value-flow and executable SSA describe the PHP function, not one native
    // fragment. Build them exactly once after tier selection and Region-block
    // splitting. Recomputing the complete dominator/phi graph inside every
    // fragment lowering made fragmentation multiply whole-function analysis.
    let value_flows = regions
        .iter()
        .map(|(function, candidate)| {
            let flow = if candidate.compile_metadata.tier == NativeCompilerTier::Optimizing {
                crate::region_ir::analyze_executable_value_flow(candidate, &unit.constants)
            } else {
                crate::region_ir::analyze_baseline_value_ownership(candidate)
            };
            flow.verify_ownership(candidate).map_err(|error| {
                CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_OWNERSHIP", error)
            })?;
            Ok((*function, flow))
        })
        .collect::<Result<BTreeMap<_, _>, CraneliftLoweringError>>()?;
    let ssa_metrics = regions
        .iter()
        .filter(|(_, candidate)| candidate.compile_metadata.tier == NativeCompilerTier::Optimizing)
        .map(|(function, _)| {
            let flow = &value_flows[function];
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
                        if !matches!(call.target, RegionCallTarget::Semantic { .. })
                            && (matches!(call.result, RegionCallResult::ReferenceLocal(_))
                            || call.args.iter().any(|argument| {
                                argument.name.is_some() || argument.unpack
                            })
                            || call.direct_compiled_target().is_some_and(|target| {
                                trampoline_functions.contains(&target)
                            }))
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
    let needs_builtin_dispatch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call)
                if stable_builtin_helper_id(&call.target).is_some())
        })
    });
    let needs_exact_symbol_query: [bool; StableSymbolQueryBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_symbol_query(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_pcre: [bool; StablePcreBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_pcre(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_json: [bool; StableJsonBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_json(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_format: [bool; StableFormatBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_format(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_path: [bool; StablePathBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_path(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_semantic_dispatch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call)
                if matches!(call.target, RegionCallTarget::Semantic { .. }))
        })
    });
    let needs_frame_arena = runtime_helpers.native_frame_alloc != 0
        && runtime_helpers.native_frame_release != 0
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(_))
            })
        });
    if baseline_helper_imports && needs_call_trampoline && runtime_helpers.native_call_dispatch == 0
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_CALL_TRAMPOLINE",
            "dynamic or complex call requires the typed native dispatch trampoline",
        ));
    }
    if baseline_helper_imports
        && needs_builtin_dispatch
        && runtime_helpers.native_builtin_dispatch == 0
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_BUILTIN_DISPATCH",
            "direct builtin call requires the stable-ID native builtin dispatcher",
        ));
    }
    if baseline_helper_imports
        && needs_semantic_dispatch
        && runtime_helpers.native_semantic_dispatch == 0
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_SEMANTIC_DISPATCH",
            "typed semantic operation requires the direct semantic dispatcher",
        ));
    }
    let needs_dynamic_code = regions.values().any(RegionGraph::has_native_dynamic_code);
    if baseline_helper_imports && needs_dynamic_code && runtime_helpers.native_dynamic_code == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_DYNAMIC_CODE",
            "include, eval, or runtime declaration requires the native dynamic-code compiler",
        ));
    }
    let native_call_symbol = NATIVE_CALL_DISPATCH_SYMBOL.to_owned();
    let native_builtin_dispatch_symbol = BASELINE_NATIVE_BUILTIN_DISPATCH_SYMBOL.to_owned();
    let native_semantic_dispatch_symbol = NATIVE_SEMANTIC_DISPATCH_SYMBOL.to_owned();
    let native_function_resolve_symbol = NATIVE_FUNCTION_RESOLVE_SYMBOL.to_owned();
    let native_dynamic_code_symbol = NATIVE_DYNAMIC_CODE_SYMBOL.to_owned();
    let needs_unary = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Unary { .. }
                    | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::EmptyLocal { .. }
            )
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
                RegionInstructionKind::Compare { .. }
                    | RegionInstructionKind::IssetDim { .. }
                    | RegionInstructionKind::IssetLocal { .. }
            )
        })
    });
    let needs_cast = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Cast { .. }
                    | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::EmptyLocal { .. }
            )
        })
    });
    let needs_float_to_string = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Cast {
                    op: RegionCastOp::String,
                    ..
                }
            )
        })
    });
    let needs_float_to_int = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Cast {
                    op: RegionCastOp::Int,
                    ..
                } | RegionInstructionKind::Unary {
                    op: RegionUnaryOp::BitNot,
                    ..
                }
            )
        })
    });
    let needs_object_class_name = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::FetchObjectClassName {
                    prepared_class: None,
                    ..
                }
            )
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
        region
            .exception_regions
            .iter()
            .any(|handler| handler.catch.is_some() && handler.exception_local.is_some())
            || region_contains(region, |kind| {
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
    let needs_value_release = true;
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
    let needs_plain_object_clone = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::CloneObject { plain: true, .. })
        })
    });
    let needs_prepared_closure_new = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::NativeDynamicCode(
                    RegionNativeDynamicCode::MakeClosure { .. }
                )
            )
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
            ) || matches!(kind, RegionInstructionKind::NativeCall(call)
                if stable_builtin_array_key_exists(&call.target))
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
            matches!(
                kind,
                RegionInstructionKind::NativeCall(call)
                    if stable_builtin_type_predicate(&call.target).is_some()
                        && call.argument_operand_offset == 0
                        && call.args.len() == 1
                        && call.args[0].name.is_none()
                        && !call.args[0].unpack
                        && call.operands.len() == 1
                        && call.operands[0].is_some()
                        && !matches!(call.result, RegionCallResult::ReferenceLocal(_))
            )
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
    let needs_string_predicate = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call)
                if stable_builtin_string_predicate(&call.target).is_some())
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
    // Shadow every generic-runtime requirement with the tier capability.  In
    // particular, the optimizing closure below never declares a helper and
    // therefore cannot smuggle one into code through an unused wrapper.
    let needs_call_trampoline = baseline_helper_imports && needs_call_trampoline;
    let needs_function_resolver = baseline_helper_imports && needs_function_resolver;
    let needs_builtin_dispatch = baseline_helper_imports && needs_builtin_dispatch;
    let needs_semantic_dispatch = baseline_helper_imports && needs_semantic_dispatch;
    let needs_frame_arena = baseline_helper_imports && needs_frame_arena;
    let needs_dynamic_code = baseline_helper_imports && needs_dynamic_code;
    let needs_unary = baseline_helper_imports && needs_unary;
    let needs_binary = baseline_helper_imports && needs_binary;
    let needs_compare = baseline_helper_imports && needs_compare;
    let needs_float_to_string = !baseline_helper_imports && needs_float_to_string;
    let needs_float_to_int = !baseline_helper_imports && needs_float_to_int;
    let needs_object_class_name = !baseline_helper_imports && needs_object_class_name;
    let needs_cast = baseline_helper_imports && needs_cast;
    let needs_direct_echo = !baseline_helper_imports && needs_echo;
    let needs_echo = baseline_helper_imports && needs_echo;
    let needs_local_fetch = baseline_helper_imports && needs_local_fetch;
    let needs_local_store = baseline_helper_imports && needs_local_store;
    let needs_value_release = baseline_helper_imports && needs_value_release;
    let needs_reference_bind = baseline_helper_imports && needs_reference_bind;
    let needs_argument_check = baseline_helper_imports && needs_argument_check;
    let needs_return_check = baseline_helper_imports && needs_return_check;
    let needs_exception_new = baseline_helper_imports && needs_exception_new;
    let needs_array_new = baseline_helper_imports && needs_array_new;
    let needs_prepared_object_new = !baseline_helper_imports && needs_object_new;
    let needs_prepared_closure_new = !baseline_helper_imports && needs_prepared_closure_new;
    let needs_object_new = baseline_helper_imports && needs_object_new;
    let needs_property_fetch = baseline_helper_imports && needs_property_fetch;
    let needs_property_assign = baseline_helper_imports && needs_property_assign;
    let needs_object_clone = baseline_helper_imports && needs_object_clone;
    let needs_plain_object_clone = !baseline_helper_imports && needs_plain_object_clone;
    let needs_object_clone_with = baseline_helper_imports && needs_object_clone_with;
    let needs_array_insert = baseline_helper_imports && needs_array_insert;
    let needs_array_fetch = baseline_helper_imports && needs_array_fetch;
    let needs_array_unset = baseline_helper_imports && needs_array_unset;
    let needs_array_spread = baseline_helper_imports && needs_array_spread;
    let needs_foreach_init = baseline_helper_imports && needs_foreach_init;
    let needs_foreach_next = baseline_helper_imports && needs_foreach_next;
    let needs_foreach_cleanup = baseline_helper_imports && needs_foreach_cleanup;
    let needs_constant_fetch = baseline_helper_imports && needs_constant_fetch;
    let needs_truthy = baseline_helper_imports && needs_truthy;
    let needs_type_predicate = baseline_helper_imports && needs_type_predicate;
    let needs_stable_length = baseline_helper_imports && needs_stable_length;
    let needs_string_predicate = baseline_helper_imports && needs_string_predicate;
    let needs_runtime_fatal = baseline_helper_imports && needs_runtime_fatal;
    let needs_execution_poll = baseline_helper_imports && needs_execution_poll;
    let mut imports = vec![(
        "region-runtime-helper-abi".to_owned(),
        region.compile_metadata.helper_abi_hash as usize,
    )];
    if baseline_helper_imports && needs_call_trampoline {
        imports.push((
            native_call_symbol.clone(),
            runtime_helpers.native_call_dispatch,
        ));
    }
    if baseline_helper_imports && needs_builtin_dispatch {
        imports.push((
            native_builtin_dispatch_symbol.clone(),
            runtime_helpers.native_builtin_dispatch,
        ));
    }
    for builtin in StableSymbolQueryBuiltin::all() {
        if !needs_exact_symbol_query[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StableSymbolQueryBuiltin::Defined => runtime_helpers.native_defined,
            StableSymbolQueryBuiltin::FunctionExists => runtime_helpers.native_function_exists,
            StableSymbolQueryBuiltin::ClassExists => runtime_helpers.native_class_exists,
            StableSymbolQueryBuiltin::InterfaceExists => runtime_helpers.native_interface_exists,
            StableSymbolQueryBuiltin::TraitExists => runtime_helpers.native_trait_exists,
            StableSymbolQueryBuiltin::EnumExists => runtime_helpers.native_enum_exists,
            StableSymbolQueryBuiltin::MethodExists => runtime_helpers.native_method_exists,
            StableSymbolQueryBuiltin::PropertyExists => runtime_helpers.native_property_exists,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_SYMBOL_QUERY",
                format!(
                    "prepared symbol query requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StablePcreBuiltin::all() {
        if !needs_exact_pcre[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StablePcreBuiltin::Match => runtime_helpers.native_preg_match,
            StablePcreBuiltin::MatchAll => runtime_helpers.native_preg_match_all,
            StablePcreBuiltin::Replace => runtime_helpers.native_preg_replace,
            StablePcreBuiltin::Filter => runtime_helpers.native_preg_filter,
            StablePcreBuiltin::Split => runtime_helpers.native_preg_split,
            StablePcreBuiltin::Grep => runtime_helpers.native_preg_grep,
            StablePcreBuiltin::Quote => runtime_helpers.native_preg_quote,
            StablePcreBuiltin::LastError => runtime_helpers.native_preg_last_error,
            StablePcreBuiltin::LastErrorMessage => runtime_helpers.native_preg_last_error_msg,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_PCRE",
                format!(
                    "prepared PCRE builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableJsonBuiltin::all() {
        if !needs_exact_json[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StableJsonBuiltin::Encode => runtime_helpers.native_json_encode,
            StableJsonBuiltin::Decode => runtime_helpers.native_json_decode,
            StableJsonBuiltin::Validate => runtime_helpers.native_json_validate,
            StableJsonBuiltin::LastError => runtime_helpers.native_json_last_error,
            StableJsonBuiltin::LastErrorMessage => runtime_helpers.native_json_last_error_msg,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_JSON",
                format!(
                    "prepared JSON builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableFormatBuiltin::all() {
        if !needs_exact_format[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StableFormatBuiltin::Sprintf => runtime_helpers.native_sprintf,
            StableFormatBuiltin::Printf => runtime_helpers.native_printf,
            StableFormatBuiltin::Vsprintf => runtime_helpers.native_vsprintf,
            StableFormatBuiltin::Vprintf => runtime_helpers.native_vprintf,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_FORMAT",
                format!(
                    "prepared formatting builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StablePathBuiltin::all() {
        if !needs_exact_path[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StablePathBuiltin::Basename => runtime_helpers.native_basename,
            StablePathBuiltin::Dirname => runtime_helpers.native_dirname,
            StablePathBuiltin::Realpath => runtime_helpers.native_realpath,
            StablePathBuiltin::FileExists => runtime_helpers.native_file_exists,
            StablePathBuiltin::Fopen => runtime_helpers.native_fopen,
            StablePathBuiltin::Fwrite => runtime_helpers.native_fwrite,
            StablePathBuiltin::Fclose => runtime_helpers.native_fclose,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_PATH",
                format!(
                    "prepared path builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    if baseline_helper_imports && needs_semantic_dispatch {
        imports.push((
            native_semantic_dispatch_symbol.clone(),
            runtime_helpers.native_semantic_dispatch,
        ));
    }
    if baseline_helper_imports && needs_function_resolver {
        imports.push((
            native_function_resolve_symbol.clone(),
            runtime_helpers.native_function_resolve,
        ));
    }
    if baseline_helper_imports && needs_frame_arena {
        imports.push((
            "phrust_native_frame_alloc".to_owned(),
            runtime_helpers.native_frame_alloc,
        ));
        imports.push((
            "phrust_native_frame_release".to_owned(),
            runtime_helpers.native_frame_release,
        ));
    }
    if baseline_helper_imports && needs_dynamic_code {
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
            needs_value_release,
            runtime_helpers.native_value_release,
            test_native_value_release_fallback as *const () as usize,
            "phrust_native_value_release",
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
            needs_array_insert,
            runtime_helpers.native_array_insert_local,
            test_native_array_insert_fallback as *const () as usize,
            "phrust_native_array_insert_local",
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
            needs_string_predicate,
            runtime_helpers.native_string_predicate,
            test_native_string_predicate_fallback as *const () as usize,
            "phrust_native_string_predicate",
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
        if baseline_helper_imports && needed {
            let address = if configured == 0 {
                fallback
            } else {
                configured
            };
            imports.push((symbol.to_owned(), address));
        }
    }
    if needs_direct_echo {
        for (configured, fallback, symbol) in [
            (
                runtime_helpers.native_echo_bytes,
                test_native_echo_bytes_fallback as *const () as usize,
                "phrust_native_echo_bytes",
            ),
            (
                runtime_helpers.native_echo_int,
                test_native_echo_int_fallback as *const () as usize,
                "phrust_native_echo_int",
            ),
            (
                runtime_helpers.native_echo_float,
                test_native_echo_float_fallback as *const () as usize,
                "phrust_native_echo_float",
            ),
        ] {
            imports.push((
                symbol.to_owned(),
                if configured == 0 {
                    fallback
                } else {
                    configured
                },
            ));
        }
    }
    if needs_float_to_string {
        imports.push((
            "phrust_native_float_to_string".to_owned(),
            if runtime_helpers.native_float_to_string == 0 {
                test_native_float_to_string_fallback as *const () as usize
            } else {
                runtime_helpers.native_float_to_string
            },
        ));
    }
    if needs_float_to_int {
        imports.push((
            "phrust_native_float_to_int".to_owned(),
            if runtime_helpers.native_float_to_int == 0 {
                test_native_float_to_int_fallback as *const () as usize
            } else {
                runtime_helpers.native_float_to_int
            },
        ));
    }
    if needs_object_class_name {
        imports.push((
            "phrust_native_object_class_name".to_owned(),
            if runtime_helpers.native_object_class_name == 0 {
                test_native_object_class_name_fallback as *const () as usize
            } else {
                runtime_helpers.native_object_class_name
            },
        ));
    }
    if needs_prepared_object_new {
        imports.push((
            "phrust_native_prepared_object_new".to_owned(),
            if runtime_helpers.native_prepared_object_new == 0 {
                test_native_prepared_object_new_fallback as *const () as usize
            } else {
                runtime_helpers.native_prepared_object_new
            },
        ));
    }
    if needs_prepared_closure_new {
        imports.push((
            "phrust_native_prepared_closure_new".to_owned(),
            if runtime_helpers.native_prepared_closure_new == 0 {
                test_native_prepared_closure_new_fallback as *const () as usize
            } else {
                runtime_helpers.native_prepared_closure_new
            },
        ));
    }
    if needs_plain_object_clone {
        imports.push((
            "phrust_native_plain_object_clone".to_owned(),
            if runtime_helpers.native_plain_object_clone == 0 {
                test_native_plain_object_clone_fallback as *const () as usize
            } else {
                runtime_helpers.native_plain_object_clone
            },
        ));
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
            .deployment_identity
            .clone()
            .unwrap_or_else(|| crate::stable_ir_fingerprint(unit)),
        function.raw(),
        unit.functions[function.index()].params.len(),
        region.local_count,
        request.opt_level >= 2,
        request.invalidation_generation,
    );
    let compiled_clif_blocks = std::cell::Cell::new(None);
    let compiled_maximum_pre_regalloc = std::cell::Cell::new(None);
    let compiled_maximum_temporary_cache_entries = std::cell::Cell::new(None);
    let compiled_pre_regalloc_replans = std::cell::Cell::new(0_usize);
    let compiled = compile_managed_native(
        request,
        function,
        function_key,
        if compilation_mode
            == crate::cranelift_lowering::baseline_streaming::NativeCompilationMode::StreamingBaseline
        {
            crate::code_manager::NativeCompileAdmission::request_critical(
                plan.admission_cost_tokens(),
            )
        } else {
            crate::code_manager::NativeCompileAdmission::background_optimizing(
                plan.admission_cost_tokens(),
            )
        },
        compilation_mode.specialization(),
        &import_refs,
        |module, codegen_context, builder_context, name| {
            let mut active_plan = selected_plan.borrow().clone();
            let mut active_fragment_layout = selected_fragment_layout.borrow().clone();
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
            let mut exact_symbol_query = [None; StableSymbolQueryBuiltin::COUNT];
            for builtin in StableSymbolQueryBuiltin::all() {
                if !needs_exact_symbol_query[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                for _ in 0..6 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_symbol_query[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_pcre = [None; StablePcreBuiltin::COUNT];
            for builtin in StablePcreBuiltin::all() {
                if !needs_exact_pcre[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                for _ in 0..6 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_pcre[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_json = [None; StableJsonBuiltin::COUNT];
            for builtin in StableJsonBuiltin::all() {
                if !needs_exact_json[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                for _ in 0..6 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_json[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_format = [None; StableFormatBuiltin::COUNT];
            for builtin in StableFormatBuiltin::all() {
                if !needs_exact_format[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                for _ in 0..6 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_format[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_path = [None; StablePathBuiltin::COUNT];
            for builtin in StablePathBuiltin::all() {
                if !needs_exact_path[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                for _ in 0..6 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_path[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let (echo_bytes, echo_int, echo_float) = if needs_direct_echo {
                let mut bytes_signature = module.make_signature();
                bytes_signature.params.push(AbiParam::new(pointer_type));
                bytes_signature.params.push(AbiParam::new(types::I64));
                let bytes = declare_native_helper(
                    module,
                    "phrust_native_echo_bytes",
                    &bytes_signature,
                    helper_address("phrust_native_echo_bytes"),
                )?;
                let mut int_signature = module.make_signature();
                int_signature.params.push(AbiParam::new(types::I64));
                let int = declare_native_helper(
                    module,
                    "phrust_native_echo_int",
                    &int_signature,
                    helper_address("phrust_native_echo_int"),
                )?;
                let mut float_signature = module.make_signature();
                float_signature.params.push(AbiParam::new(types::F64));
                let float = declare_native_helper(
                    module,
                    "phrust_native_echo_float",
                    &float_signature,
                    helper_address("phrust_native_echo_float"),
                )?;
                (Some(bytes), Some(int), Some(float))
            } else {
                (None, None, None)
            };
            let float_to_string = if needs_float_to_string {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::F64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_float_to_string",
                    &signature,
                    helper_address("phrust_native_float_to_string"),
                )?)
            } else {
                None
            };
            let float_to_int = if needs_float_to_int {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::F64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_float_to_int",
                    &signature,
                    helper_address("phrust_native_float_to_int"),
                )?)
            } else {
                None
            };
            let object_class_name = if needs_object_class_name {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_object_class_name",
                    &signature,
                    helper_address("phrust_native_object_class_name"),
                )?)
            } else {
                None
            };
            let prepared_object_new = if needs_prepared_object_new {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_prepared_object_new",
                    &signature,
                    helper_address("phrust_native_prepared_object_new"),
                )?)
            } else {
                None
            };
            let prepared_closure_new = if needs_prepared_closure_new {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_prepared_closure_new",
                    &signature,
                    helper_address("phrust_native_prepared_closure_new"),
                )?)
            } else {
                None
            };
            let plain_object_clone = if needs_plain_object_clone {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_plain_object_clone",
                    &signature,
                    helper_address("phrust_native_plain_object_clone"),
                )?)
            } else {
                None
            };
            if needs_builtin_dispatch {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.builtin_dispatch = Some(declare_native_helper(
                    module,
                    &native_builtin_dispatch_symbol,
                    &signature,
                    helper_address(&native_builtin_dispatch_symbol),
                )?);
            }
            if needs_semantic_dispatch {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.semantic_dispatch = Some(declare_native_helper(
                    module,
                    &native_semantic_dispatch_symbol,
                    &signature,
                    helper_address(&native_semantic_dispatch_symbol),
                )?);
            }
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
                native_operations.unary = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_unary",
                    1,
                    helper_address("phrust_native_unary"),
                )?);
            }
            if needs_binary {
                native_operations.binary = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_binary",
                    4,
                    helper_address("phrust_native_binary"),
                )?);
            }
            if needs_compare {
                native_operations.compare = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_compare",
                    2,
                    helper_address("phrust_native_compare"),
                )?);
            }
            if needs_cast {
                native_operations.cast = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_cast",
                    1,
                    helper_address("phrust_native_cast"),
                )?);
            }
            if needs_echo {
                let mut signature = module.make_signature();
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
                native_operations.local_fetch = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_local_fetch",
                    5,
                    helper_address("phrust_native_local_fetch"),
                )?);
            }
            if needs_local_store {
                native_operations.local_store = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_local_store",
                    4,
                    helper_address("phrust_native_local_store"),
                )?);
            }
            if needs_value_release {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.value_release = Some(declare_native_helper(
                    module,
                    "phrust_native_value_release",
                    &signature,
                    helper_address("phrust_native_value_release"),
                )?);
            }
            if needs_reference_bind {
                native_operations.reference_bind = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_reference_bind",
                    3,
                    helper_address("phrust_native_reference_bind"),
                )?);
            }
            if needs_argument_check {
                native_operations.argument_check = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_argument_check",
                    5,
                    helper_address("phrust_native_argument_check"),
                )?);
            }
            if needs_return_check {
                native_operations.return_check = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_return_check",
                    2,
                    helper_address("phrust_native_return_check"),
                )?);
            }
            if needs_exception_new {
                native_operations.exception_new = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_exception_new",
                    3,
                    helper_address("phrust_native_exception_new"),
                )?);
            }
            if needs_array_new {
                native_operations.array_new = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_new",
                    0,
                    helper_address("phrust_native_array_new"),
                )?);
            }
            if needs_object_new {
                native_operations.object_new = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_object_new",
                    0,
                    helper_address("phrust_native_object_new"),
                )?);
            }
            if needs_property_fetch {
                native_operations.property_fetch = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_property_fetch",
                    3,
                    helper_address("phrust_native_property_fetch"),
                )?);
            }
            if needs_property_assign {
                native_operations.property_assign = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_property_assign",
                    4,
                    helper_address("phrust_native_property_assign"),
                )?);
            }
            if needs_object_clone {
                native_operations.object_clone = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_object_clone",
                    1,
                    helper_address("phrust_native_object_clone"),
                )?);
            }
            if needs_object_clone_with {
                native_operations.object_clone_with = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_object_clone_with",
                    2,
                    helper_address("phrust_native_object_clone_with"),
                )?);
            }
            if needs_array_insert {
                native_operations.array_insert = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_insert",
                    3,
                    helper_address("phrust_native_array_insert"),
                )?);
                native_operations.array_insert_local = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_insert_local",
                    3,
                    helper_address("phrust_native_array_insert_local"),
                )?);
            }
            if needs_array_fetch {
                native_operations.array_fetch = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_fetch",
                    2,
                    helper_address("phrust_native_array_fetch"),
                )?);
            }
            if needs_array_unset {
                native_operations.array_unset = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_unset",
                    2,
                    helper_address("phrust_native_array_unset"),
                )?);
            }
            if needs_array_spread {
                native_operations.array_spread = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_spread",
                    2,
                    helper_address("phrust_native_array_spread"),
                )?);
            }
            if needs_foreach_init {
                native_operations.foreach_init = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_foreach_init",
                    3,
                    helper_address("phrust_native_foreach_init"),
                )?);
            }
            if needs_foreach_next {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
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
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.foreach_cleanup = Some(declare_native_helper(
                    module,
                    "phrust_native_foreach_cleanup",
                    &signature,
                    helper_address("phrust_native_foreach_cleanup"),
                )?);
            }
            if needs_constant_fetch {
                native_operations.constant_fetch = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_constant_fetch",
                    2,
                    helper_address("phrust_native_constant_fetch"),
                )?);
            }
            if needs_truthy {
                let mut signature = module.make_signature();
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
                native_operations.type_predicate = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_type_predicate",
                    1,
                    helper_address("phrust_native_type_predicate"),
                )?);
            }
            if needs_stable_length {
                native_operations.stable_length = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_stable_length",
                    3,
                    helper_address("phrust_native_stable_length"),
                )?);
            }
            if needs_string_predicate {
                native_operations.string_predicate = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_string_predicate",
                    2,
                    helper_address("phrust_native_string_predicate"),
                )?);
            }
            if needs_runtime_fatal {
                let mut signature = module.make_signature();
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
            let synthetic_base = u32::try_from(unit.functions.len()).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                    "source unit function count does not fit the fragment symbol space",
                )
            })?;
            let mut next_synthetic = synthetic_base;
            let tier_operations = if baseline_helper_imports {
                NativeTierOperations::Baseline {
                    call: native_call_helper,
                    dynamic_code: native_dynamic_code_helper,
                    operations: native_operations,
                }
            } else {
                let array_ensure_unique_symbol = FunctionId::new(next_synthetic);
                next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                        "native optimizing-operation symbol id overflowed",
                    )
                })?;
                let symbol = format!("{name}.native.array_ensure_unique");
                let signature = direct_array_ensure_unique_signature(module);
                let array_ensure_unique = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare {symbol}: {error}"),
                        )
                    })?;
                functions.insert(array_ensure_unique_symbol, array_ensure_unique);
                let array_child_entry_symbol = FunctionId::new(next_synthetic);
                next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                        "native optimizing-operation symbol id overflowed",
                    )
                })?;
                let symbol = format!("{name}.native.array_child_entry");
                let signature = direct_array_child_entry_signature(module);
                let array_child_entry = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare {symbol}: {error}"),
                        )
                    })?;
                functions.insert(array_child_entry_symbol, array_child_entry);
                let value_release_validate_symbol = FunctionId::new(next_synthetic);
                next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                        "native value-release validator symbol id overflowed",
                    )
                })?;
                let symbol = format!("{name}.native.value_release_validate");
                let signature = direct_value_release_signature(module);
                let value_release_validate = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare {symbol}: {error}"),
                        )
                    })?;
                functions.insert(value_release_validate_symbol, value_release_validate);
                let value_release_commit_symbol = FunctionId::new(next_synthetic);
                next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                        "native value-release commit symbol id overflowed",
                    )
                })?;
                let symbol = format!("{name}.native.value_release_commit");
                let signature = direct_value_release_signature(module);
                let value_release_commit = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare {symbol}: {error}"),
                        )
                    })?;
                functions.insert(value_release_commit_symbol, value_release_commit);
                NativeTierOperations::Optimizing {
                    operations: NativeOptimizingOperations {
                        echo_bytes,
                        echo_int,
                        echo_float,
                        float_to_string,
                        float_to_int,
                        object_class_name,
                        prepared_object_new,
                        prepared_closure_new,
                        plain_object_clone,
                        exact_symbol_query,
                        exact_pcre,
                        exact_json,
                        exact_format,
                        exact_path,
                        array_ensure_unique,
                        array_ensure_unique_symbol,
                        array_child_entry,
                        array_child_entry_symbol,
                        value_release_validate,
                        value_release_validate_symbol,
                        value_release_commit,
                        value_release_commit_symbol,
                    },
                }
            };
            let (mut fragment_functions, mut fragment_symbols) =
                declare_fragment_functions(
                    module,
                    name,
                    region,
                    active_fragment_layout.as_ref(),
                    0,
                    &mut next_synthetic,
                    &mut functions,
                )?;
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
            let mut maximum_temporary_cache_entries = 0_usize;
            let mut native_pc_ranges = Vec::new();
            let mut relocatable_bytes = Vec::new();
            let mut relocatable_functions = Vec::new();
            let mut relocatable_relocations = Vec::new();
            let mut emitted_production_lowering = Vec::new();
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
                            (function.params.iter().any(|parameter| parameter.by_ref)
                                || function.returns_by_ref)
                                && !ir_function_requires_non_reference_trampoline(function),
                        ),
                    ))
                })
                .collect::<BTreeMap<_, _>>();
            function_params.extend(regions.iter().map(|(function, region)| {
                (
                    *function,
                    (
                        unit.functions[function.index()].name.clone(),
                        region.params.clone(),
                        trampoline_functions.contains(function),
                        region.arity(),
                        unit.functions
                            .get(function.index())
                            .is_some_and(|function| {
                                (function.params.iter().any(|parameter| parameter.by_ref)
                                    || function.returns_by_ref)
                                    && !ir_function_requires_non_reference_trampoline(function)
                            }),
                    ),
                )
            }));
            let mut preflighted_fragments = BTreeMap::<u32, DefinedRegionFunction>::new();
            // Fragmented optimizing functions need the same exact CLIF
            // preflight as streaming baseline functions. The cheap planner
            // estimate intentionally cannot account for the full live-state
            // fanout of direct guards; without this pass, one underestimated
            // fragment rejects the complete optimizing artifact only after
            // all preceding fragments have already been compiled.
            if active_fragment_layout.is_some() {
                for replan_attempt in 0..=MAX_PRE_REGALLOC_REPLAN_ATTEMPTS {
                    let mut offending_fragments = Vec::new();
                    let mut round_preflighted = BTreeMap::new();
                    if let Some(layout) = active_fragment_layout.as_ref() {
                        for fragment in &layout.fragments {
                            let compiler = crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
                                region.compile_metadata.tier,
                            );
                            let preflight = compiler.compile_fragment(&mut |mode| {
                                let func_id = if layout.fragments.len() == 1 {
                                    functions[&region.function]
                                } else {
                                    fragment_functions[&fragment.id]
                                };
                                define_region_graph_function(
                                    module,
                                    codegen_context,
                                    builder_context,
                                    region,
                                    &unit.constants,
                                    &value_flows[&region.function],
                                    func_id,
                                    &functions,
                                    &inline_constants,
                                    &tail_forwards,
                                    &function_params,
                                    &request.external_function_signatures,
                                    tier_operations,
                                    &layout.register_liveness,
                                    Some(NativeFragmentDefinition {
                                        layout,
                                        fragment,
                                        functions: &fragment_functions,
                                    }),
                                    runtime_unit_identity,
                                    mode,
                                    layout.fragments.len() == 1,
                                    true,
                                )
                            });
                            match preflight {
                                Ok(defined)
                                    if defined
                                        .pre_regalloc
                                        .exceeds_replan_margin(region.compile_metadata.tier) =>
                                {
                                    offending_fragments.push((
                                        fragment.id,
                                        defined
                                            .pre_regalloc
                                            .minimum_fragment_count(region.compile_metadata.tier),
                                    ));
                                }
                                Ok(defined) => {
                                    round_preflighted.insert(fragment.id, defined);
                                }
                                Err(error) if error.code == "JIT_CRANELIFT_PRE_REGALLOC_BUDGET" => {
                                    // A hard-limit rejection does not expose
                                    // trustworthy metrics. Bisect it and let
                                    // the next exact preflight size both
                                    // children before any regalloc work.
                                    offending_fragments.push((fragment.id, 2));
                                }
                                Err(error) => return Err(error),
                            }
                        }
                    }
                    if offending_fragments.is_empty() {
                        preflighted_fragments = round_preflighted;
                        break;
                    }
                    if replan_attempt == MAX_PRE_REGALLOC_REPLAN_ATTEMPTS {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_PRE_REGALLOC_REPLAN_LIMIT",
                            format!(
                                "fragments {offending_fragments:?} still exceed the exact pre-regalloc safety margin after {MAX_PRE_REGALLOC_REPLAN_ATTEMPTS} deterministic replan rounds"
                            ),
                        ));
                    }
                    // Refine every exact offender in the same deterministic
                    // round. Splitting only the first offender made the global
                    // attempt limit depend on how many independently large
                    // fragments a function happened to contain. Descending IDs
                    // keep lower fragment IDs stable while each split
                    // re-enumerates the plan.
                    offending_fragments.sort_unstable_by_key(|(fragment_id, _)| *fragment_id);
                    offending_fragments.dedup_by_key(|(fragment_id, _)| *fragment_id);
                    for (fragment_id, pieces) in offending_fragments.into_iter().rev() {
                        let block_shape = active_plan
                            .fragments
                            .iter()
                            .find(|fragment| fragment.id == fragment_id)
                            .map(|fragment| {
                                fragment
                                    .blocks
                                    .iter()
                                    .map(|block| {
                                        format!(
                                            "{}:{}",
                                            block.raw(),
                                            region.blocks[block.index()].instructions.len()
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(",")
                            })
                            .unwrap_or_default();
                        active_plan = active_plan.refine_fragment_into(region, fragment_id, pieces).ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_PRE_REGALLOC_UNSPLITTABLE",
                                format!(
                                    "function {} fragment {fragment_id} exceeds the exact pre-regalloc safety margin and contains no safe Region-block cut (block:instruction-count={block_shape})",
                                    region.function_name,
                                ),
                            )
                        })?;
                    }
                    compiled_pre_regalloc_replans
                        .set(compiled_pre_regalloc_replans.get().saturating_add(1));
                    active_fragment_layout =
                        Some(NativeFunctionFragmentLayout::for_plan(region, &active_plan)?);
                    (fragment_functions, fragment_symbols) = declare_fragment_functions(
                        module,
                        name,
                        region,
                        active_fragment_layout.as_ref(),
                        replan_attempt + 1,
                        &mut next_synthetic,
                        &mut functions,
                    )?;
                }
            }
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
                maximum_temporary_cache_entries = maximum_temporary_cache_entries
                    .max(defined.maximum_temporary_cache_entries);
                relocatable_bytes.extend_from_slice(&defined.code);
                for relocation in &mut defined.relocations {
                    relocation.offset = relocation.offset.saturating_add(code_offset);
                }
                relocatable_relocations.append(&mut defined.relocations);
                emitted_production_lowering.append(&mut defined.production_lowering);
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
            if let NativeTierOperations::Optimizing { operations } = tier_operations {
                let defined = define_direct_array_ensure_unique_function(
                    module,
                    codegen_context,
                    builder_context,
                    operations.array_ensure_unique,
                )?;
                let _ = append_defined(
                    operations.array_ensure_unique_symbol,
                    0,
                    0,
                    defined,
                )?;
                let defined = define_direct_array_child_entry_function(
                    module,
                    codegen_context,
                    builder_context,
                    operations.array_child_entry,
                )?;
                let _ = append_defined(
                    operations.array_child_entry_symbol,
                    0,
                    0,
                    defined,
                )?;
                let defined = define_direct_value_release_validate_function(
                    module,
                    codegen_context,
                    builder_context,
                    operations.value_release_validate,
                    operations.value_release_validate_symbol,
                )?;
                let _ = append_defined(
                    operations.value_release_validate_symbol,
                    0,
                    0,
                    defined,
                )?;
                let defined = define_direct_value_release_commit_function(
                    module,
                    codegen_context,
                    builder_context,
                    operations.value_release_commit,
                    operations.value_release_commit_symbol,
                )?;
                let _ = append_defined(
                    operations.value_release_commit_symbol,
                    0,
                    0,
                    defined,
                )?;
            }
            for candidate in regions.values() {
                if let Some(layout) = &active_fragment_layout {
                    let mut function_bytes = 0_u64;
                    let mut maximum_stack = 0_u32;
                    if layout.fragments.len() == 1 {
                        let fragment = &layout.fragments[0];
                        let defined = if let Some(preflighted) =
                            preflighted_fragments.remove(&fragment.id)
                        {
                            compile_preflighted_region_function(
                                module,
                                codegen_context,
                                functions[&candidate.function],
                                candidate,
                                &functions,
                                preflighted,
                            )?
                        } else {
                            let compiler =
                                crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
                                    candidate.compile_metadata.tier,
                                );
                            compiler.compile_fragment(&mut |compilation_mode| {
                                define_region_graph_function(
                                    module,
                                    codegen_context,
                                    builder_context,
                                    candidate,
                                    &unit.constants,
                                    &value_flows[&candidate.function],
                                    functions[&candidate.function],
                                    &functions,
                                    &inline_constants,
                                    &tail_forwards,
                                    &function_params,
                                    &request.external_function_signatures,
                                    tier_operations,
                                    &layout.register_liveness,
                                    Some(NativeFragmentDefinition {
                                        layout,
                                        fragment,
                                        functions: &fragment_functions,
                                    }),
                                    runtime_unit_identity,
                                    compilation_mode,
                                    true,
                                    false,
                                )
                            })?
                        };
                        let metrics = append_defined(
                            candidate.function,
                            region_arity(candidate)?,
                            candidate.local_count,
                            defined,
                        )?;
                        function_code_metrics.insert(candidate.function, metrics);
                        continue;
                    }
                    for fragment in &layout.fragments {
                        let defined = if let Some(preflighted) =
                            preflighted_fragments.remove(&fragment.id)
                        {
                            compile_preflighted_region_function(
                                module,
                                codegen_context,
                                fragment_functions[&fragment.id],
                                candidate,
                                &functions,
                                preflighted,
                            )?
                        } else {
                            let compiler =
                                crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
                                    candidate.compile_metadata.tier,
                                );
                            compiler.compile_fragment(&mut |compilation_mode| {
                                define_region_graph_function(
                                    module,
                                    codegen_context,
                                    builder_context,
                                    candidate,
                                    &unit.constants,
                                    &value_flows[&candidate.function],
                                    fragment_functions[&fragment.id],
                                    &functions,
                                    &inline_constants,
                                    &tail_forwards,
                                    &function_params,
                                    &request.external_function_signatures,
                                    tier_operations,
                                    &layout.register_liveness,
                                    Some(NativeFragmentDefinition {
                                        layout,
                                        fragment,
                                        functions: &fragment_functions,
                                    }),
                                    runtime_unit_identity,
                                    compilation_mode,
                                    false,
                                    false,
                                )
                            })?
                        };
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
                        &fragment_functions,
                        layout,
                        &functions,
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
                    let register_liveness = NativeRegisterLiveness::analyze(candidate);
                    let compiler =
                        crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
                            candidate.compile_metadata.tier,
                        );
                    let defined = compiler.compile_fragment(&mut |compilation_mode| {
                        define_region_graph_function(
                            module,
                            codegen_context,
                            builder_context,
                            candidate,
                            &unit.constants,
                            &value_flows[&candidate.function],
                            functions[&candidate.function],
                            &functions,
                            &inline_constants,
                            &tail_forwards,
                            &function_params,
                            &request.external_function_signatures,
                            tier_operations,
                            &register_liveness,
                            None,
                            runtime_unit_identity,
                            compilation_mode,
                            false,
                            false,
                        )
                    })?;
                    let metrics = append_defined(
                        candidate.function,
                        region_arity(candidate)?,
                        candidate.local_count,
                        defined,
                    )?;
                    function_code_metrics.insert(candidate.function, metrics);
                }
            }
            drop(append_defined);
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
                                        && function_params.get(&target).is_some_and(
                                            |(
                                                _,
                                                params,
                                                requires_trampoline,
                                                arity,
                                                reference_only_trampoline,
                                            )| {
                                                *arity == call.operands.len()
                                                    && (!*requires_trampoline
                                                        || (*reference_only_trampoline
                                                            && params.iter().enumerate().all(
                                                                |(index, parameter)| {
                                                                    !parameter.by_ref
                                                                        || call.args.get(index).is_some_and(
                                                                            |argument| {
                                                                                argument.by_ref_local.is_some()
                                                                                    && argument.by_ref_dim.is_none()
                                                                                    && argument.by_ref_property.is_none()
                                                                                    && argument.by_ref_property_dim.is_none()
                                                                            },
                                                                        )
                                                                },
                                                            )))
                                            },
                                        )
                                        && !matches!(
                                            call.result,
                                            RegionCallResult::ReferenceLocal(_)
                                        )
                                        && call.args.iter().all(|argument| {
                                            argument.name.is_none() && !argument.unpack
                                        })
                                        && !(call.operands.is_empty()
                                            && inline_constants.contains_key(&target))
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
                                                .is_some_and(|(_, _, requires_trampoline, _, _)| {
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
                                        .and_then(|value| bounded_inline_call_operand(call, value))
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
                                        .and_then(|value| bounded_inline_call_operand(call, value))
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
                active_fragment_layout
                    .as_ref()
                    .map(|layout| &layout.register_liveness),
                &value_flows,
                emitted_production_lowering,
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
            if compilation_mode
                == crate::cranelift_lowering::baseline_streaming::NativeCompilationMode::SsaOptimizing
            {
                let forbidden = relocatable_relocations.iter().find_map(|relocation| {
                    match &relocation.target {
                        crate::JitRelocatableTarget::Helper(symbol)
                            if matches!(
                                symbol.as_str(),
                                "phrust_native_defined"
                                    | "phrust_native_echo_bytes"
                                    | "phrust_native_echo_int"
                                    | "phrust_native_echo_float"
                                    | "phrust_native_float_to_string"
                                    | "phrust_native_float_to_int"
                                    | "phrust_native_object_class_name"
                                    | "phrust_native_prepared_object_new"
                                    | "phrust_native_prepared_closure_new"
                                    | "phrust_native_plain_object_clone"
                                    | "phrust_native_function_exists"
                                    | "phrust_native_class_exists"
                                    | "phrust_native_interface_exists"
                                    | "phrust_native_trait_exists"
                                    | "phrust_native_enum_exists"
                                    | "phrust_native_method_exists"
                                    | "phrust_native_property_exists"
                            )
                                || symbol.starts_with("phrust_native_preg_")
                                || symbol.starts_with("phrust_native_json_")
                                || matches!(
                                    symbol.as_str(),
                                    "phrust_native_sprintf"
                                        | "phrust_native_printf"
                                        | "phrust_native_vsprintf"
                                        | "phrust_native_vprintf"
                                        | "phrust_native_basename"
                                        | "phrust_native_dirname"
                                        | "phrust_native_realpath"
                                        | "phrust_native_file_exists"
                                        | "phrust_native_fopen"
                                        | "phrust_native_fwrite"
                                        | "phrust_native_fclose"
                                ) =>
                        {
                            None
                        }
                        crate::JitRelocatableTarget::Helper(symbol) => Some(symbol.as_str()),
                        crate::JitRelocatableTarget::InternalFunction(_) => None,
                    }
                });
                if let Some(symbol) = forbidden {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_OPTIMIZER_HELPER_IMPORT",
                        format!(
                            "optimizing artifact attempted to publish forbidden runtime import {symbol}"
                        ),
                    ));
                }
            }
            handle.bind_relocatable_code(crate::JitRelocatableCode {
                root: function,
                code: relocatable_bytes,
                functions: relocatable_functions,
                relocations: relocatable_relocations,
            });
            compiled_clif_blocks.set(Some(clif_blocks));
            compiled_maximum_pre_regalloc.set(Some(maximum_pre_regalloc));
            compiled_maximum_temporary_cache_entries
                .set(Some(maximum_temporary_cache_entries));
            *selected_plan.borrow_mut() = active_plan;
            *selected_fragment_layout.borrow_mut() = active_fragment_layout;
            Ok((handle, code_bytes))
        },
    )?;
    let plan = selected_plan.into_inner();
    let fragment_layout = selected_fragment_layout.into_inner();
    let fragment_frame_metrics = fragment_layout.as_ref().map_or((0, 0, 0), |layout| {
        (
            layout.frame.value_slots,
            layout.frame.shared_register_slots,
            layout.frame.scratch_register_slots,
        )
    });
    let mut handle = compiled.handle;
    handle.bind_ssa_metrics(ssa_metrics.0, ssa_metrics.1, ssa_metrics.2);
    Ok(NativeScalarRegionCompileResult {
        handle,
        code_bytes: compiled.code_bytes,
        clif_blocks: compiled_clif_blocks.get(),
        maximum_pre_regalloc: compiled_maximum_pre_regalloc.get(),
        maximum_temporary_cache_entries: compiled_maximum_temporary_cache_entries.get(),
        fragment_frame_slots: fragment_frame_metrics.0,
        fragment_shared_register_slots: fragment_frame_metrics.1,
        fragment_scratch_register_slots: fragment_frame_metrics.2,
        pre_regalloc_replans: compiled_pre_regalloc_replans.get(),
        fast_path_hits,
        has_control_flow,
        compilation_mode,
        plan,
    })
}

pub(super) fn select_native_region_tier(
    candidate: &mut RegionGraph,
    _plan: &NativeCompilePlan,
    _constants: &[IrConstant],
) {
    // Baseline and optimizing code must share real CFG boundaries around a
    // baseline-only island.  Otherwise baseline has no edge on which it can
    // re-enter the published optimizing continuation and one unsupported
    // operation silently downgrades the rest of the PHP block.
    *candidate = prepare_optimizing_baseline_islands(candidate.clone());
    if candidate.compile_metadata.tier == NativeCompilerTier::Optimizing {
        let _ = crate::region_ir::opt::optimize_executable_region(candidate);
        // Optimization may remove instructions, but it must not collapse a
        // direct/unsupported family boundary into one lowering block.
        *candidate = prepare_optimizing_baseline_islands(candidate.clone());
    }
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

fn region_instruction_defined_registers(kind: &RegionInstructionKind) -> Vec<RegId> {
    let mut registers = region_instruction_result_register(kind)
        .into_iter()
        .collect::<Vec<_>>();
    match kind {
        RegionInstructionKind::ArrayInsert { array, .. }
        | RegionInstructionKind::ArraySpread { array, .. } => registers.push(*array),
        RegionInstructionKind::ForeachNext { key, value, .. } => {
            registers.extend(*key);
            registers.push(*value);
        }
        RegionInstructionKind::ForeachNextRef { key, .. } => registers.extend(*key),
        _ => {}
    }
    registers.sort_unstable();
    registers.dedup();
    registers
}

fn region_register_types(region: &RegionGraph) -> BTreeMap<RegId, ir::Type> {
    region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .flat_map(|instruction| {
            region_instruction_defined_registers(&instruction.kind)
                .into_iter()
                .map(|register| (register, types::I64))
        })
        .collect()
}

/// Deliberately tiny first inlining tier. It handles only a stable zero-arity
/// function whose complete body returns one scalar constant. This preserves a
/// hard code-growth bound and cannot recursively inline a call graph.
fn bounded_inline_return(region: &RegionGraph) -> Option<BoundedInlineValue> {
    if region.return_type.is_some()
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
        "not-bounded-wrapper"
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

fn direct_array_ensure_unique_signature(module: &JITModule) -> Signature {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I8));
    signature.returns.push(AbiParam::new(types::I32));
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn direct_array_child_entry_signature(module: &JITModule) -> Signature {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(pointer_type));
    signature
}

fn direct_value_release_signature(module: &JITModule) -> Signature {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(types::I8));
    signature
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
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    Ok(signature)
}

#[allow(clippy::too_many_arguments)]
fn declare_fragment_functions(
    module: &mut JITModule,
    root_symbol: &str,
    region: &RegionGraph,
    layout: Option<&NativeFunctionFragmentLayout>,
    replan_attempt: usize,
    next_synthetic: &mut u32,
    functions: &mut BTreeMap<FunctionId, FuncId>,
) -> Result<(BTreeMap<u32, FuncId>, BTreeMap<u32, FunctionId>), CraneliftLoweringError> {
    let mut fragment_functions = BTreeMap::new();
    let mut fragment_symbols = BTreeMap::new();
    let Some(layout) = layout else {
        return Ok((fragment_functions, fragment_symbols));
    };
    if layout.fragments.len() == 1 {
        fragment_functions.insert(layout.fragments[0].id, functions[&region.function]);
        return Ok((fragment_functions, fragment_symbols));
    }
    for fragment in &layout.fragments {
        let synthetic = FunctionId::new(*next_synthetic);
        *next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                "native fragment symbol id overflowed",
            )
        })?;
        let symbol = if replan_attempt == 0 {
            format!("{root_symbol}.fragment.{}", fragment.id)
        } else {
            format!(
                "{root_symbol}.replan.{replan_attempt}.fragment.{}",
                fragment.id
            )
        };
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
    Ok((fragment_functions, fragment_symbols))
}

pub(super) struct DefinedRegionFunction {
    lowered_function: Option<ir::Function>,
    code: Vec<u8>,
    clif_blocks: usize,
    alignment: u64,
    relocations: Vec<crate::JitRelocatableRelocation>,
    native_pc_ranges: Vec<crate::JitNativePcRange>,
    native_stack_bytes: u32,
    pre_regalloc: PreRegallocMetrics,
    maximum_temporary_cache_entries: usize,
    production_lowering: Vec<crate::JitProductionLoweringMetadata>,
}

const MAX_NATIVE_SPILL_FRAME_BYTES: u32 = 1024 * 1024;
const MAX_FRAGMENT_CLIF_BLOCKS: usize = 768;
const MAX_OPTIMIZING_CLIF_BLOCKS: usize = 4_096;
const MAX_FRAGMENT_CLIF_VALUES: usize = 16_384;
const MAX_OPTIMIZING_CLIF_VALUES: usize = 65_536;
const MAX_FRAGMENT_CLIF_INSTRUCTIONS: usize = 32_768;
const MAX_OPTIMIZING_CLIF_INSTRUCTIONS: usize = 65_536;
const MAX_FRAGMENT_BLOCK_PARAMETERS: usize = 4_096;
const MAX_OPTIMIZING_BLOCK_PARAMETERS: usize = 16_384;
// Exact CLIF must retain 30% headroom below the absolute backend ceiling.
// This is intentionally stricter than merely avoiding a hard rejection: it
// keeps the admitted regalloc graph away from the nonlinear edge while the
// planner's cheaper estimate remains calibrated independently.
const PRE_REGALLOC_REPLAN_MARGIN_PERCENT: usize = 70;
// The planner admits at most 64 Region blocks per fragment. Six bisection
// rounds are therefore sufficient to reduce every splittable offender to one
// Region block (ceil(log2(64))). A remaining offender is structurally
// unsplittable and is rejected before regalloc; this is a proof-derived bound,
// not a wall-time retry budget.
const MAX_PRE_REGALLOC_REPLAN_ATTEMPTS: usize = 6;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct PreRegallocMetrics {
    pub(super) blocks: usize,
    pub(super) values: usize,
    pub(super) instructions: usize,
    pub(super) block_parameters: usize,
    pub(super) loads: usize,
    pub(super) stores: usize,
    pub(super) loads_per_source_instruction_milli: usize,
    pub(super) stores_per_source_instruction_milli: usize,
}

impl PreRegallocMetrics {
    fn limits(tier: NativeCompilerTier) -> (usize, usize, usize, usize) {
        if tier == NativeCompilerTier::Optimizing {
            (
                MAX_OPTIMIZING_CLIF_BLOCKS,
                MAX_OPTIMIZING_CLIF_VALUES,
                MAX_OPTIMIZING_CLIF_INSTRUCTIONS,
                MAX_OPTIMIZING_BLOCK_PARAMETERS,
            )
        } else {
            (
                MAX_FRAGMENT_CLIF_BLOCKS,
                MAX_FRAGMENT_CLIF_VALUES,
                MAX_FRAGMENT_CLIF_INSTRUCTIONS,
                MAX_FRAGMENT_BLOCK_PARAMETERS,
            )
        }
    }

    fn exceeds_replan_margin(self, tier: NativeCompilerTier) -> bool {
        let (blocks, values, instructions, parameters) = Self::limits(tier);
        self.blocks.saturating_mul(100) > blocks.saturating_mul(PRE_REGALLOC_REPLAN_MARGIN_PERCENT)
            || self.values.saturating_mul(100)
                > values.saturating_mul(PRE_REGALLOC_REPLAN_MARGIN_PERCENT)
            || self.instructions.saturating_mul(100)
                > instructions.saturating_mul(PRE_REGALLOC_REPLAN_MARGIN_PERCENT)
            || self.block_parameters.saturating_mul(100)
                > parameters.saturating_mul(PRE_REGALLOC_REPLAN_MARGIN_PERCENT)
    }

    /// Minimum number of approximately balanced fragments required by the
    /// largest exact CLIF dimension. This is a planning hint only: every
    /// resulting fragment is exact-preflighted again before regalloc.
    fn minimum_fragment_count(self, tier: NativeCompilerTier) -> usize {
        let percent = PRE_REGALLOC_REPLAN_MARGIN_PERCENT;
        let (blocks, values, instructions, parameters) = Self::limits(tier);
        let block_limit = blocks.saturating_mul(percent) / 100;
        let value_limit = values.saturating_mul(percent) / 100;
        let instruction_limit = instructions.saturating_mul(percent) / 100;
        let parameter_limit = parameters.saturating_mul(percent) / 100;
        [
            self.blocks.div_ceil(block_limit.max(1)),
            self.values.div_ceil(value_limit.max(1)),
            self.instructions.div_ceil(instruction_limit.max(1)),
            self.block_parameters.div_ceil(parameter_limit.max(1)),
        ]
        .into_iter()
        .max()
        .unwrap_or(2)
        .max(2)
    }

    fn max_assign(&mut self, other: Self) {
        self.blocks = self.blocks.max(other.blocks);
        self.values = self.values.max(other.values);
        self.instructions = self.instructions.max(other.instructions);
        self.block_parameters = self.block_parameters.max(other.block_parameters);
        self.loads = self.loads.max(other.loads);
        self.stores = self.stores.max(other.stores);
        self.loads_per_source_instruction_milli = self
            .loads_per_source_instruction_milli
            .max(other.loads_per_source_instruction_milli);
        self.stores_per_source_instruction_milli = self
            .stores_per_source_instruction_milli
            .max(other.stores_per_source_instruction_milli);
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
    let mut loads = 0_usize;
    let mut stores = 0_usize;
    for block in function.layout.blocks() {
        for instruction in function.layout.block_insts(block) {
            match function.dfg.insts[instruction].opcode() {
                ir::Opcode::Load | ir::Opcode::StackLoad => loads = loads.saturating_add(1),
                ir::Opcode::Store | ir::Opcode::StackStore => stores = stores.saturating_add(1),
                _ => {}
            }
        }
    }
    let (maximum_blocks, maximum_values, maximum_instructions, maximum_block_parameters) =
        if region.compile_metadata.tier == NativeCompilerTier::Optimizing {
            (
                MAX_OPTIMIZING_CLIF_BLOCKS,
                MAX_OPTIMIZING_CLIF_VALUES,
                MAX_OPTIMIZING_CLIF_INSTRUCTIONS,
                MAX_OPTIMIZING_BLOCK_PARAMETERS,
            )
        } else {
            (
                MAX_FRAGMENT_CLIF_BLOCKS,
                MAX_FRAGMENT_CLIF_VALUES,
                MAX_FRAGMENT_CLIF_INSTRUCTIONS,
                MAX_FRAGMENT_BLOCK_PARAMETERS,
            )
        };
    if blocks > maximum_blocks
        || values > maximum_values
        || instructions > maximum_instructions
        || block_parameters > maximum_block_parameters
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_PRE_REGALLOC_BUDGET",
            format!(
                "function {} fragment={} exceeds the pre-regalloc ceiling: clif_blocks={blocks}/{maximum_blocks} clif_values={values}/{maximum_values} clif_instructions={instructions}/{maximum_instructions} block_parameters={block_parameters}/{maximum_block_parameters}",
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
        loads,
        stores,
        loads_per_source_instruction_milli: 0,
        stores_per_source_instruction_milli: 0,
    })
}

fn compile_preflighted_region_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    func_id: FuncId,
    region: &RegionGraph,
    functions: &BTreeMap<FunctionId, FuncId>,
    mut defined: DefinedRegionFunction,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    ctx.func = defined.lowered_function.take().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_MISSING_PREFLIGHT_CLIF",
            "exact preflight did not retain its verified CLIF function",
        )
    })?;
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define preflighted native function: {error}"),
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
    defined.code = compiled.code_buffer().to_vec();
    defined.alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    defined.relocations = compiled
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
    defined.native_pc_ranges = ctx
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
    defined.native_stack_bytes = native_stack_bytes;
    module.clear_context(ctx);
    Ok(defined)
}

#[allow(clippy::too_many_arguments)]
fn define_region_fragment_wrapper(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    region: &RegionGraph,
    func_id: FuncId,
    fragment_functions: &BTreeMap<u32, FuncId>,
    layout: &NativeFunctionFragmentLayout,
    relocation_functions: &BTreeMap<FunctionId, FuncId>,
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
        let runtime = params[0];
        let arguments = params[1];
        let result_out = params[2];
        let deopt_out = params[3];
        let resume_id = params[4];
        let resume_state = params[5];
        let frame_layout = &layout.frame;
        let frame_bytes = frame_layout.frame_bytes()?;
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
        for local in frame_layout.local_slots.keys().copied() {
            builder.ins().store(
                MemFlagsData::new(),
                uninitialized,
                frame,
                frame_layout.local_offset(local)?,
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
                frame_layout.local_offset(*local)?,
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
            frame_layout.pending_status_offset(),
        );
        builder.ins().store(
            MemFlagsData::new(),
            empty,
            frame,
            frame_layout.pending_value_offset(),
        );
        for (value, offset) in [
            (arguments, frame_layout.arguments_offset()),
            (result_out, frame_layout.result_out_offset()),
            (deopt_out, frame_layout.deopt_out_offset()),
            (resume_state, frame_layout.resume_state_offset()),
        ] {
            builder
                .ins()
                .store(MemFlagsData::new(), value, frame, offset);
        }
        builder.ins().store(
            MemFlagsData::new(),
            resume_id,
            frame,
            frame_layout.resume_id_offset(),
        );

        let call_blocks = layout
            .fragments
            .iter()
            .map(|fragment| (fragment.id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let root_fragment = layout.block_owner[&BlockId::new(0)];
        if layout.fragments.len() == 1 {
            builder.ins().jump(call_blocks[&root_fragment], &[]);
        } else {
            // Cranelift lowers a sparse `Switch` to control-flow blocks for
            // every resume id. Large PHP functions have hundreds of precise
            // transition ids, so that representation made this tiny wrapper
            // larger than a bounded fragment. Match all ids owned by a
            // fragment in one straight-line predicate instead. Intermediate
            // compare values die immediately and the wrapper CFG now scales
            // with the number of fragments, not the number of safepoints.
            for fragment in &layout.fragments {
                if fragment.id == root_fragment {
                    continue;
                }
                let mut matches_fragment = None;
                for encoded_resume in
                    layout
                        .resume_owner
                        .iter()
                        .filter_map(|(encoded_resume, owner)| {
                            (*owner == fragment.id).then_some(*encoded_resume)
                        })
                {
                    let matches_resume =
                        builder
                            .ins()
                            .icmp_imm(IntCC::Equal, resume_id, i64::from(encoded_resume));
                    matches_fragment = Some(match matches_fragment {
                        Some(previous) => builder.ins().bor(previous, matches_resume),
                        None => matches_resume,
                    });
                }
                if let Some(matches_fragment) = matches_fragment {
                    let next_fragment = builder.create_block();
                    builder.ins().brif(
                        matches_fragment,
                        call_blocks[&fragment.id],
                        &[],
                        next_fragment,
                        &[],
                    );
                    builder.switch_to_block(next_fragment);
                }
            }
            builder.ins().jump(call_blocks[&root_fragment], &[]);
        }

        for fragment in &layout.fragments {
            builder.switch_to_block(call_blocks[&fragment.id]);
            let callee =
                module.declare_func_in_func(fragment_functions[&fragment.id], builder.func);
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
                frame_layout.entry_id_offset(),
            );
            let call = builder.ins().call(callee, &[runtime, frame]);
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
                relocation_functions,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations,
        native_pc_ranges: Vec::new(),
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries: 0,
        production_lowering: Vec::new(),
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
    value_flow: &ExecutableValueFlow,
    func_id: FuncId,
    functions: &BTreeMap<FunctionId, FuncId>,
    inline_constants: &BTreeMap<FunctionId, BoundedInlineValue>,
    tail_forwards: &BTreeMap<(FunctionId, u32), FunctionId>,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    tier_operations: NativeTierOperations,
    register_liveness: &NativeRegisterLiveness,
    fragment: Option<NativeFragmentDefinition<'_>>,
    unit_identity: u64,
    compilation_mode: crate::cranelift_lowering::baseline_streaming::NativeCompilationMode,
    inline_fragment_entry: bool,
    preflight_only: bool,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let mut maximum_temporary_cache_entries = 0_usize;
    let mut production_lowering = Vec::new();
    ctx.func.signature = if fragment.is_some() && !inline_fragment_entry {
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
        // An optimizing guard failure transfers once to the matching
        // baseline-native continuation. Baseline code deliberately remains
        // in that tier until the PHP call returns: instruction/block-level
        // ping-pong required two independently computed sparse-live layouts
        // to share a positional ABI and could silently restore one register
        // into another. It also rebuilt transition state at every CFG edge.
        let terminator_blocks = blocks.clone();
        // Only true resumable native transitions need an instruction-entry
        // block. Ordinary Region instructions are lowered directly into their
        // PHP CFG block (or the continuation block created by a fallible
        // helper). Creating an entry block for every instruction turns a
        // large but ordinary PHP function into a pathological Cranelift CFG
        // before regalloc2 sees it.
        let transition_blocks = owned_blocks
            .iter()
            .flat_map(|block| {
                block
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        instruction_has_native_resume_entry(
                            instruction,
                            region.compile_metadata.tier,
                        )
                    })
                    .map(|instruction| instruction.continuation_id)
                    .chain(
                        block_terminator_has_native_transition(block, region.compile_metadata.tier)
                            .then_some(block.terminator_continuation_id),
                    )
            })
            .map(|continuation| (continuation, builder.create_block()))
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
        let runtime = params[0];
        let frame_layout = fragment.map(|fragment| &fragment.layout.frame);
        let fragment_frame = if fragment.is_some() {
            if inline_fragment_entry {
                let frame_bytes = frame_layout
                    .expect("inline fragment frame layout")
                    .frame_bytes()?;
                let frame_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    frame_bytes,
                    3,
                ));
                Some(builder.ins().stack_addr(pointer_type, frame_slot, 0))
            } else {
                Some(params[1])
            }
        } else {
            None
        };
        let streaming_state_frame = if compilation_mode.streams_cfg_state_through_slots() {
            fragment_frame
        } else {
            None
        };
        let (arguments, result_out, deopt_out, resume_id, resume_state, fragment_entry_id) =
            if let Some(frame) = fragment_frame {
                let layout = frame_layout.expect("fragment frame layout");
                let (arguments, result_out, deopt_out, resume_id, resume_state, entry_id) =
                    if inline_fragment_entry {
                        let entry_id = builder.ins().iconst(types::I32, 0);
                        for (value, offset) in [
                            (params[1], layout.arguments_offset()),
                            (params[2], layout.result_out_offset()),
                            (params[3], layout.deopt_out_offset()),
                            (params[5], layout.resume_state_offset()),
                        ] {
                            builder
                                .ins()
                                .store(MemFlagsData::new(), value, frame, offset);
                        }
                        builder.ins().store(
                            MemFlagsData::new(),
                            params[4],
                            frame,
                            layout.resume_id_offset(),
                        );
                        builder.ins().store(
                            MemFlagsData::new(),
                            entry_id,
                            frame,
                            layout.entry_id_offset(),
                        );
                        (
                            params[1], params[2], params[3], params[4], params[5], entry_id,
                        )
                    } else {
                        (
                            builder.ins().load(
                                pointer_type,
                                MemFlagsData::new(),
                                frame,
                                layout.arguments_offset(),
                            ),
                            builder.ins().load(
                                pointer_type,
                                MemFlagsData::new(),
                                frame,
                                layout.result_out_offset(),
                            ),
                            builder.ins().load(
                                pointer_type,
                                MemFlagsData::new(),
                                frame,
                                layout.deopt_out_offset(),
                            ),
                            builder.ins().load(
                                types::I32,
                                MemFlagsData::new(),
                                frame,
                                layout.resume_id_offset(),
                            ),
                            builder.ins().load(
                                pointer_type,
                                MemFlagsData::new(),
                                frame,
                                layout.resume_state_offset(),
                            ),
                            builder.ins().load(
                                types::I32,
                                MemFlagsData::new(),
                                frame,
                                layout.entry_id_offset(),
                            ),
                        )
                    };
                (
                    arguments,
                    result_out,
                    deopt_out,
                    resume_id,
                    resume_state,
                    Some(entry_id),
                )
            } else {
                (params[1], params[2], params[3], params[4], params[5], None)
            };
        let (native_call_helper, native_dynamic_code_helper, mut native_operations) =
            match tier_operations {
                NativeTierOperations::Baseline {
                    call,
                    dynamic_code,
                    operations,
                } => (
                    call.map(|helper| helper.with_runtime(runtime)),
                    dynamic_code.map(|helper| helper.with_runtime(runtime)),
                    operations
                        .with_runtime(runtime)
                        .with_terminal_exit(NativeTerminalExit {
                            block: terminal_exit,
                        }),
                ),
                NativeTierOperations::Optimizing { .. } => {
                    (None, None, NativeOperationFunctions::default())
                }
            };
        // These guards read the request-owned runtime view directly and only
        // call Rust for reference, warning, destructor, or unsupported dynamic
        // cases. Baseline code needs the same fast paths: forcing every local,
        // scalar comparison, and retain/release through helpers dominated warm
        // execution long after compilation had finished.
        native_operations.value_release = native_operations
            .value_release
            .map(NativeHelper::with_inline_runtime_view);
        native_operations.compare = native_operations
            .compare
            .map(NativeHelper::with_inline_runtime_view);
        native_operations.local_fetch = native_operations
            .local_fetch
            .map(NativeHelper::with_inline_runtime_view);
        native_operations.local_store = native_operations
            .local_store
            .map(NativeHelper::with_inline_runtime_view);
        native_operations.truthy = native_operations
            .truthy
            .map(NativeHelper::with_inline_runtime_view);
        native_operations.type_predicate = native_operations
            .type_predicate
            .map(NativeHelper::with_inline_runtime_view);
        native_operations.stable_length = native_operations
            .stable_length
            .map(NativeHelper::with_inline_runtime_view);
        native_operations.array_fetch = native_operations
            .array_fetch
            .map(NativeHelper::with_inline_runtime_view);
        native_operations.foreach_next = native_operations
            .foreach_next
            .map(NativeHelper::with_inline_runtime_view);
        native_operations.string_predicate = native_operations
            .string_predicate
            .map(NativeHelper::with_inline_runtime_view);
        let local_ids = fragment.map_or_else(
            || {
                (0..region.local_count)
                    .map(LocalId::new)
                    .collect::<BTreeSet<_>>()
            },
            |fragment| fragment.fragment.locals.clone(),
        );
        let locals = if let Some(frame) = streaming_state_frame {
            let layout = frame_layout.expect("streaming frame layout");
            local_ids
                .into_iter()
                .map(|local| {
                    Ok((
                        local,
                        NativeLocalStorage::FrameSlot {
                            frame,
                            offset: layout.local_offset(local)?,
                        },
                    ))
                })
                .collect::<Result<NativeLocalMap, CraneliftLoweringError>>()?
        } else {
            local_ids
                .into_iter()
                .map(|local| {
                    (
                        local,
                        NativeLocalStorage::Variable(builder.declare_var(types::I64)),
                    )
                })
                .collect::<NativeLocalMap>()
        };
        let streaming_call_exit = streaming_state_frame
            .filter(|_| {
                owned_blocks.iter().any(|block| {
                    block.instructions.iter().any(|instruction| {
                        matches!(instruction.kind, RegionInstructionKind::NativeCall(_))
                    })
                })
            })
            .map(|_| {
                let block = builder.create_block();
                builder.set_cold_block(block);
                builder.append_block_param(block, types::I32);
                builder.append_block_param(block, types::I64);
                builder.append_block_param(block, types::I32);
                builder.append_block_param(block, types::I64);
                for _ in 0..crate::JIT_DEOPT_LOCAL_MASK_WORDS {
                    builder.append_block_param(block, types::I64);
                }
                NativeStreamingCallExit { block }
            });
        let register_types = region_register_types(region);
        let register_live_in = &register_liveness.block_live_in;
        let transition_register_liveness = &register_liveness.transition_live;
        let register_ids = fragment.map_or_else(
            || {
                (0..region.register_count)
                    .map(RegId::new)
                    .collect::<BTreeSet<_>>()
            },
            |fragment| fragment.fragment.registers.clone(),
        );
        let register_variables = register_ids
            .into_iter()
            .map(|register| {
                let type_ = register_types.get(&register).copied().unwrap_or(types::I64);
                let storage = if let Some(frame) = streaming_state_frame {
                    frame_layout
                        .expect("streaming frame layout")
                        .register_offset_if_present(
                            fragment.expect("streaming fragment definition").fragment.id,
                            register,
                        )
                        .map_or(NativeRegisterStorage::Transient { type_ }, |offset| {
                            NativeRegisterStorage::FrameSlot {
                                frame,
                                offset,
                                type_,
                            }
                        })
                } else {
                    NativeRegisterStorage::Variable(builder.declare_var(type_))
                };
                (register, storage)
            })
            .collect::<NativeRegisterMap>();
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
        if let Some(frame) = fragment_frame
            && !inline_fragment_entry
        {
            let status = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                frame,
                frame_layout
                    .expect("fragment frame layout")
                    .pending_status_offset(),
            );
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                frame,
                frame_layout
                    .expect("fragment frame layout")
                    .pending_value_offset(),
            );
            builder.def_var(pending_status, status);
            builder.def_var(pending_value, value);
            if let Some(frame) = streaming_state_frame {
                builder.ins().store(
                    MemFlagsData::new(),
                    status,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .pending_status_offset(),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    value,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .pending_value_offset(),
                );
            }
        } else if let Some(frame) = fragment_frame {
            let layout = frame_layout.expect("inline fragment frame layout");
            builder.ins().store(
                MemFlagsData::new(),
                continue_status,
                frame,
                layout.pending_status_offset(),
            );
            builder.ins().store(
                MemFlagsData::new(),
                empty_value,
                frame,
                layout.pending_value_offset(),
            );
        }
        let uninitialized_value = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        for (local, storage) in &locals {
            if let NativeLocalStorage::Variable(variable) = *storage {
                let initial = if region.compile_metadata.tier == NativeCompilerTier::Optimizing
                    && matches!(
                        value_flow.local_storage(*local),
                        crate::region_ir::LocalStorageClass::RequestGlobal
                            | crate::region_ir::LocalStorageClass::Superglobal
                    ) {
                    lower_trusted_request_local_reference(
                        &mut builder,
                        deopt_out,
                        region.function,
                        *local,
                    )
                } else {
                    uninitialized_value
                };
                builder.def_var(variable, initial);
            }
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
                define_local_variable(&mut builder, &locals, *param, value)?;
            }
        } else if inline_fragment_entry {
            let frame = fragment_frame.expect("inline fragment frame");
            let layout = frame_layout.expect("inline fragment frame layout");
            for local in layout.local_slots.keys().copied() {
                builder.ins().store(
                    MemFlagsData::new(),
                    uninitialized_value,
                    frame,
                    layout.local_offset(local)?,
                );
            }
            for (index, local) in region.parameter_locals.iter().enumerate() {
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
                builder.ins().store(
                    MemFlagsData::new(),
                    value,
                    frame,
                    layout.local_offset(*local)?,
                );
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
        let transition_resume_loaders = owned_blocks
            .iter()
            .flat_map(|block| {
                block
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        instruction_has_native_resume_entry(
                            instruction,
                            region.compile_metadata.tier,
                        )
                    })
                    .map(|instruction| instruction.continuation_id)
                    .chain(
                        block_terminator_has_native_transition(block, region.compile_metadata.tier)
                            .then_some(block.terminator_continuation_id),
                    )
            })
            .filter(|continuation| {
                transition_register_liveness
                    .get(continuation)
                    .is_some_and(|registers| registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
            })
            .map(|continuation| (continuation, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let optimizing_block_resume_loaders = (region.compile_metadata.tier
            == NativeCompilerTier::Optimizing)
            .then(|| {
                owned_blocks
                    .iter()
                    .map(|block| (block.id, builder.create_block()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
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
            || !suspension_blocks.is_empty()
            || !transition_resume_loaders.is_empty()
            || !optimizing_block_resume_loaders.is_empty()
            || !osr_resume_loaders.is_empty();
        let resume_default = has_resume_entries.then(|| builder.create_block());
        let mut resume_switch = Switch::new();
        let streaming_resume_restore =
            (has_resume_entries && streaming_state_frame.is_some()).then(|| builder.create_block());
        for (target, loader) in &handler_resume_loaders {
            let resume = u128::from(crate::native_handler_resume_id(*target) as u32);
            resume_switch.set_entry(resume, *loader);
        }
        for (continuation, resume_block) in &suspension_blocks {
            let resume = u128::from(crate::native_suspension_resume_id(*continuation) as u32);
            resume_switch.set_entry(resume, *resume_block);
        }
        for (continuation, loader) in &transition_resume_loaders {
            let resume = u128::from(crate::native_transition_resume_id(*continuation) as u32);
            resume_switch.set_entry(resume, *loader);
        }
        for (block, loader) in &optimizing_block_resume_loaders {
            let continuation = region_block_entry_continuation(&region.blocks[block.index()]);
            let resume =
                u128::from(crate::native_optimizing_continuation_resume_id(continuation) as u32);
            resume_switch.set_entry(resume, *loader);
        }
        for (id, loader) in &osr_resume_loaders {
            let resume = u128::from(*id);
            resume_switch.set_entry(resume, *loader);
        }
        let resume_dispatch = if let Some(resume_default) = resume_default {
            let dispatch = builder.create_block();
            builder.set_cold_block(dispatch);
            if let Some(restore) = streaming_resume_restore {
                let is_normal_entry = builder.ins().icmp_imm(IntCC::Equal, resume_id, -1);
                builder
                    .ins()
                    .brif(is_normal_entry, resume_default, &[], restore, &[]);
                builder.switch_to_block(restore);
                builder.set_cold_block(restore);
                let local_restore_done = builder.create_block();
                builder.set_cold_block(local_restore_done);
                emit_streaming_local_restore_loop(
                    &mut builder,
                    pointer_type,
                    resume_state,
                    streaming_state_frame.expect("streaming resume frame"),
                    region.local_count,
                    local_restore_done,
                );
                builder.switch_to_block(local_restore_done);
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
                let frame = streaming_state_frame.expect("streaming resume frame");
                let layout = frame_layout.expect("streaming resume frame layout");
                builder.ins().store(
                    MemFlagsData::new(),
                    control_status,
                    frame,
                    layout.pending_status_offset(),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    control_value,
                    frame,
                    layout.pending_value_offset(),
                );
                builder.ins().jump(dispatch, &[]);
            } else {
                builder.ins().jump(dispatch, &[]);
            }
            Some(dispatch)
        } else {
            None
        };

        for target in handler_resume_blocks {
            let loader = handler_resume_loaders[&target];
            builder.switch_to_block(loader);
            builder.set_cold_block(loader);
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
            if streaming_state_frame.is_none() {
                restore_native_local_state_values(
                    &mut builder,
                    resume_state,
                    &locals,
                    &resume_locals.into_iter().collect::<Vec<_>>(),
                )?;
            }
            // A call-originated throw reaches a handler through the published
            // control value, not through a pre-existing caller local slot.
            // Install that authoritative native throwable directly into the
            // catch local after restoring the caller frame. Restoring the
            // uninitialized snapshot slot here previously replaced every
            // caught Error with NULL.
            if let Some(exception_locals) = handler_exception_locals.get(&target) {
                if matches!(tier_operations, NativeTierOperations::Optimizing { .. }) {
                    // Catch binding can overwrite an object whose destructor
                    // re-enters PHP. Exception paths are cold, so hand the
                    // complete pre-bind frame and pending throwable to the
                    // exact baseline handler entry once instead of embedding
                    // a partial release/store sequence in optimizing code.
                    let transition_locals = target_block
                        .entry_live_locals
                        .iter()
                        .copied()
                        .chain(exception_locals.iter().copied())
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>();
                    publish_native_continuation_state(
                        &mut builder,
                        deopt_out,
                        region.function,
                        region.local_count,
                        // Catch-bind transitions resume by exact handler block,
                        // including an empty handler with no instruction
                        // continuation of its own.
                        target.raw(),
                        &transition_locals,
                        &locals,
                        native_version,
                    )?;
                    builder.ins().store(
                        MemFlagsData::new(),
                        status,
                        deopt_out,
                        std::mem::offset_of!(crate::JitDeoptState, control_status) as i32,
                    );
                    let detail = builder.ins().iconst(
                        types::I32,
                        i64::from(crate::JIT_NATIVE_CATCH_BIND_TRANSITION_DETAIL),
                    );
                    builder.ins().store(
                        MemFlagsData::new(),
                        detail,
                        deopt_out,
                        std::mem::offset_of!(crate::JitDeoptState, control_reserved) as i32,
                    );
                    builder.ins().store(
                        MemFlagsData::new(),
                        value,
                        deopt_out,
                        std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
                    );
                    let empty = builder
                        .ins()
                        .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
                    builder
                        .ins()
                        .store(MemFlagsData::new(), empty, result_out, 0);
                    let transition = builder.ins().iconst(
                        types::I32,
                        i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
                    );
                    builder.ins().return_(&[transition]);
                    let unreachable = builder.create_block();
                    builder.switch_to_block(unreachable);
                    builder.seal_block(unreachable);
                } else {
                    for local in exception_locals {
                        let current = use_local_variable(&mut builder, &locals, *local)?;
                        let function = builder
                            .ins()
                            .iconst(types::I64, i64::from(region.function.raw()));
                        let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
                        let stored = lower_native_value_operation(
                            module,
                            &mut builder,
                            native_operations.local_store,
                            crate::JIT_LOCAL_STORE_MOVE_INPUT,
                            &[current, value, function, local_value],
                            result_out,
                        )?;
                        define_local_variable(&mut builder, &locals, *local, stored)?;
                    }
                }
            }
            builder.ins().jump(cranelift_block(&blocks, target)?, &[]);
        }
        for region_block in &owned_blocks {
            for instruction in &region_block.instructions {
                if let Some(live_registers) = transition_register_liveness
                    .get(&instruction.continuation_id)
                    .filter(|_| {
                        instruction_has_native_resume_entry(
                            instruction,
                            region.compile_metadata.tier,
                        )
                    })
                    .filter(|registers| registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
                {
                    let loader = transition_resume_loaders[&instruction.continuation_id];
                    builder.switch_to_block(loader);
                    builder.set_cold_block(loader);
                    if let Some(frame) = streaming_state_frame {
                        let fragment = fragment.expect("streaming transition fragment");
                        let layout = frame_layout.expect("streaming transition frame layout");
                        for (snapshot_slot, register) in live_registers.iter().enumerate() {
                            let source_offset =
                                std::mem::offset_of!(crate::JitDeoptState, registers)
                                    .saturating_add(snapshot_slot.saturating_mul(8));
                            let value = builder.ins().load(
                                types::I64,
                                MemFlagsData::new(),
                                resume_state,
                                source_offset as i32,
                            );
                            builder.ins().store(
                                MemFlagsData::new(),
                                value,
                                frame,
                                layout.register_offset(fragment.fragment.id, *register)?,
                            );
                        }
                        builder
                            .ins()
                            .jump(transition_blocks[&instruction.continuation_id], &[]);
                        continue;
                    }
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
                    restore_native_local_state_values(
                        &mut builder,
                        resume_state,
                        &locals,
                        &instruction.live_locals,
                    )?;
                    let mut restored_registers = register_variables.clone();
                    for (snapshot_slot, register) in live_registers.iter().enumerate() {
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
                        define_region_register(
                            &mut builder,
                            &register_variables,
                            &mut restored_registers,
                            *register,
                            value,
                        )?;
                    }
                    builder
                        .ins()
                        .jump(transition_blocks[&instruction.continuation_id], &[]);
                }
            }
        }
        for region_block in &owned_blocks {
            let continuation = region_block.terminator_continuation_id;
            let Some(live_registers) = transition_register_liveness
                .get(&continuation)
                .filter(|_| {
                    block_terminator_has_native_transition(
                        region_block,
                        region.compile_metadata.tier,
                    )
                })
                .filter(|registers| registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
            else {
                continue;
            };
            let loader = transition_resume_loaders[&continuation];
            builder.switch_to_block(loader);
            builder.set_cold_block(loader);
            if let Some(frame) = streaming_state_frame {
                let fragment = fragment.expect("streaming terminator transition fragment");
                let layout = frame_layout.expect("streaming terminator transition frame layout");
                for (snapshot_slot, register) in live_registers.iter().enumerate() {
                    let source_offset = std::mem::offset_of!(crate::JitDeoptState, registers)
                        .saturating_add(snapshot_slot.saturating_mul(8));
                    let value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        resume_state,
                        source_offset as i32,
                    );
                    builder.ins().store(
                        MemFlagsData::new(),
                        value,
                        frame,
                        layout.register_offset(fragment.fragment.id, *register)?,
                    );
                }
            } else {
                restore_native_local_state_values(
                    &mut builder,
                    resume_state,
                    &locals,
                    &region_block.terminator_live_locals,
                )?;
                let mut restored_registers = register_variables.clone();
                for (snapshot_slot, register) in live_registers.iter().enumerate() {
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
                    define_region_register(
                        &mut builder,
                        &register_variables,
                        &mut restored_registers,
                        *register,
                        value,
                    )?;
                }
            }
            builder.ins().jump(transition_blocks[&continuation], &[]);
        }
        for (block_id, loader) in &optimizing_block_resume_loaders {
            let target = region.blocks.get(block_id.index()).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_OPTIMIZING_REENTRY_BLOCK",
                    format!("optimizing re-entry block {} is missing", block_id.raw()),
                )
            })?;
            builder.switch_to_block(*loader);
            builder.set_cold_block(*loader);
            restore_native_local_state_values(
                &mut builder,
                resume_state,
                &locals,
                &target.entry_state_locals,
            )?;
            let mut restored_registers = register_variables.clone();
            for (snapshot_slot, register) in register_live_in
                .get(block_id)
                .into_iter()
                .flatten()
                .enumerate()
            {
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
                define_region_register(
                    &mut builder,
                    &register_variables,
                    &mut restored_registers,
                    *register,
                    value,
                )?;
            }
            builder
                .ins()
                .jump(cranelift_block(&blocks, *block_id)?, &[]);
        }
        for osr_entry in &osr_entries {
            let loader = osr_resume_loaders[&osr_entry.id];
            builder.switch_to_block(loader);
            builder.set_cold_block(loader);
            if streaming_state_frame.is_none() {
                restore_native_local_state_values(
                    &mut builder,
                    resume_state,
                    &locals,
                    &osr_entry.live_locals,
                )?;
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
                if streaming_state_frame.is_none() {
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
                            frame_layout
                                .expect("streaming frame layout")
                                .local_offset(local)?,
                        );
                        define_local_variable(&mut builder, &locals, local, value)?;
                    }
                }
                builder.ins().jump(cranelift_block(&blocks, *entry)?, &[]);
            }
            builder.switch_to_block(invalid_entry);
            builder.set_cold_block(invalid_entry);
            let invalid_entry_marker = builder.ins().iconst(types::I32, 0x4652_4147);
            builder.ins().store(
                MemFlagsData::new(),
                invalid_entry_marker,
                deopt_out,
                std::mem::offset_of!(crate::JitDeoptState, control_reserved) as i32,
            );
            let invalid_entry_value = builder.ins().sextend(types::I64, entry_id);
            builder.ins().store(
                MemFlagsData::new(),
                invalid_entry_value,
                deopt_out,
                std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
            );
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
            if let Some(frame) = streaming_state_frame {
                for register in register_live_in.get(&region_block.id).into_iter().flatten() {
                    let type_ = register_types.get(register).copied().unwrap_or(types::I64);
                    let value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        frame,
                        frame_layout
                            .expect("streaming frame layout")
                            .register_offset(
                                fragment.expect("streaming fragment definition").fragment.id,
                                *register,
                            )?,
                    );
                    let value = if type_ == types::I64 {
                        value
                    } else {
                        builder.ins().ireduce(type_, value)
                    };
                    // One load per real block live-in is cheaper than
                    // reloading the same slot at every operand use. The frame
                    // remains authoritative; this cache is discarded at the
                    // next real CFG boundary.
                    registers.insert(*register, NativeRegisterStorage::Cached(value));
                }
            }
            if loop_headers.contains(&region_block.id)
                && let Some(helper) = native_operations.execution_poll
            {
                let count_visits = builder.create_block();
                let poll = builder.create_block();
                let continue_execution = builder.create_block();
                let runtime_view_offset =
                    std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
                let counter_address = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    deopt_out,
                    runtime_view_offset
                        + std::mem::offset_of!(crate::JitNativeRuntimeView, poll_counter) as i32,
                );
                let pointer_type = module.target_config().pointer_type();
                let counter_address = if pointer_type == types::I64 {
                    counter_address
                } else {
                    builder.ins().ireduce(pointer_type, counter_address)
                };
                let counter_available = builder.ins().icmp_imm(IntCC::NotEqual, counter_address, 0);
                builder
                    .ins()
                    .brif(counter_available, count_visits, &[], poll, &[]);

                builder.switch_to_block(count_visits);
                let counter =
                    builder
                        .ins()
                        .load(types::I32, MemFlagsData::new(), counter_address, 0);
                let counter = builder.ins().iadd_imm(counter, 1);
                let counter = builder.ins().band_imm(counter, 4095);
                builder
                    .ins()
                    .store(MemFlagsData::new(), counter, counter_address, 0);
                let deadline_check = builder.ins().icmp_imm(IntCC::Equal, counter, 0);
                builder
                    .ins()
                    .brif(deadline_check, poll, &[], continue_execution, &[]);

                builder.switch_to_block(poll);
                let call = call_native_helper(module, &mut builder, helper, &[]);
                let status = builder.inst_results(call)[0];
                require_native_operation_ok(&mut builder, status, helper.terminal_exit()?)?;
                builder.ins().jump(continue_execution, &[]);
                builder.switch_to_block(continue_execution);
            }
            let mut terminated = false;
            for instruction in &region_block.instructions {
                let transition_block = transition_blocks.get(&instruction.continuation_id).copied();
                if let Some(transition_block) = transition_block {
                    builder.ins().jump(transition_block, &[]);
                    builder.switch_to_block(transition_block);
                    // A resume loader may enter this instruction without
                    // executing earlier instructions in the Region block.
                    // The compact frame is authoritative at that boundary;
                    // block-local cached SSA values would not dominate the
                    // resume edge.
                    if streaming_state_frame.is_some() {
                        registers = register_variables.clone();
                    }
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
                        &[
                            runtime,
                            arguments,
                            result_out,
                            deopt_out,
                            resume_id,
                            resume_state,
                        ],
                    );
                    terminated = true;
                    break;
                }
                let transition_live_registers = transition_register_liveness
                    .get(&instruction.continuation_id)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                match tier_operations {
                    NativeTierOperations::Optimizing { operations } => {
                        lower_optimizing_region_instruction(
                            module,
                            &mut builder,
                            &register_variables,
                            &suspension_blocks,
                            &locals,
                            &mut registers,
                            instruction,
                            transition_live_registers,
                            constants,
                            &value_flow,
                            inline_constants,
                            function_params,
                            runtime,
                            result_out,
                            deopt_out,
                            resume_state,
                            pending_status,
                            pending_value,
                            region.function,
                            region.local_count,
                            native_version,
                            unit_identity,
                            operations.with_runtime(runtime),
                        )
                        .map(|emitted| {
                            production_lowering.push(crate::JitProductionLoweringMetadata {
                                function: region.function,
                                continuation_id: instruction.continuation_id,
                                operation: crate::region_ir::baseline_instruction_lowering(
                                    &instruction.source_kind,
                                )
                                .variant
                                .to_owned(),
                                class: emitted.class,
                                operation_local_transition: emitted.operation_local_transition,
                            });
                        })
                    }
                    NativeTierOperations::Baseline { .. } => lower_baseline_region_instruction(
                        module,
                        &mut builder,
                        functions,
                        inline_constants,
                        function_params,
                        external_function_signatures,
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
                        transition_live_registers,
                        constants,
                        &value_flow,
                        streaming_call_exit,
                        result_out,
                        deopt_out,
                        resume_state,
                        pending_status,
                        pending_value,
                        region.function,
                        region.local_count,
                        native_version,
                        region.flags.is_top_level,
                        &region.locals,
                        unit_identity,
                        pointer_type,
                    ),
                }
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        error.code,
                        format!(
                            "{} in Region block {} continuation {} ({:?})",
                            error.detail,
                            region_block.id.raw(),
                            instruction.continuation_id,
                            instruction.source_kind,
                        ),
                    )
                })?;
                maximum_temporary_cache_entries = maximum_temporary_cache_entries.max(
                    registers
                        .values()
                        .filter(|storage| matches!(storage, NativeRegisterStorage::Cached(_)))
                        .count(),
                );
                if matches!(instruction.kind, RegionInstructionKind::RuntimeFatal { .. }) {
                    terminated = true;
                    break;
                }
            }
            if terminated {
                continue;
            }
            if let Some(transition_block) =
                transition_blocks.get(&region_block.terminator_continuation_id)
            {
                builder.ins().jump(*transition_block, &[]);
                builder.switch_to_block(*transition_block);
                // The normal edge and a resume loader both enter this block.
                // Values cached while lowering the normal predecessor do not
                // dominate the resume edge; the compact frame does.
                if streaming_state_frame.is_some() {
                    registers = register_variables.clone();
                }
            }
            builder.set_srcloc(ir::SourceLoc::new(
                region_block.terminator_continuation_id.saturating_add(1),
            ));
            // Streaming definitions store through to every externally live
            // frame slot immediately. Re-emitting all successor live-ins here
            // duplicated stores on every CFG edge and inflated both baseline
            // code and execution traffic; successor blocks already reload the
            // authoritative slots above.
            match tier_operations {
                NativeTierOperations::Optimizing { operations } => {
                    let value_release_validate = module
                        .declare_func_in_func(operations.value_release_validate, builder.func);
                    let value_release_commit =
                        module.declare_func_in_func(operations.value_release_commit, builder.func);
                    lower_optimizing_region_terminator(
                        &mut builder,
                        &blocks,
                        &locals,
                        &registers,
                        result_out,
                        deopt_out,
                        region.function,
                        region.local_count,
                        region_block.terminator_continuation_id,
                        &region_block.terminator_live_locals,
                        transition_register_liveness
                            .get(&region_block.terminator_continuation_id)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                        native_version,
                        value_release_validate,
                        value_release_commit,
                        region.return_type.as_ref(),
                        &region_block.terminator,
                        constants,
                        &value_flow,
                    )
                    .map(|emitted| {
                        production_lowering.push(crate::JitProductionLoweringMetadata {
                            function: region.function,
                            continuation_id: region_block.terminator_continuation_id,
                            operation: crate::region_ir::baseline_terminator_lowering(
                                &region_block.source_terminator,
                            )
                            .variant
                            .to_owned(),
                            class: emitted.class,
                            operation_local_transition: emitted.operation_local_transition,
                        });
                    })
                }
                NativeTierOperations::Baseline { .. } => lower_region_terminator(
                    &mut builder,
                    &terminator_blocks,
                    &locals,
                    &registers,
                    result_out,
                    deopt_out,
                    pending_status,
                    pending_value,
                    module,
                    native_operations,
                    region.function,
                    region.return_type.is_some(),
                    &region_block.terminator,
                    constants,
                    &value_flow,
                ),
            }?;
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
                if streaming_state_frame.is_none() {
                    for local in &target_block.entry_state_locals {
                        let value = use_local_variable(&mut builder, &locals, *local)?;
                        builder.ins().store(
                            MemFlagsData::new(),
                            value,
                            frame,
                            frame_layout
                                .expect("fragment frame layout")
                                .local_offset(*local)?,
                        );
                    }
                }
                if streaming_state_frame.is_none() {
                    for register in register_live_in.get(target).into_iter().flatten() {
                        let value =
                            use_region_register(&mut builder, &register_variables, *register)?;
                        let value = if builder.func.dfg.value_type(value) == types::I64 {
                            value
                        } else {
                            builder.ins().uextend(types::I64, value)
                        };
                        builder.ins().store(
                            MemFlagsData::new(),
                            value,
                            frame,
                            frame_layout
                                .expect("fragment frame layout")
                                .register_offset(fragment.fragment.id, *register)?,
                        );
                    }
                }
                let status = builder.use_var(pending_status);
                let value = builder.use_var(pending_value);
                builder.ins().store(
                    MemFlagsData::new(),
                    status,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .pending_status_offset(),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    value,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .pending_value_offset(),
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
                    frame_layout
                        .expect("fragment frame layout")
                        .entry_id_offset(),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    no_resume,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .resume_id_offset(),
                );
                builder.ins().return_call(callee, &[runtime, frame]);
            }
        }
        if let (Some(streaming_call_exit), Some(frame)) =
            (streaming_call_exit, streaming_state_frame)
        {
            builder.switch_to_block(streaming_call_exit.block);
            let params = builder.block_params(streaming_call_exit.block).to_vec();
            let status = params[0];
            let value = params[1];
            let continuation = params[2];
            let suspension_link = params[3];
            let store_i32 = |builder: &mut FunctionBuilder<'_>, offset: usize, value: ir::Value| {
                builder
                    .ins()
                    .store(MemFlagsData::new(), value, deopt_out, offset as i32);
            };
            let function_id = builder
                .ins()
                .iconst(types::I32, i64::from(region.function.raw()));
            let slot_count = builder
                .ins()
                .iconst(types::I32, i64::from(region.local_count));
            let native_version_value = builder.ins().iconst(types::I32, i64::from(native_version));
            store_i32(
                &mut builder,
                std::mem::offset_of!(crate::JitDeoptState, function_id),
                function_id,
            );
            store_i32(
                &mut builder,
                std::mem::offset_of!(crate::JitDeoptState, continuation_id),
                continuation,
            );
            store_i32(
                &mut builder,
                std::mem::offset_of!(crate::JitDeoptState, slot_count),
                slot_count,
            );
            store_i32(
                &mut builder,
                std::mem::offset_of!(crate::JitDeoptState, native_version),
                native_version_value,
            );
            for (word, mask) in params[4..].iter().copied().enumerate() {
                builder.ins().store(
                    MemFlagsData::new(),
                    mask,
                    deopt_out,
                    std::mem::offset_of!(crate::JitDeoptState, initialized_mask)
                        .saturating_add(word.saturating_mul(8)) as i32,
                );
            }
            builder
                .ins()
                .store(MemFlagsData::new(), value, result_out, 0);
            publish_native_fiber_suspension_link(&mut builder, deopt_out, suspension_link);
            let finished = builder.create_block();
            builder.set_cold_block(finished);
            emit_streaming_local_snapshot_loop(
                &mut builder,
                pointer_type,
                deopt_out,
                frame,
                region.local_count,
                finished,
            );
            builder.switch_to_block(finished);
            builder.ins().return_(&[status]);
        }
        if let (Some(dispatch), Some(resume_default)) = (resume_dispatch, resume_default) {
            builder.switch_to_block(dispatch);
            resume_switch.emit(&mut builder, resume_id, resume_default);
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
    let mut pre_regalloc = match validate_pre_regalloc_structure(
        &ctx.func,
        region,
        fragment.map(|fragment| fragment.fragment.id),
    ) {
        Ok(metrics) => metrics,
        Err(error) => {
            module.clear_context(ctx);
            return Err(error);
        }
    };
    let source_instructions = fragment.map_or_else(
        || {
            region
                .blocks
                .iter()
                .map(|block| block.instructions.len())
                .sum()
        },
        |fragment| {
            fragment
                .fragment
                .blocks
                .iter()
                .map(|block| region.blocks[block.index()].instructions.len())
                .sum::<usize>()
        },
    );
    if source_instructions != 0 {
        pre_regalloc.loads_per_source_instruction_milli = pre_regalloc
            .loads
            .saturating_mul(1_000)
            .div_ceil(source_instructions);
        pre_regalloc.stores_per_source_instruction_milli = pre_regalloc
            .stores
            .saturating_mul(1_000)
            .div_ceil(source_instructions);
    }
    let verifier_flags = settings::Flags::new(settings::builder());
    if let Err(error) = verify_function(&ctx.func, &verifier_flags) {
        let error = CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("Cranelift verifier rejected executable Region IR: {error}"),
        );
        module.clear_context(ctx);
        return Err(error);
    }
    let clif_blocks = ctx.func.layout.blocks().count();
    if preflight_only {
        let lowered_function = std::mem::replace(&mut ctx.func, ir::Function::new());
        module.clear_context(ctx);
        return Ok(DefinedRegionFunction {
            lowered_function: Some(lowered_function),
            code: Vec::new(),
            clif_blocks,
            alignment: 1,
            relocations: Vec::new(),
            native_pc_ranges: Vec::new(),
            native_stack_bytes: 0,
            pre_regalloc,
            maximum_temporary_cache_entries,
            production_lowering,
        });
    }
    if let Err(error) = module.define_function(func_id, ctx) {
        let error = CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define native function: {error}"),
        );
        module.clear_context(ctx);
        return Err(error);
    }
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
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations,
        native_pc_ranges,
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries,
        production_lowering,
    })
}

fn lower_direct_array_child_entry_body(builder: &mut FunctionBuilder<'_>) {
    let entry = builder.create_block();
    let inspect = builder.create_block();
    let search = builder.create_block();
    let compare = builder.create_block();
    let next = builder.create_block();
    let found = builder.create_block();
    let failed = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.append_block_param(search, types::I64);
    builder.append_block_param(next, types::I64);
    let pointer_type = builder.func.dfg.value_type(builder.block_params(entry)[0]);
    builder.append_block_param(found, pointer_type);

    builder.switch_to_block(entry);
    let deopt_out = builder.block_params(entry)[0];
    let array = builder.block_params(entry)[1];
    let key = builder.block_params(entry)[2];
    let is_array = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    let encoded_index = builder.ins().ireduce(types::I32, array);
    let is_direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        encoded_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(is_array, is_direct_index);
    builder.ins().brif(direct, inspect, &[], failed, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, array, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let direct_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let key_runtime = lower_is_runtime_handle(builder, key);
    let key_constant = lower_value_has_namespace_tag(builder, key, crate::JIT_VALUE_CONSTANT_TAG);
    let namespaced = builder.ins().bor(key_runtime, key_constant);
    let key_immediate = builder.ins().icmp_imm(IntCC::Equal, namespaced, 0);
    let key_string = lower_value_has_tag(builder, key, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    let supported_key = builder.ins().bor(key_immediate, key_string);
    let supported_key = builder.ins().bor(supported_key, key_constant);
    let admitted = builder.ins().band(direct_kind, supported_key);
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let zero = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .brif(admitted, search, &[zero.into()], failed, &[]);

    builder.switch_to_block(search);
    let index = builder.block_params(search)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(exhausted, failed, &[], compare, &[]);

    builder.switch_to_block(compare);
    let pointer_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let candidate_entry = builder.ins().iadd(entries, offset);
    let candidate = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), candidate_entry, 0);
    let matches = lower_native_array_key_equal(builder, candidate, key, deopt_out);
    builder.ins().brif(
        matches,
        found,
        &[candidate_entry.into()],
        next,
        &[index.into()],
    );

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(search, &[index.into()]);

    builder.switch_to_block(found);
    let candidate_entry = builder.block_params(found)[0];
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        candidate_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    builder.ins().return_(&[value, candidate_entry]);

    builder.switch_to_block(failed);
    let zero_value = builder.ins().iconst(types::I64, 0);
    let null_entry = builder.ins().iconst(pointer_type, 0);
    builder.ins().return_(&[zero_value, null_entry]);
}

fn lower_direct_value_release_validate_body(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    func_id: FuncId,
) {
    let entry = builder.create_block();
    let inspect = builder.create_block();
    let inspect_last = builder.create_block();
    let validate_reference = builder.create_block();
    let inspect_composite = builder.create_block();
    let inspect_foreach = builder.create_block();
    let validate_foreach = builder.create_block();
    let scan = builder.create_block();
    let validate_key = builder.create_block();
    let validate_value = builder.create_block();
    let next = builder.create_block();
    let accepted = builder.create_block();
    let rejected = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.append_block_param(inspect_composite, types::I8);
    builder.append_block_param(inspect_foreach, types::I8);
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(validate_value, types::I64);
    builder.append_block_param(next, types::I64);

    let recurse = module.declare_func_in_func(func_id, builder.func);
    builder.switch_to_block(entry);
    let deopt_out = builder.block_params(entry)[0];
    let value = builder.block_params(entry)[1];
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let runtime = lower_is_runtime_handle(builder, value);
    builder.ins().brif(runtime, inspect, &[], accepted, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, value, deopt_out);
    let refcount = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    let shared = builder
        .ins()
        .icmp_imm(IntCC::UnsignedGreaterThan, refcount, 1);
    let last = builder.ins().icmp_imm(IntCC::Equal, refcount, 1);
    builder.ins().brif(shared, accepted, &[], inspect_last, &[]);

    builder.switch_to_block(inspect_last);
    let index = builder.ins().ireduce(types::I32, value);
    let direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let direct_reference = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR),
    );
    let direct_string = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_STRING),
    );
    let direct_float = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_FLOAT),
    );
    let scalar = builder.ins().bor(direct_string, direct_float);
    let scalar = builder.ins().band(direct, scalar);
    let valid_last = builder.ins().band(direct, last);
    let reference = builder.ins().band(valid_last, direct_reference);
    let inspect_reference = builder.create_block();
    builder
        .ins()
        .brif(scalar, accepted, &[], inspect_reference, &[]);

    builder.switch_to_block(inspect_reference);
    builder.ins().brif(
        reference,
        validate_reference,
        &[],
        inspect_composite,
        &[valid_last.into()],
    );

    builder.switch_to_block(validate_reference);
    let payload = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let call = builder.ins().call(recurse, &[deopt_out, payload]);
    let valid = builder.inst_results(call)[0];
    builder.ins().brif(valid, accepted, &[], rejected, &[]);

    builder.switch_to_block(inspect_composite);
    let valid_last = builder.block_params(inspect_composite)[0];
    let direct_array = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let direct_array = builder.ins().band(valid_last, direct_array);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().brif(
        direct_array,
        scan,
        &[zero.into()],
        inspect_foreach,
        &[valid_last.into()],
    );

    builder.switch_to_block(inspect_foreach);
    let valid_last = builder.block_params(inspect_foreach)[0];
    let direct_foreach = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH),
    );
    let direct_foreach = builder.ins().band(valid_last, direct_foreach);
    builder
        .ins()
        .brif(direct_foreach, validate_foreach, &[], rejected, &[]);

    builder.switch_to_block(validate_foreach);
    let source = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let call = builder.ins().call(recurse, &[deopt_out, source]);
    let valid = builder.inst_results(call)[0];
    builder.ins().brif(valid, accepted, &[], rejected, &[]);

    builder.switch_to_block(scan);
    let scan_index = builder.block_params(scan)[0];
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let finished = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, scan_index, length);
    builder
        .ins()
        .brif(finished, accepted, &[], validate_key, &[]);

    builder.switch_to_block(validate_key);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let pointer_index = if pointer_type == types::I64 {
        scan_index
    } else {
        builder.ins().ireduce(pointer_type, scan_index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let array_entry = builder.ins().iadd(entries, offset);
    let key = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), array_entry, 0);
    let call = builder.ins().call(recurse, &[deopt_out, key]);
    let valid = builder.inst_results(call)[0];
    builder
        .ins()
        .brif(valid, validate_value, &[scan_index.into()], rejected, &[]);

    builder.switch_to_block(validate_value);
    let scan_index = builder.block_params(validate_value)[0];
    let pointer_index = if pointer_type == types::I64 {
        scan_index
    } else {
        builder.ins().ireduce(pointer_type, scan_index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let array_entry = builder.ins().iadd(entries, offset);
    let child = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        array_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let call = builder.ins().call(recurse, &[deopt_out, child]);
    let valid = builder.inst_results(call)[0];
    builder
        .ins()
        .brif(valid, next, &[scan_index.into()], rejected, &[]);

    builder.switch_to_block(next);
    let scan_index = builder.block_params(next)[0];
    let next_index = builder.ins().iadd_imm(scan_index, 1);
    builder.ins().jump(scan, &[next_index.into()]);

    builder.switch_to_block(accepted);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().return_(&[yes]);
    builder.switch_to_block(rejected);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().return_(&[no]);
}

fn lower_free_direct_array_entries(
    builder: &mut FunctionBuilder<'_>,
    deopt_out: ir::Value,
    entries: ir::Value,
    capacity: ir::Value,
) {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_entries) as i32,
    );
    let free_heads = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_free_heads) as i32,
    );
    let leading = builder.ins().clz(capacity);
    let ceiling = builder.ins().iconst(types::I32, 31);
    let bucket = builder.ins().isub(ceiling, leading);
    let wide_bucket = builder.ins().uextend(pointer_type, bucket);
    let bucket_offset = builder.ins().ishl_imm(wide_bucket, 2);
    let head_ptr = builder.ins().iadd(free_heads, bucket_offset);
    let old_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), head_ptr, 0);
    let byte_offset = builder.ins().isub(entries, arena);
    let entry_index = builder.ins().ushr_imm(byte_offset, 4);
    let entry_index = if pointer_type == types::I32 {
        entry_index
    } else {
        builder.ins().ireduce(types::I32, entry_index)
    };
    builder
        .ins()
        .store(MemFlagsData::new(), old_head, entries, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), entry_index, head_ptr, 0);
}

fn lower_free_direct_string_bytes(
    builder: &mut FunctionBuilder<'_>,
    deopt_out: ir::Value,
    bytes: ir::Value,
    reserved: ir::Value,
) {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_bytes) as i32,
    );
    let free_heads = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_free_heads) as i32,
    );
    let capacity = builder.ins().ushr_imm(
        reserved,
        crate::JIT_NATIVE_DIRECT_STRING_CAPACITY_SHIFT as i64,
    );
    let leading = builder.ins().clz(capacity);
    let ceiling = builder.ins().iconst(types::I32, 31);
    let bucket = builder.ins().isub(ceiling, leading);
    let wide_bucket = builder.ins().uextend(pointer_type, bucket);
    let bucket_offset = builder.ins().ishl_imm(wide_bucket, 2);
    let head_ptr = builder.ins().iadd(free_heads, bucket_offset);
    let old_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), head_ptr, 0);
    let byte_offset = builder.ins().isub(bytes, arena);
    let byte_offset = if pointer_type == types::I32 {
        byte_offset
    } else {
        builder.ins().ireduce(types::I32, byte_offset)
    };
    builder.ins().store(MemFlagsData::new(), old_head, bytes, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), byte_offset, head_ptr, 0);
}

fn lower_direct_value_release_commit_body(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    func_id: FuncId,
) {
    let entry = builder.create_block();
    let inspect = builder.create_block();
    let decrement = builder.create_block();
    let inspect_last = builder.create_block();
    let release_reference = builder.create_block();
    let inspect_composite = builder.create_block();
    let inspect_foreach = builder.create_block();
    let release_foreach = builder.create_block();
    let free_string = builder.create_block();
    let scan = builder.create_block();
    let release_entry = builder.create_block();
    let next = builder.create_block();
    let free_array = builder.create_block();
    let free_slot = builder.create_block();
    let accepted = builder.create_block();
    let rejected = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.append_block_param(inspect_composite, types::I8);
    builder.append_block_param(inspect_foreach, types::I8);
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(next, types::I64);

    let recurse = module.declare_func_in_func(func_id, builder.func);
    builder.switch_to_block(entry);
    let deopt_out = builder.block_params(entry)[0];
    let value = builder.block_params(entry)[1];
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let runtime = lower_is_runtime_handle(builder, value);
    builder.ins().brif(runtime, inspect, &[], accepted, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, value, deopt_out);
    let refcount = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    let shared = builder
        .ins()
        .icmp_imm(IntCC::UnsignedGreaterThan, refcount, 1);
    builder
        .ins()
        .brif(shared, decrement, &[], inspect_last, &[]);

    builder.switch_to_block(decrement);
    let remaining = builder.ins().iadd_imm(refcount, -1);
    builder.ins().store(
        MemFlagsData::new(),
        remaining,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    builder.ins().jump(accepted, &[]);

    builder.switch_to_block(inspect_last);
    let last = builder.ins().icmp_imm(IntCC::Equal, refcount, 1);
    let index = builder.ins().ireduce(types::I32, value);
    let direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let valid_last = builder.ins().band(last, direct);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let direct_reference = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR),
    );
    let direct_string = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_STRING),
    );
    let direct_float = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_FLOAT),
    );
    let string = builder.ins().band(valid_last, direct_string);
    let float = builder.ins().band(valid_last, direct_float);
    let reference = builder.ins().band(valid_last, direct_reference);
    let inspect_reference = builder.create_block();
    builder
        .ins()
        .brif(string, free_string, &[], inspect_reference, &[]);

    builder.switch_to_block(inspect_reference);
    let inspect_float = builder.create_block();
    builder
        .ins()
        .brif(reference, release_reference, &[], inspect_float, &[]);

    builder.switch_to_block(inspect_float);
    builder.ins().brif(
        float,
        free_slot,
        &[],
        inspect_composite,
        &[valid_last.into()],
    );

    builder.switch_to_block(free_string);
    let bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let reserved = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    lower_free_direct_string_bytes(builder, deopt_out, bytes, reserved);
    builder.ins().jump(free_slot, &[]);

    builder.switch_to_block(release_reference);
    let payload = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let _ = builder.ins().call(recurse, &[deopt_out, payload]);
    builder.ins().jump(free_slot, &[]);

    builder.switch_to_block(inspect_composite);
    let valid_last = builder.block_params(inspect_composite)[0];
    let direct_array = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let direct_array = builder.ins().band(valid_last, direct_array);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().brif(
        direct_array,
        scan,
        &[zero.into()],
        inspect_foreach,
        &[valid_last.into()],
    );

    builder.switch_to_block(inspect_foreach);
    let valid_last = builder.block_params(inspect_foreach)[0];
    let direct_foreach = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH),
    );
    let direct_foreach = builder.ins().band(valid_last, direct_foreach);
    builder
        .ins()
        .brif(direct_foreach, release_foreach, &[], rejected, &[]);

    builder.switch_to_block(release_foreach);
    let source = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let _ = builder.ins().call(recurse, &[deopt_out, source]);
    builder.ins().jump(free_slot, &[]);

    builder.switch_to_block(scan);
    let scan_index = builder.block_params(scan)[0];
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let finished = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, scan_index, length);
    builder
        .ins()
        .brif(finished, free_array, &[], release_entry, &[]);

    builder.switch_to_block(release_entry);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let pointer_index = if pointer_type == types::I64 {
        scan_index
    } else {
        builder.ins().ireduce(pointer_type, scan_index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let array_entry = builder.ins().iadd(entries, offset);
    let key = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), array_entry, 0);
    let child = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        array_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let _ = builder.ins().call(recurse, &[deopt_out, key]);
    let _ = builder.ins().call(recurse, &[deopt_out, child]);
    builder.ins().jump(next, &[scan_index.into()]);

    builder.switch_to_block(next);
    let scan_index = builder.block_params(next)[0];
    let next_index = builder.ins().iadd_imm(scan_index, 1);
    builder.ins().jump(scan, &[next_index.into()]);

    builder.switch_to_block(free_array);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let capacity = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    lower_free_direct_array_entries(builder, deopt_out, entries, capacity);
    builder.ins().jump(free_slot, &[]);

    builder.switch_to_block(free_slot);
    lower_free_direct_scalar_slot(builder, value, slot, deopt_out);
    builder.ins().jump(accepted, &[]);

    builder.switch_to_block(accepted);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().return_(&[yes]);
    builder.switch_to_block(rejected);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().return_(&[no]);
}

fn lower_direct_array_ensure_unique_body(builder: &mut FunctionBuilder<'_>) {
    let entry = builder.create_block();
    let inspect = builder.create_block();
    let choose = builder.create_block();
    let grow = builder.create_block();
    let allocate = builder.create_block();
    let reuse = builder.create_block();
    let bump = builder.create_block();
    let range_ready = builder.create_block();
    let clone_slot = builder.create_block();
    let move_slot = builder.create_block();
    let copy = builder.create_block();
    let copy_entry = builder.create_block();
    let retain_entry = builder.create_block();
    let store_entry = builder.create_block();
    let finalize = builder.create_block();
    let finalize_clone = builder.create_block();
    let release_cloned_source = builder.create_block();
    let complete_clone = builder.create_block();
    let finalize_move = builder.create_block();
    let failed = builder.create_block();
    let succeeded = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.append_block_param(choose, types::I8);
    builder.append_block_param(grow, types::I32);
    builder.append_block_param(grow, types::I8);
    builder.append_block_param(allocate, types::I32);
    builder.append_block_param(allocate, types::I8);
    let pointer_type = builder.func.dfg.value_type(builder.block_params(entry)[0]);
    builder.append_block_param(range_ready, pointer_type);
    builder.append_block_param(range_ready, types::I32);
    builder.append_block_param(range_ready, types::I8);
    for block in [copy, finalize] {
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, pointer_type);
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I8);
    }
    builder.append_block_param(store_entry, types::I64);
    builder.append_block_param(store_entry, pointer_type);
    builder.append_block_param(store_entry, types::I64);
    builder.append_block_param(store_entry, types::I8);
    builder.append_block_param(succeeded, types::I64);

    builder.switch_to_block(entry);
    let deopt_out = builder.block_params(entry)[0];
    let array = builder.block_params(entry)[1];
    let additional = builder.block_params(entry)[2];
    let consume_owner = builder.block_params(entry)[3];
    let is_array = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    let encoded_index = builder.ins().ireduce(types::I32, array);
    let is_direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        encoded_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(is_array, is_direct_index);
    builder.ins().brif(direct, inspect, &[], failed, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, array, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let refcount = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let capacity = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let old_entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let required = builder.ins().iadd(length, additional);
    let wrapped = builder
        .ins()
        .icmp(IntCC::UnsignedLessThan, required, length);
    let within_limit = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        required,
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
    );
    let kind_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let live = builder.ins().icmp_imm(IntCC::NotEqual, refcount, 0);
    let valid = builder.ins().band(kind_ok, live);
    let valid = builder.ins().band_not(valid, wrapped);
    let valid = builder.ins().band(valid, within_limit);
    let unique = builder.ins().icmp_imm(IntCC::Equal, refcount, 1);
    let clone = builder.ins().icmp_imm(IntCC::Equal, unique, 0);
    builder
        .ins()
        .brif(valid, choose, &[clone.into()], failed, &[]);

    builder.switch_to_block(choose);
    let clone = builder.block_params(choose)[0];
    let capacity_wide = builder.ins().uextend(types::I64, capacity);
    let enough = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, capacity_wide, required);
    let unique_and_enough = builder.ins().band(unique, enough);
    builder.ins().brif(
        unique_and_enough,
        succeeded,
        &[array.into()],
        grow,
        &[capacity.into(), clone.into()],
    );

    builder.switch_to_block(grow);
    let candidate = builder.block_params(grow)[0];
    let clone = builder.block_params(grow)[1];
    let wide = builder.ins().uextend(types::I64, candidate);
    let enough = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, wide, required);
    let double = builder.create_block();
    builder.ins().brif(
        enough,
        allocate,
        &[candidate.into(), clone.into()],
        double,
        &[],
    );
    builder.switch_to_block(double);
    let at_limit = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        candidate,
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
    );
    let doubled = builder.ins().imul_imm(candidate, 2);
    let minimum = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY),
    );
    let zero_capacity = builder.ins().icmp_imm(IntCC::Equal, candidate, 0);
    let next = builder.ins().select(zero_capacity, minimum, doubled);
    builder
        .ins()
        .brif(at_limit, failed, &[], grow, &[next.into(), clone.into()]);

    builder.switch_to_block(allocate);
    let destination_capacity = builder.block_params(allocate)[0];
    let clone = builder.block_params(allocate)[1];
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_entries) as i32,
    );
    let free_heads = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_free_heads) as i32,
    );
    let leading = builder.ins().clz(destination_capacity);
    let ceiling = builder.ins().iconst(types::I32, 31);
    let bucket = builder.ins().isub(ceiling, leading);
    let wide_bucket = builder.ins().uextend(pointer_type, bucket);
    let bucket_offset = builder.ins().ishl_imm(wide_bucket, 2);
    let free_head_ptr = builder.ins().iadd(free_heads, bucket_offset);
    let free_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), free_head_ptr, 0);
    let has_free = builder.ins().icmp_imm(
        IntCC::NotEqual,
        free_head,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE),
    );
    builder.ins().brif(has_free, reuse, &[], bump, &[]);

    builder.switch_to_block(reuse);
    let wide_free_head = builder.ins().uextend(pointer_type, free_head);
    let free_offset = builder.ins().ishl_imm(wide_free_head, 4);
    let destination = builder.ins().iadd(arena, free_offset);
    let preceding = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), destination, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), preceding, free_head_ptr, 0);
    builder.ins().jump(
        range_ready,
        &[
            destination.into(),
            destination_capacity.into(),
            clone.into(),
        ],
    );

    builder.switch_to_block(bump);
    let next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_next) as i32,
    );
    let next_entry = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), next_ptr, 0);
    let end = builder.ins().iadd(next_entry, destination_capacity);
    let room = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        end,
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
    );
    let wide_next_entry = builder.ins().uextend(pointer_type, next_entry);
    let offset = builder.ins().ishl_imm(wide_next_entry, 4);
    let destination = builder.ins().iadd(arena, offset);
    let bump_ok = builder.create_block();
    builder.ins().brif(room, bump_ok, &[], failed, &[]);
    builder.switch_to_block(bump_ok);
    builder.ins().store(MemFlagsData::new(), end, next_ptr, 0);
    builder.ins().jump(
        range_ready,
        &[
            destination.into(),
            destination_capacity.into(),
            clone.into(),
        ],
    );

    builder.switch_to_block(range_ready);
    let destination_entries = builder.block_params(range_ready)[0];
    let destination_capacity = builder.block_params(range_ready)[1];
    let clone = builder.block_params(range_ready)[2];
    builder.ins().brif(clone, clone_slot, &[], move_slot, &[]);

    builder.switch_to_block(clone_slot);
    let new_index = lower_reserve_direct_value_index(builder, deopt_out, failed);
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let wide_new_index = builder.ins().uextend(pointer_type, new_index);
    let new_slot_offset = builder.ins().ishl_imm(wide_new_index, 5);
    let destination_slot = builder.ins().iadd(slots, new_slot_offset);
    let runtime_index = builder.ins().iadd_imm(
        new_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let runtime_index = builder.ins().uextend(types::I64, runtime_index);
    let destination_handle = builder
        .ins()
        .bor_imm(runtime_index, crate::JIT_VALUE_RUNTIME_ARRAY_TAG as i64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(
        copy,
        &[
            zero.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
    );

    builder.switch_to_block(move_slot);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(
        copy,
        &[zero.into(), slot.into(), array.into(), clone.into()],
    );

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let destination_slot = builder.block_params(copy)[1];
    let destination_handle = builder.block_params(copy)[2];
    let clone = builder.block_params(copy)[3];
    let finished = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(
        finished,
        finalize,
        &[
            index.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
        copy_entry,
        &[],
    );

    builder.switch_to_block(copy_entry);
    let pointer_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let source_entry = builder.ins().iadd(old_entries, offset);
    let destination_entry = builder.ins().iadd(destination_entries, offset);
    let key = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), source_entry, 0);
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        source_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    builder.ins().brif(
        clone,
        retain_entry,
        &[],
        store_entry,
        &[
            index.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
    );
    builder.switch_to_block(retain_entry);
    lower_optimizing_retain(builder, key, deopt_out);
    lower_optimizing_retain(builder, value, deopt_out);
    builder.ins().jump(
        store_entry,
        &[
            index.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
    );
    builder.switch_to_block(store_entry);
    let index = builder.block_params(store_entry)[0];
    let destination_slot = builder.block_params(store_entry)[1];
    let destination_handle = builder.block_params(store_entry)[2];
    let clone = builder.block_params(store_entry)[3];
    builder
        .ins()
        .store(MemFlagsData::new(), key, destination_entry, 0);
    builder.ins().store(
        MemFlagsData::new(),
        value,
        destination_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(
        copy,
        &[
            next.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
    );

    builder.switch_to_block(finalize);
    let destination_slot = builder.block_params(finalize)[1];
    let destination_handle = builder.block_params(finalize)[2];
    let clone = builder.block_params(finalize)[3];
    builder
        .ins()
        .brif(clone, finalize_clone, &[], finalize_move, &[]);
    builder.switch_to_block(finalize_clone);
    let one = builder.ins().iconst(types::I32, 1);
    let array_kind = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    for (value, offset) in [
        (
            one,
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount),
        ),
        (
            array_kind,
            std::mem::offset_of!(crate::JitNativeValueSlot, kind),
        ),
        (
            flags,
            std::mem::offset_of!(crate::JitNativeValueSlot, flags),
        ),
        (
            destination_capacity,
            std::mem::offset_of!(crate::JitNativeValueSlot, reserved),
        ),
    ] {
        builder
            .ins()
            .store(MemFlagsData::new(), value, destination_slot, offset as i32);
    }
    builder.ins().store(
        MemFlagsData::new(),
        length,
        destination_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        destination_entries,
        destination_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    builder.ins().brif(
        consume_owner,
        release_cloned_source,
        &[],
        complete_clone,
        &[],
    );

    builder.switch_to_block(release_cloned_source);
    let remaining = builder.ins().iadd_imm(refcount, -1);
    builder.ins().store(MemFlagsData::new(), remaining, slot, 0);
    builder.ins().jump(complete_clone, &[]);

    builder.switch_to_block(complete_clone);
    builder.ins().jump(succeeded, &[destination_handle.into()]);

    builder.switch_to_block(finalize_move);
    let old_leading = builder.ins().clz(capacity);
    let old_ceiling = builder.ins().iconst(types::I32, 31);
    let old_bucket = builder.ins().isub(old_ceiling, old_leading);
    let wide_old_bucket = builder.ins().uextend(pointer_type, old_bucket);
    let old_bucket_offset = builder.ins().ishl_imm(wide_old_bucket, 2);
    let old_head_ptr = builder.ins().iadd(free_heads, old_bucket_offset);
    let old_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), old_head_ptr, 0);
    let old_byte_offset = builder.ins().isub(old_entries, arena);
    let old_entry_index = builder.ins().ushr_imm(old_byte_offset, 4);
    let old_index = if pointer_type == types::I32 {
        old_entry_index
    } else {
        builder.ins().ireduce(types::I32, old_entry_index)
    };
    builder
        .ins()
        .store(MemFlagsData::new(), old_head, old_entries, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), old_index, old_head_ptr, 0);
    builder.ins().store(
        MemFlagsData::new(),
        destination_capacity,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        destination_entries,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    builder.ins().jump(succeeded, &[array.into()]);

    builder.switch_to_block(failed);
    let status = builder.ins().iconst(types::I32, 1);
    builder.ins().return_(&[status, array]);
    builder.switch_to_block(succeeded);
    let result = builder.block_params(succeeded)[0];
    let status = builder.ins().iconst(types::I32, 0);
    builder.ins().return_(&[status, result]);
}

fn define_direct_array_child_entry_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    func_id: FuncId,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    ctx.func.signature = direct_array_child_entry_signature(module);
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        lower_direct_array_child_entry_body(&mut builder);
        builder.seal_all_blocks();
        builder.finalize();
    }
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("direct array child-entry verifier failure: {error}"),
        )
    })?;
    let clif_blocks = ctx.func.layout.blocks().count();
    let pre_regalloc = PreRegallocMetrics {
        blocks: clif_blocks,
        values: ctx.func.dfg.num_values(),
        instructions: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.layout.block_insts(block).count())
            .sum(),
        block_parameters: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.dfg.block_params(block).len())
            .sum(),
        ..PreRegallocMetrics::default()
    };
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define direct array child-entry function: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            "Cranelift returned no direct array child-entry code",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |layout| layout.frame_to_fp_offset);
    let code = compiled.code_buffer().to_vec();
    let alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations: Vec::new(),
        native_pc_ranges: Vec::new(),
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries: 0,
        production_lowering: Vec::new(),
    })
}

fn define_direct_value_release_validate_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    func_id: FuncId,
    symbol: FunctionId,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    define_direct_value_release_function(module, ctx, builder_context, func_id, symbol, false)
}

fn define_direct_value_release_commit_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    func_id: FuncId,
    symbol: FunctionId,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    define_direct_value_release_function(module, ctx, builder_context, func_id, symbol, true)
}

fn define_direct_value_release_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    func_id: FuncId,
    symbol: FunctionId,
    commit: bool,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    ctx.func.signature = direct_value_release_signature(module);
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        if commit {
            lower_direct_value_release_commit_body(module, &mut builder, func_id);
        } else {
            lower_direct_value_release_validate_body(module, &mut builder, func_id);
        }
        builder.seal_all_blocks();
        builder.finalize();
    }
    let phase = if commit { "commit" } else { "validator" };
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("direct value-release {phase} verifier failure: {error}"),
        )
    })?;
    let clif_blocks = ctx.func.layout.blocks().count();
    let pre_regalloc = PreRegallocMetrics {
        blocks: clif_blocks,
        values: ctx.func.dfg.num_values(),
        instructions: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.layout.block_insts(block).count())
            .sum(),
        block_parameters: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.dfg.block_params(block).len())
            .sum(),
        ..PreRegallocMetrics::default()
    };
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define direct value-release {phase}: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            format!("Cranelift returned no direct value-release {phase} code"),
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |layout| layout.frame_to_fp_offset);
    let code = compiled.code_buffer().to_vec();
    let alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    let relocation_functions = BTreeMap::from([(symbol, func_id)]);
    let relocations = compiled
        .buffer
        .relocs()
        .iter()
        .map(|relocation| {
            capture_relocation(
                module,
                ModuleReloc::from_mach_reloc(relocation, &ctx.func, func_id),
                &relocation_functions,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations,
        native_pc_ranges: Vec::new(),
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries: 0,
        production_lowering: Vec::new(),
    })
}

fn define_direct_array_ensure_unique_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    func_id: FuncId,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    ctx.func.signature = direct_array_ensure_unique_signature(module);
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        lower_direct_array_ensure_unique_body(&mut builder);
        builder.seal_all_blocks();
        builder.finalize();
    }
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("direct array COW verifier failure: {error}"),
        )
    })?;
    let clif_blocks = ctx.func.layout.blocks().count();
    let pre_regalloc = PreRegallocMetrics {
        blocks: clif_blocks,
        values: ctx.func.dfg.num_values(),
        instructions: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.layout.block_insts(block).count())
            .sum(),
        block_parameters: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.dfg.block_params(block).len())
            .sum(),
        ..PreRegallocMetrics::default()
    };
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define direct array COW function: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            "Cranelift returned no direct array COW code",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |layout| layout.frame_to_fp_offset);
    let code = compiled.code_buffer().to_vec();
    let alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations: Vec::new(),
        native_pc_ranges: Vec::new(),
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries: 0,
        production_lowering: Vec::new(),
    })
}

fn region_graph_metadata<'a>(
    root: FunctionId,
    root_local_count: u32,
    regions: impl Iterator<Item = &'a RegionGraph>,
    native_pc_ranges: Vec<crate::JitNativePcRange>,
    function_entries: Vec<crate::JitNativeFunctionEntryMetadata>,
    root_register_liveness: Option<&NativeRegisterLiveness>,
    value_flows: &BTreeMap<FunctionId, ExecutableValueFlow>,
    mut emitted_production_lowering: Vec<crate::JitProductionLoweringMetadata>,
) -> crate::JitRegionStateMetadata {
    let regions = regions.collect::<Vec<_>>();
    emitted_production_lowering.sort_by_key(|entry| (entry.function, entry.continuation_id));
    emitted_production_lowering.dedup_by_key(|entry| (entry.function, entry.continuation_id));
    let transition_liveness = regions
        .iter()
        .map(|region| {
            let liveness = root_register_liveness
                .filter(|_| region.function == root)
                .map_or_else(
                    || NativeRegisterLiveness::analyze(region).transition_live,
                    |liveness| liveness.transition_live.clone(),
                );
            (region.function, liveness)
        })
        .collect::<BTreeMap<_, _>>();
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
    let direct_callees = regions
        .iter()
        .flat_map(|region| region.direct_callees())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
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
        direct_callees,
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
                let liveness = &transition_liveness[&region.function];
                let value_flow = &value_flows[&region.function];
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
                        let live_registers = liveness
                            .get(&instruction.continuation_id)
                            .cloned()
                            .unwrap_or_default();
                        let owned_locals = instruction
                            .live_locals
                            .iter()
                            .copied()
                            .filter(|local| {
                                value_flow.local_storage(*local).is_native_frame_local()
                            })
                            .collect();
                        let owned_registers = live_registers
                            .iter()
                            .copied()
                            .filter(|register| {
                                crate::region_ir::value_release_required(
                                    value_flow.register_fact(*register),
                                )
                            })
                            .collect();
                        Some(crate::JitNativeSuspensionMetadata {
                            function: region.function,
                            continuation_id: instruction.continuation_id,
                            resume_id: crate::native_suspension_resume_id(
                                instruction.continuation_id,
                            ),
                            kind,
                            span: instruction.span,
                            live_locals: instruction.live_locals.clone(),
                            owned_locals,
                            live_registers,
                            owned_registers,
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
                let liveness = &transition_liveness[&region.function];
                let value_flow = &value_flows[&region.function];
                let mut transitions = region
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .filter_map(|instruction| {
                        if !instruction_has_native_transition(
                            instruction,
                            region.compile_metadata.tier,
                        ) {
                            return None;
                        }
                        let live_registers = liveness.get(&instruction.continuation_id)?;
                        (live_registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS).then(|| {
                            let owned_locals = instruction
                                .live_locals
                                .iter()
                                .copied()
                                .filter(|local| {
                                    value_flow.local_storage(*local).is_native_frame_local()
                                })
                                .collect();
                            let owned_registers = live_registers
                                .iter()
                                .copied()
                                .filter(|register| {
                                    crate::region_ir::value_release_required(
                                        value_flow.register_fact(*register),
                                    )
                                })
                                .collect();
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
                                owned_locals,
                                owned_registers,
                                result_register: region_instruction_result_register(
                                    &instruction.kind,
                                ),
                            }
                        })
                    })
                    .collect::<Vec<_>>();
                transitions.extend(region.blocks.iter().filter_map(|block| {
                    if !block_terminator_has_native_transition(block, region.compile_metadata.tier)
                    {
                        return None;
                    }
                    let continuation_id = block.terminator_continuation_id;
                    let live_registers = liveness.get(&continuation_id)?;
                    (live_registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS).then(|| {
                        let owned_locals = block
                            .terminator_live_locals
                            .iter()
                            .copied()
                            .filter(|local| {
                                value_flow.local_storage(*local).is_native_frame_local()
                            })
                            .collect();
                        let owned_registers = live_registers
                            .iter()
                            .copied()
                            .filter(|register| {
                                crate::region_ir::value_release_required(
                                    value_flow.register_fact(*register),
                                )
                            })
                            .collect();
                        crate::JitNativeTransitionMetadata {
                            function: region.function,
                            native_version: u32::from(
                                region.compile_metadata.tier == NativeCompilerTier::Optimizing,
                            ),
                            continuation_id,
                            resume_id: crate::native_transition_resume_id(continuation_id),
                            span: block.terminator_span,
                            live_locals: block.terminator_live_locals.clone(),
                            live_registers: live_registers.clone(),
                            owned_locals,
                            owned_registers,
                            result_register: None,
                        }
                    })
                }));
                transitions
            })
            .collect(),
        production_lowering: emitted_production_lowering,
        function_entries,
    }
}

#[cfg(test)]
mod tests {
    // Module-layout invariants are tested in `module_layout`; executable tests
    // exercise the resulting multi-function publication and invocation path.
}
