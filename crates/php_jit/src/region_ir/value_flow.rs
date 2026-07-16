//! Executable PHP value-flow analysis for optimizing Region IR lowering.

use std::collections::{BTreeMap, BTreeSet};

use php_ir::{IrConstant, IrReturnType, LocalId, RegId};

use super::{
    RegionBinaryOp, RegionCallResult, RegionCallTarget, RegionCastOp, RegionGraph,
    RegionInstructionKind, RegionNativeControl, RegionNativeDynamicCode, RegionNativeSuspend,
    RegionOperand, RegionUnaryOp, SsaOwnership, SsaValueClass, SsaValueFact,
    ssa::ExecutableSsaGraph,
};

/// Storage selected for a PHP local before Cranelift lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalStorageClass {
    SsaPlain,
    SsaMaybeUninitialized,
    MemoryReference,
    RequestGlobal,
    Superglobal,
    Globals,
    SuspensionPersistent,
}

impl LocalStorageClass {
    #[must_use]
    pub const fn is_promoted(self) -> bool {
        matches!(self, Self::SsaPlain | Self::SsaMaybeUninitialized)
    }
}

/// Facts that directly alter executable lowering decisions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutableValueFlow {
    local_storage: BTreeMap<LocalId, LocalStorageClass>,
    local_facts: BTreeMap<LocalId, SsaValueFact>,
    register_facts: BTreeMap<RegId, SsaValueFact>,
    borrowed_local_loads: BTreeSet<u32>,
    moved_local_stores: BTreeSet<u32>,
    elided_discards: BTreeSet<u32>,
    frame_cleanup_locals: BTreeSet<LocalId>,
    ssa: ExecutableSsaGraph,
}

impl ExecutableValueFlow {
    #[must_use]
    pub fn local_storage(&self, local: LocalId) -> LocalStorageClass {
        self.local_storage
            .get(&local)
            .copied()
            .unwrap_or(LocalStorageClass::SsaMaybeUninitialized)
    }

    #[must_use]
    pub fn local_fact(&self, local: LocalId) -> SsaValueFact {
        self.local_facts
            .get(&local)
            .copied()
            .unwrap_or(SsaValueFact::UNKNOWN)
    }

    #[must_use]
    pub fn register_fact(&self, register: RegId) -> SsaValueFact {
        self.register_facts
            .get(&register)
            .copied()
            .unwrap_or(SsaValueFact::UNKNOWN)
    }

    #[must_use]
    pub fn operand_fact(&self, constants: &[IrConstant], operand: RegionOperand) -> SsaValueFact {
        match operand {
            RegionOperand::Register(register) => self.register_fact(register),
            RegionOperand::Local(local) => self.local_fact(local),
            RegionOperand::I64(_) => {
                SsaValueFact::exact(SsaValueClass::Int, SsaOwnership::ImmortalConstant)
            }
            RegionOperand::Constant(index) => constants
                .get(index as usize)
                .map_or_else(|| reserved_constant_fact(index), constant_fact),
        }
    }

    #[must_use]
    pub fn promoted_local_count(&self) -> usize {
        self.local_storage
            .values()
            .filter(|storage| storage.is_promoted())
            .count()
    }

    #[must_use]
    pub fn promoted_register_count(&self) -> usize {
        self.register_facts
            .values()
            .filter(|fact| fact.certainty != super::SsaCertainty::Unknown)
            .count()
    }

    /// Whether this load's result can borrow the local's owning handle until
    /// its final same-block use.
    #[must_use]
    pub fn can_borrow_local_load(&self, continuation_id: u32) -> bool {
        self.borrowed_local_loads.contains(&continuation_id)
    }

    #[must_use]
    pub fn moves_value_into_local(&self, continuation_id: u32) -> bool {
        self.moved_local_stores.contains(&continuation_id)
    }

    #[must_use]
    pub fn elides_discard(&self, continuation_id: u32) -> bool {
        self.elided_discards.contains(&continuation_id)
    }

    #[must_use]
    pub const fn ssa(&self) -> &ExecutableSsaGraph {
        &self.ssa
    }

    #[must_use]
    pub fn ownership_move_count(&self) -> usize {
        self.moved_local_stores.len()
    }

    #[must_use]
    pub fn releases_local_at_frame_exit(&self, local: LocalId) -> bool {
        self.frame_cleanup_locals.contains(&local)
    }

    /// Verify the executable ownership decisions made by this analysis.
    ///
    /// This deliberately verifies the transformed decisions (borrowed loads,
    /// last-use moves, and elided discards), rather than treating ownership as
    /// report-only metadata.
    pub fn verify_ownership(&self, region: &RegionGraph) -> Result<(), String> {
        let mut instruction_uses = BTreeMap::<RegId, Vec<(usize, usize, u32)>>::new();
        let mut terminator_uses = BTreeSet::new();
        for (block_index, block) in region.blocks.iter().enumerate() {
            for (instruction_index, instruction) in block.instructions.iter().enumerate() {
                for register in instruction.register_uses() {
                    instruction_uses.entry(register).or_default().push((
                        block_index,
                        instruction_index,
                        instruction.continuation_id,
                    ));
                }
            }
            terminator_uses.extend(block.terminator.register_uses());
        }

        for (block_index, block) in region.blocks.iter().enumerate() {
            for (instruction_index, instruction) in block.instructions.iter().enumerate() {
                match instruction.kind {
                    RegionInstructionKind::LoadLocal { dst, .. }
                        if self
                            .borrowed_local_loads
                            .contains(&instruction.continuation_id) =>
                    {
                        if terminator_uses.contains(&dst) {
                            return Err(format!(
                                "borrowed r{} escapes through a terminator",
                                dst.raw()
                            ));
                        }
                        if instruction_uses.get(&dst).into_iter().flatten().any(
                            |&(use_block, use_index, _)| {
                                use_block != block_index || use_index <= instruction_index
                            },
                        ) {
                            return Err(format!(
                                "borrowed r{} escapes its forward same-block lifetime",
                                dst.raw()
                            ));
                        }
                    }
                    RegionInstructionKind::StoreLocal {
                        src: RegionOperand::Register(src),
                        ..
                    } if self
                        .moved_local_stores
                        .contains(&instruction.continuation_id) =>
                    {
                        if terminator_uses.contains(&src) {
                            return Err(format!("moved r{} is used by a terminator", src.raw()));
                        }
                        let invalid_use = instruction_uses.get(&src).into_iter().flatten().find(
                            |&&(use_block, use_index, continuation)| {
                                (use_block, use_index) != (block_index, instruction_index)
                                    && !self.elided_discards.contains(&continuation)
                            },
                        );
                        if let Some(&(_, _, continuation)) = invalid_use {
                            return Err(format!(
                                "moved r{} is reused at continuation {}",
                                src.raw(),
                                continuation
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }

        for continuation in &self.elided_discards {
            let is_discard = region
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|instruction| {
                    instruction.continuation_id == *continuation
                        && matches!(instruction.kind, RegionInstructionKind::Discard { .. })
                });
            if !is_discard {
                return Err(format!(
                    "ownership elision references non-discard continuation {continuation}"
                ));
            }
        }
        Ok(())
    }
}

/// Build the value facts used by optimizing Cranelift lowering.
#[must_use]
pub fn analyze_executable_value_flow(
    region: &RegionGraph,
    constants: &[IrConstant],
) -> ExecutableValueFlow {
    let local_storage = classify_locals(region);
    let eligible_locals = local_storage
        .iter()
        .filter_map(|(local, storage)| storage.is_promoted().then_some(*local))
        .collect::<BTreeSet<_>>();
    let ssa = super::build_executable_ssa(region, &eligible_locals);
    debug_assert!(ssa.verify(region).is_ok());
    let mut local_facts = initial_local_facts(region, &local_storage);
    let mut register_facts = BTreeMap::new();

    // Register definitions and local stores form a small monotone system. A
    // bounded fixed point handles loop-carried local values without making
    // lowering depend on source block order.
    let iteration_limit = region
        .register_count
        .saturating_add(region.local_count)
        .saturating_add(1) as usize;
    for _ in 0..iteration_limit {
        let previous_locals = local_facts.clone();
        let previous_registers = register_facts.clone();
        let mut stored_facts = BTreeMap::<LocalId, Vec<SsaValueFact>>::new();
        for block in &region.blocks {
            for instruction in &block.instructions {
                if let Some((register, fact)) = instruction_result_fact(
                    &instruction.kind,
                    constants,
                    &local_facts,
                    &register_facts,
                ) {
                    // Executable Region IR registers are single-assignment;
                    // reevaluation replaces the previous iteration's fact.
                    register_facts.insert(register, fact);
                }
                match &instruction.kind {
                    RegionInstructionKind::StoreLocal { local, src }
                    | RegionInstructionKind::AssignLocalResult {
                        local, value: src, ..
                    } => stored_facts.entry(*local).or_default().push(operand_fact(
                        constants,
                        &local_facts,
                        &register_facts,
                        *src,
                    )),
                    RegionInstructionKind::UnsetLocal { local } => stored_facts
                        .entry(*local)
                        .or_default()
                        .push(SsaValueFact::exact(
                            SsaValueClass::Uninitialized,
                            SsaOwnership::ImmortalConstant,
                        )),
                    _ => {}
                }
            }
        }
        for (local, facts) in stored_facts {
            let stored = facts
                .into_iter()
                .reduce(join_facts)
                .unwrap_or(SsaValueFact::UNKNOWN);
            let fact = if region
                .params
                .iter()
                .any(|parameter| parameter.local == local)
            {
                join_facts(
                    initial_fact_for_local(region, local, &local_storage),
                    stored,
                )
            } else {
                stored
            };
            local_facts.insert(local, fact);
        }
        if local_facts == previous_locals && register_facts == previous_registers {
            break;
        }
    }

    let borrowed_local_loads = find_borrowed_local_loads(region, &local_storage);
    for block in &region.blocks {
        for instruction in &block.instructions {
            let RegionInstructionKind::LoadLocal { dst, .. } = instruction.kind else {
                continue;
            };
            if borrowed_local_loads.contains(&instruction.continuation_id)
                && let Some(fact) = register_facts.get_mut(&dst)
            {
                fact.ownership = SsaOwnership::Borrowed;
            }
        }
    }
    let (moved_local_stores, elided_discards) =
        find_moved_local_stores(region, &local_storage, &register_facts);
    let frame_cleanup_locals =
        find_frame_cleanup_locals(region, &moved_local_stores, &local_storage);

    ExecutableValueFlow {
        local_storage,
        local_facts,
        register_facts,
        borrowed_local_loads,
        moved_local_stores,
        elided_discards,
        frame_cleanup_locals,
        ssa,
    }
}

fn find_frame_cleanup_locals(
    region: &RegionGraph,
    moved_stores: &BTreeSet<u32>,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
) -> BTreeSet<LocalId> {
    let mut candidates = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::StoreLocal { local, .. }
                if moved_stores.contains(&instruction.continuation_id)
                    && storage.get(&local).is_some_and(|class| class.is_promoted()) =>
            {
                Some(local)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    for local in candidates.clone() {
        let loaded = region
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction.kind {
                RegionInstructionKind::LoadLocal {
                    dst, local: loaded, ..
                } if loaded == local => Some(dst),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let escapes_call = region
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| match &instruction.kind {
                RegionInstructionKind::NativeCall(call) => {
                    call.args
                        .iter()
                        .any(|argument| argument.by_ref_local == Some(local))
                        || instruction
                            .register_uses()
                            .into_iter()
                            .any(|register| loaded.contains(&register))
                }
                RegionInstructionKind::NativeDynamicCode(_)
                | RegionInstructionKind::NativeSuspend(_) => {
                    instruction.live_locals.contains(&local)
                }
                _ => false,
            });
        if escapes_call {
            candidates.remove(&local);
        }
    }
    candidates
}

fn find_moved_local_stores(
    region: &RegionGraph,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
    register_facts: &BTreeMap<RegId, SsaValueFact>,
) -> (BTreeSet<u32>, BTreeSet<u32>) {
    let mut uses = BTreeMap::<RegId, Vec<(usize, usize, bool, u32)>>::new();
    let mut terminator_uses = BTreeSet::new();
    for (block_index, block) in region.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            let discarded = matches!(instruction.kind, RegionInstructionKind::Discard { .. });
            for register in instruction.register_uses() {
                uses.entry(register).or_default().push((
                    block_index,
                    instruction_index,
                    discarded,
                    instruction.continuation_id,
                ));
            }
        }
        terminator_uses.extend(block.terminator.register_uses());
    }

    let mut moved_stores = BTreeSet::new();
    let mut elided_discards = BTreeSet::new();
    for (block_index, block) in region.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            let RegionInstructionKind::StoreLocal {
                local,
                src: RegionOperand::Register(register),
            } = instruction.kind
            else {
                continue;
            };
            let fact = register_facts
                .get(&register)
                .copied()
                .unwrap_or(SsaValueFact::UNKNOWN);
            if !storage
                .get(&local)
                .is_some_and(|storage| storage.is_promoted())
                || fact.ownership != SsaOwnership::Owned
                || terminator_uses.contains(&register)
            {
                continue;
            }
            let remaining = uses
                .get(&register)
                .into_iter()
                .flatten()
                .filter(|&&(use_block, use_index, _, _)| {
                    use_block != block_index || use_index != instruction_index
                })
                .copied()
                .collect::<Vec<_>>();
            match remaining.as_slice() {
                [] => {
                    moved_stores.insert(instruction.continuation_id);
                }
                [(use_block, use_index, true, discard_continuation)]
                    if *use_block == block_index && *use_index > instruction_index =>
                {
                    moved_stores.insert(instruction.continuation_id);
                    elided_discards.insert(*discard_continuation);
                }
                _ => {}
            }
        }
    }
    (moved_stores, elided_discards)
}

fn find_borrowed_local_loads(
    region: &RegionGraph,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
) -> BTreeSet<u32> {
    let mut uses = BTreeMap::<RegId, Vec<(usize, usize)>>::new();
    let mut terminator_uses = BTreeSet::new();
    for (block_index, block) in region.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            for register in instruction.register_uses() {
                uses.entry(register)
                    .or_default()
                    .push((block_index, instruction_index));
            }
        }
        terminator_uses.extend(block.terminator.register_uses());
    }

    let mut borrowed = BTreeSet::new();
    for (block_index, block) in region.blocks.iter().enumerate() {
        for (load_index, instruction) in block.instructions.iter().enumerate() {
            let RegionInstructionKind::LoadLocal { dst, local, .. } = instruction.kind else {
                continue;
            };
            if !storage
                .get(&local)
                .is_some_and(|storage| storage.is_promoted())
                || !instruction.live_locals.contains(&local)
                || terminator_uses.contains(&dst)
            {
                continue;
            }
            let Some(register_uses) = uses.get(&dst).filter(|uses| !uses.is_empty()) else {
                continue;
            };
            if register_uses
                .iter()
                .any(|&(use_block, use_index)| use_block != block_index || use_index <= load_index)
            {
                continue;
            }
            let last_use = register_uses
                .iter()
                .map(|&(_, use_index)| use_index)
                .max()
                .expect("non-empty register use list");
            if block.instructions[(load_index + 1)..last_use]
                .iter()
                .any(|instruction| instruction_mutates_local(&instruction.kind, local))
            {
                continue;
            }
            borrowed.insert(instruction.continuation_id);
        }
    }
    borrowed
}

fn instruction_mutates_local(kind: &RegionInstructionKind, local: LocalId) -> bool {
    match kind {
        RegionInstructionKind::StoreLocal { local: target, .. }
        | RegionInstructionKind::AssignLocalResult { local: target, .. }
        | RegionInstructionKind::InitStaticLocal { local: target, .. }
        | RegionInstructionKind::AssignDim { local: target, .. }
        | RegionInstructionKind::AppendDim { local: target, .. }
        | RegionInstructionKind::UnsetDim { local: target, .. }
        | RegionInstructionKind::UnsetLocal { local: target }
        | RegionInstructionKind::ForeachInitRef { local: target, .. }
        | RegionInstructionKind::ForeachNextRef {
            value_local: target,
            ..
        } => *target == local,
        RegionInstructionKind::BindReference { target, source } => {
            *target == local || *source == local
        }
        RegionInstructionKind::BindReferenceDim { target, array, .. }
        | RegionInstructionKind::BindReferenceFromPropertyDim {
            target,
            object: RegionOperand::Local(array),
            ..
        } => *target == local || *array == local,
        RegionInstructionKind::BindReferenceIntoDim { array, source, .. }
        | RegionInstructionKind::BindReferenceDimFromProperty {
            array,
            object: RegionOperand::Local(source),
            ..
        } => *array == local || *source == local,
        RegionInstructionKind::BindReferenceProperty { source, .. }
        | RegionInstructionKind::BindReferenceStaticProperty { source }
        | RegionInstructionKind::BindReferenceIntoPropertyDim { source, .. } => *source == local,
        RegionInstructionKind::BindReferenceFromProperty { target, .. }
        | RegionInstructionKind::BindReferenceFromPropertyDim { target, .. } => *target == local,
        RegionInstructionKind::NativeCall(call) => {
            matches!(call.result, RegionCallResult::ReferenceLocal(target) if target == local)
                || call.args.iter().enumerate().any(|(index, argument)| {
                    call.argument_requires_reference_binding(index)
                        && argument.by_ref_local == Some(local)
                })
        }
        _ => false,
    }
}

fn classify_locals(region: &RegionGraph) -> BTreeMap<LocalId, LocalStorageClass> {
    const SUPERGLOBALS: &[&str] = &[
        "_SERVER", "_GET", "_POST", "_FILES", "_COOKIE", "_SESSION", "_REQUEST", "_ENV",
    ];
    let mut references = region
        .params
        .iter()
        .filter(|parameter| parameter.by_ref)
        .map(|parameter| parameter.local)
        .chain(
            region
                .captures
                .iter()
                .filter(|capture| capture.by_ref)
                .map(|capture| capture.local),
        )
        .collect::<BTreeSet<_>>();
    let mut suspension = BTreeSet::new();
    for block in &region.blocks {
        for instruction in &block.instructions {
            match &instruction.kind {
                RegionInstructionKind::BindReference { target, source } => {
                    references.extend([*target, *source]);
                }
                RegionInstructionKind::BindReferenceDim { target, array, .. }
                | RegionInstructionKind::BindReferenceFromPropertyDim {
                    target,
                    object: RegionOperand::Local(array),
                    ..
                } => {
                    references.extend([*target, *array]);
                }
                RegionInstructionKind::BindReferenceIntoDim { array, source, .. }
                | RegionInstructionKind::BindReferenceDimFromProperty {
                    array,
                    object: RegionOperand::Local(source),
                    ..
                } => {
                    references.extend([*array, *source]);
                }
                RegionInstructionKind::BindReferenceProperty { source, .. }
                | RegionInstructionKind::BindReferenceStaticProperty { source }
                | RegionInstructionKind::BindReferenceIntoPropertyDim { source, .. } => {
                    references.insert(*source);
                }
                RegionInstructionKind::BindReferenceFromProperty { target, .. }
                | RegionInstructionKind::BindReferenceFromPropertyDim { target, .. }
                | RegionInstructionKind::InitStaticLocal { local: target, .. }
                | RegionInstructionKind::ForeachNextRef {
                    value_local: target,
                    ..
                } => {
                    references.insert(*target);
                }
                RegionInstructionKind::ForeachInitRef { local, .. } => {
                    references.insert(*local);
                }
                RegionInstructionKind::NativeCall(call) => {
                    for (index, argument) in call.args.iter().enumerate() {
                        if call.argument_requires_reference_binding(index)
                            && let Some(local) = argument.by_ref_local
                        {
                            references.insert(local);
                        }
                    }
                    if let RegionCallResult::ReferenceLocal(local) = call.result {
                        references.insert(local);
                    }
                }
                RegionInstructionKind::NativeSuspend(_) => {
                    suspension.extend(instruction.live_locals.iter().copied());
                }
                _ => {}
            }
        }
        if let super::RegionTerminator::ReturnReference { local, .. } = block.terminator {
            references.insert(local);
        }
    }

    (0..region.local_count)
        .map(LocalId::new)
        .map(|local| {
            let name = region.locals.get(local.index()).map(String::as_str);
            let storage = if name == Some("GLOBALS") {
                LocalStorageClass::Globals
            } else if name.is_some_and(|name| SUPERGLOBALS.contains(&name)) {
                LocalStorageClass::Superglobal
            } else if region.flags.is_top_level {
                LocalStorageClass::RequestGlobal
            } else if references.contains(&local) {
                LocalStorageClass::MemoryReference
            } else if suspension.contains(&local) {
                LocalStorageClass::SuspensionPersistent
            } else if region.blocks.iter().all(|block| {
                block.instructions.iter().all(|instruction| {
                    !matches!(
                        instruction.kind,
                        RegionInstructionKind::LoadLocal { local: loaded, .. }
                            if loaded == local && !instruction.live_locals.contains(&local)
                    )
                })
            }) {
                LocalStorageClass::SsaPlain
            } else {
                LocalStorageClass::SsaMaybeUninitialized
            };
            (local, storage)
        })
        .collect()
}

fn initial_local_facts(
    region: &RegionGraph,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
) -> BTreeMap<LocalId, SsaValueFact> {
    (0..region.local_count)
        .map(LocalId::new)
        .map(|local| (local, initial_fact_for_local(region, local, storage)))
        .collect()
}

fn initial_fact_for_local(
    region: &RegionGraph,
    local: LocalId,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
) -> SsaValueFact {
    if !storage
        .get(&local)
        .is_some_and(|storage| storage.is_promoted())
    {
        return SsaValueFact::UNKNOWN;
    }
    if let Some(parameter) = region
        .params
        .iter()
        .find(|parameter| parameter.local == local)
    {
        return parameter
            .type_
            .as_ref()
            .map_or(SsaValueFact::UNKNOWN, type_fact);
    }
    SsaValueFact::exact(SsaValueClass::Uninitialized, SsaOwnership::ImmortalConstant)
}

fn operand_fact(
    constants: &[IrConstant],
    locals: &BTreeMap<LocalId, SsaValueFact>,
    registers: &BTreeMap<RegId, SsaValueFact>,
    operand: RegionOperand,
) -> SsaValueFact {
    match operand {
        RegionOperand::Register(register) => registers
            .get(&register)
            .copied()
            .unwrap_or(SsaValueFact::UNKNOWN),
        RegionOperand::Local(local) => locals.get(&local).copied().unwrap_or(SsaValueFact::UNKNOWN),
        RegionOperand::I64(_) => {
            SsaValueFact::exact(SsaValueClass::Int, SsaOwnership::ImmortalConstant)
        }
        RegionOperand::Constant(index) => constants
            .get(index as usize)
            .map_or_else(|| reserved_constant_fact(index), constant_fact),
    }
}

fn instruction_result_fact(
    kind: &RegionInstructionKind,
    constants: &[IrConstant],
    locals: &BTreeMap<LocalId, SsaValueFact>,
    registers: &BTreeMap<RegId, SsaValueFact>,
) -> Option<(RegId, SsaValueFact)> {
    let fact = |operand| operand_fact(constants, locals, registers, operand);
    match kind {
        RegionInstructionKind::Move { dst, src }
        | RegionInstructionKind::AssignLocalResult {
            dst, value: src, ..
        } => Some((*dst, fact(*src))),
        RegionInstructionKind::LoadLocal { dst, local, .. } => Some((
            *dst,
            locals.get(local).copied().unwrap_or(SsaValueFact::UNKNOWN),
        )),
        RegionInstructionKind::Binary { dst, op, lhs, rhs } => {
            let lhs = fact(*lhs);
            let rhs = fact(*rhs);
            let output = if lhs.class == SsaValueClass::Int && rhs.class == SsaValueClass::Int {
                match op {
                    RegionBinaryOp::Add
                    | RegionBinaryOp::Sub
                    | RegionBinaryOp::Mul
                    | RegionBinaryOp::Mod
                    | RegionBinaryOp::BitAnd
                    | RegionBinaryOp::BitOr
                    | RegionBinaryOp::BitXor
                    | RegionBinaryOp::ShiftLeft
                    | RegionBinaryOp::ShiftRight => {
                        SsaValueFact::known(SsaValueClass::Int, SsaOwnership::Owned)
                    }
                    RegionBinaryOp::Div | RegionBinaryOp::Pow => SsaValueFact::UNKNOWN,
                    RegionBinaryOp::Concat => {
                        SsaValueFact::known(SsaValueClass::StringHandle, SsaOwnership::Owned)
                    }
                }
            } else {
                SsaValueFact::UNKNOWN
            };
            Some((*dst, output))
        }
        RegionInstructionKind::Unary { dst, op, src } => {
            let input = fact(*src);
            let output = match op {
                RegionUnaryOp::Not => SsaValueFact::known(SsaValueClass::Bool, SsaOwnership::Owned),
                RegionUnaryOp::Plus | RegionUnaryOp::Minus | RegionUnaryOp::BitNot
                    if input.class == SsaValueClass::Int =>
                {
                    SsaValueFact::known(SsaValueClass::Int, SsaOwnership::Owned)
                }
                _ => SsaValueFact::UNKNOWN,
            };
            Some((*dst, output))
        }
        RegionInstructionKind::Compare {
            dst,
            op: super::RegionCompareOpCode::Spaceship,
            ..
        } => Some((
            *dst,
            SsaValueFact::known(SsaValueClass::Int, SsaOwnership::Owned),
        )),
        RegionInstructionKind::Compare { dst, .. }
        | RegionInstructionKind::IssetDim { dst, .. }
        | RegionInstructionKind::EmptyDim { dst, .. }
        | RegionInstructionKind::IssetLocal { dst, .. }
        | RegionInstructionKind::EmptyLocal { dst, .. } => Some((
            *dst,
            SsaValueFact::known(SsaValueClass::Bool, SsaOwnership::Owned),
        )),
        RegionInstructionKind::Cast { dst, op, .. } => Some((
            *dst,
            SsaValueFact::known(
                match op {
                    RegionCastOp::Bool => SsaValueClass::Bool,
                    RegionCastOp::Int => SsaValueClass::Int,
                    RegionCastOp::Float => SsaValueClass::Float,
                    RegionCastOp::String => SsaValueClass::StringHandle,
                    RegionCastOp::Array => SsaValueClass::ArrayHandle,
                    RegionCastOp::Object => SsaValueClass::ObjectHandle,
                    RegionCastOp::Void => SsaValueClass::Null,
                },
                SsaOwnership::Owned,
            ),
        )),
        RegionInstructionKind::NewArray { dst } => Some((
            *dst,
            SsaValueFact::known(SsaValueClass::ArrayHandle, SsaOwnership::Owned),
        )),
        RegionInstructionKind::NewObject { dst, .. }
        | RegionInstructionKind::CloneObject { dst, .. }
        | RegionInstructionKind::CloneWith { dst, .. }
        | RegionInstructionKind::NativeControl(RegionNativeControl::MakeException {
            dst, ..
        }) => Some((
            *dst,
            SsaValueFact::known(SsaValueClass::ObjectHandle, SsaOwnership::Owned),
        )),
        RegionInstructionKind::FetchObjectClassName { dst, .. } => Some((
            *dst,
            SsaValueFact::known(SsaValueClass::StringHandle, SsaOwnership::Owned),
        )),
        RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::MakeClosure {
            dst,
            ..
        }) => Some((
            *dst,
            SsaValueFact::known(SsaValueClass::CallableHandle, SsaOwnership::Owned),
        )),
        RegionInstructionKind::NativeSuspend(
            RegionNativeSuspend::GeneratorYield { dst, .. }
            | RegionNativeSuspend::GeneratorDelegate { dst, .. }
            | RegionNativeSuspend::FiberSuspend { dst, .. },
        )
        | RegionInstructionKind::NativeDynamicCode(
            RegionNativeDynamicCode::Include { dst, .. }
            | RegionNativeDynamicCode::Eval { dst, .. },
        ) => Some((*dst, SsaValueFact::UNKNOWN)),
        RegionInstructionKind::NativeCall(call) => match call.result {
            RegionCallResult::Register(dst) => {
                let class = match &call.target {
                    RegionCallTarget::Function { .. }
                    | RegionCallTarget::Method { .. }
                    | RegionCallTarget::StaticMethod { .. }
                    | RegionCallTarget::Closure { .. }
                    | RegionCallTarget::Callable { .. }
                    | RegionCallTarget::Pipe { .. }
                    | RegionCallTarget::Constructor { .. }
                    | RegionCallTarget::DynamicConstructor { .. }
                    | RegionCallTarget::Semantic { .. } => SsaValueFact::UNKNOWN,
                };
                Some((dst, class))
            }
            RegionCallResult::ReferenceLocal(_) | RegionCallResult::Discard => None,
        },
        _ => None,
    }
}

fn constant_fact(constant: &IrConstant) -> SsaValueFact {
    let class = match constant {
        IrConstant::Null => SsaValueClass::Null,
        IrConstant::Bool(_) => SsaValueClass::Bool,
        IrConstant::Int(_) => SsaValueClass::Int,
        IrConstant::Float(_) => SsaValueClass::Float,
        IrConstant::String(_) | IrConstant::StringBytes(_) => SsaValueClass::StringHandle,
        IrConstant::Array(_) => SsaValueClass::ArrayHandle,
        IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => {
            return SsaValueFact::UNKNOWN;
        }
    };
    SsaValueFact::exact(class, SsaOwnership::ImmortalConstant)
}

fn reserved_constant_fact(index: u32) -> SsaValueFact {
    let class = if index == u32::MAX {
        SsaValueClass::Null
    } else if matches!(index, crate::JIT_VALUE_FALSE | crate::JIT_VALUE_TRUE) {
        SsaValueClass::Bool
    } else if index == crate::JIT_VALUE_UNINITIALIZED {
        SsaValueClass::Uninitialized
    } else {
        return SsaValueFact::UNKNOWN;
    };
    SsaValueFact::exact(class, SsaOwnership::ImmortalConstant)
}

fn type_fact(type_: &IrReturnType) -> SsaValueFact {
    let class = match type_ {
        IrReturnType::Null => SsaValueClass::Null,
        IrReturnType::Bool | IrReturnType::True | IrReturnType::False => SsaValueClass::Bool,
        IrReturnType::Int => SsaValueClass::Int,
        IrReturnType::Float => SsaValueClass::Float,
        IrReturnType::String => SsaValueClass::StringHandle,
        IrReturnType::Array | IrReturnType::Iterable => SsaValueClass::ArrayHandle,
        IrReturnType::Object | IrReturnType::Class { .. } => SsaValueClass::ObjectHandle,
        IrReturnType::Callable => SsaValueClass::CallableHandle,
        IrReturnType::Mixed
        | IrReturnType::Void
        | IrReturnType::Never
        | IrReturnType::Nullable { .. }
        | IrReturnType::Union { .. }
        | IrReturnType::Intersection { .. }
        | IrReturnType::Dnf { .. } => return SsaValueFact::UNKNOWN,
    };
    SsaValueFact::known(class, SsaOwnership::Borrowed)
}

fn join_facts(left: SsaValueFact, right: SsaValueFact) -> SsaValueFact {
    if left == right {
        return left;
    }
    if left.class != right.class {
        return SsaValueFact::UNKNOWN;
    }
    SsaValueFact::known(
        left.class,
        if left.ownership == right.ownership {
            left.ownership
        } else {
            SsaOwnership::Unknown
        },
    )
}

#[cfg(test)]
mod tests {
    use php_ir::{
        FunctionFlags, InstructionKind, IrBuilder, IrParam, IrReturnType, IrSpan, Operand, UnitId,
    };

    use super::*;
    use crate::region_ir::build_baseline_region;

    #[test]
    fn promotes_initialized_scalar_local_and_tracks_register_chain() {
        let mut builder = IrBuilder::new(UnitId::new(4_201));
        let file = builder.add_file("ssa-flow.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("flow", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        let block = builder.append_block(function);
        let one = builder.intern_constant(IrConstant::Int(1));
        builder.emit(
            function,
            block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Constant(one),
            },
            span,
        );
        let loaded = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal { dst: loaded, local },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(loaded)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);

        assert_eq!(flow.local_storage(local), LocalStorageClass::SsaPlain);
        assert_eq!(flow.local_fact(local).class, SsaValueClass::Int);
        assert_eq!(flow.register_fact(loaded).class, SsaValueClass::Int);
        assert_eq!(flow.promoted_local_count(), 1);
    }

    #[test]
    fn borrows_promoted_handle_through_same_block_uses() {
        let mut builder = IrBuilder::new(UnitId::new(4_204));
        let file = builder.add_file("ssa-borrow.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("borrow", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        builder.push_param(
            function,
            IrParam {
                name: "value".to_owned(),
                local,
                required: true,
                default: None,
                type_: Some(IrReturnType::String),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let block = builder.append_block(function);
        let borrowed = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: borrowed,
                local,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Echo {
                src: Operand::Register(borrowed),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(borrowed),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);

        assert!(flow.can_borrow_local_load(region.blocks[0].instructions[0].continuation_id));
        assert_eq!(
            flow.register_fact(borrowed).ownership,
            SsaOwnership::Borrowed
        );
        flow.verify_ownership(&region)
            .expect("same-block borrow should verify");
    }

    #[test]
    fn ownership_verifier_rejects_use_after_forced_move() {
        let mut builder = IrBuilder::new(UnitId::new(4_211));
        let file = builder.add_file("ssa-use-after-move.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("use_after_move", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        let block = builder.append_block(function);
        let array = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: array },
            span,
        );
        let store = builder.emit(
            function,
            block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(array),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Echo {
                src: Operand::Register(array),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let mut flow = analyze_executable_value_flow(&region, &unit.constants);
        let store_continuation = region.blocks[0].instructions[store.index()].continuation_id;
        flow.moved_local_stores.insert(store_continuation);

        let error = flow
            .verify_ownership(&region)
            .expect_err("forced move must reject later echo use");
        assert!(error.contains("reused"), "{error}");
    }
}
