//! Executable PHP value-flow analysis for optimizing Region IR lowering.

use std::collections::{BTreeMap, BTreeSet};

use php_ir::{IrConstant, IrReturnType, LocalId, RegId};

use super::{
    RegionBinaryOp, RegionCallResult, RegionCallTarget, RegionCastOp, RegionGraph,
    RegionInstructionKind, RegionNativeControl, RegionNativeDynamicCode, RegionNativeSuspend,
    RegionOperand, RegionSemanticOp, RegionTerminator, RegionUnaryOp, SsaIntegerRange,
    SsaOwnership, SsaValueClass, SsaValueFact, ssa::ExecutableSsaGraph,
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

    /// Whether the encoded local slot is authoritative inside a native
    /// fragment. Request globals and superglobals remain runtime-owned.
    #[must_use]
    pub const fn is_native_frame_local(self) -> bool {
        !matches!(
            self,
            Self::RequestGlobal | Self::Superglobal | Self::Globals
        )
    }

    #[must_use]
    pub const fn is_reference_slot(self) -> bool {
        matches!(
            self,
            Self::MemoryReference | Self::RequestGlobal | Self::Superglobal
        )
    }
}

/// Facts that directly alter executable lowering decisions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutableValueFlow {
    local_storage: BTreeMap<LocalId, LocalStorageClass>,
    local_facts: BTreeMap<LocalId, SsaValueFact>,
    register_facts: BTreeMap<RegId, SsaValueFact>,
    borrowed_local_loads: BTreeSet<u32>,
    reference_dimension_loads: BTreeMap<u32, RegId>,
    moved_local_stores: BTreeSet<u32>,
    moved_register_copies: BTreeSet<u32>,
    consumed_call_operands: BTreeSet<(u32, RegId)>,
    elided_discards: BTreeSet<u32>,
    frame_cleanup_locals: BTreeSet<LocalId>,
    owned_entry_parameters: BTreeSet<LocalId>,
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
    pub fn frame_cleanup_locals(&self) -> impl Iterator<Item = LocalId> + '_ {
        self.frame_cleanup_locals.iter().copied()
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
            RegionOperand::I64(value) => {
                SsaValueFact::exact(SsaValueClass::Int, SsaOwnership::ImmortalConstant)
                    .with_integer_range(SsaIntegerRange::exact(value))
            }
            RegionOperand::Constant(index) => constants
                .get(index as usize)
                .map_or_else(|| reserved_constant_fact(index), constant_fact),
            RegionOperand::LinkedConstant { class, .. } => {
                SsaValueFact::exact(class, SsaOwnership::Borrowed)
            }
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

    /// Whether this reference local can remain encoded until its typed
    /// dimension consumer dereferences it.
    #[must_use]
    pub fn passes_reference_to_typed_consumer(&self, continuation_id: u32) -> bool {
        self.reference_dimension_loads
            .contains_key(&continuation_id)
    }

    #[must_use]
    pub fn moves_value_into_local(&self, continuation_id: u32) -> bool {
        self.moved_local_stores.contains(&continuation_id)
    }

    /// Whether an SSA copy is the source owner's final use and therefore
    /// transfers that ownership to its destination without refcount traffic.
    #[must_use]
    pub fn moves_value_into_register(&self, continuation_id: u32) -> bool {
        self.moved_register_copies.contains(&continuation_id)
    }

    /// Whether this semantic call is the final owner-bearing use of a
    /// register and therefore must release the synchronous ABI borrow after
    /// the call returns.
    #[must_use]
    pub fn consumes_call_operand(&self, continuation_id: u32, register: RegId) -> bool {
        self.consumed_call_operands
            .contains(&(continuation_id, register))
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
        self.moved_local_stores
            .len()
            .saturating_add(self.moved_register_copies.len())
    }

    #[must_use]
    pub fn releases_local_at_frame_exit(&self, local: LocalId) -> bool {
        self.frame_cleanup_locals.contains(&local)
    }

    /// Whether a mutable by-value parameter receives a frame-owned native
    /// handle at entry instead of borrowing the caller's handle.
    #[must_use]
    pub fn owns_parameter_at_entry(&self, local: LocalId) -> bool {
        self.owned_entry_parameters.contains(&local)
    }

    /// Verify the executable ownership decisions made by this analysis.
    ///
    /// This deliberately verifies the transformed decisions (borrowed loads,
    /// last-use moves, and elided discards), rather than treating ownership as
    /// report-only metadata.
    pub fn verify_ownership(&self, region: &RegionGraph) -> Result<(), String> {
        let reachability = block_reachability(region);
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
                        let retained_result = matches!(
                            block.terminator,
                            RegionTerminator::Return {
                                value: RegionOperand::Register(register),
                                ..
                            } if register == dst
                        ) || matches!(
                            block.terminator,
                            RegionTerminator::Exit {
                                value: Some(RegionOperand::Register(register)),
                                ..
                            } if register == dst
                        );
                        if terminator_uses.contains(&dst) && !retained_result {
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
                                use_may_follow(
                                    &reachability,
                                    block_index,
                                    instruction_index,
                                    use_block,
                                    use_index,
                                ) && !self.elided_discards.contains(&continuation)
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
                    RegionInstructionKind::Move {
                        src: RegionOperand::Register(src),
                        ..
                    } if self
                        .moved_register_copies
                        .contains(&instruction.continuation_id) =>
                    {
                        if terminator_uses.contains(&src) {
                            return Err(format!("moved r{} is used by a terminator", src.raw()));
                        }
                        let invalid_use = instruction_uses.get(&src).into_iter().flatten().find(
                            |&&(use_block, use_index, continuation)| {
                                use_may_follow(
                                    &reachability,
                                    block_index,
                                    instruction_index,
                                    use_block,
                                    use_index,
                                ) && !self.elided_discards.contains(&continuation)
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

fn block_reachability(region: &RegionGraph) -> Vec<BTreeSet<usize>> {
    let block_indices = region
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect::<BTreeMap<_, _>>();
    let successors = region
        .blocks
        .iter()
        .map(|block| {
            let targets = match block.terminator {
                RegionTerminator::Jump { target } => vec![target],
                RegionTerminator::JumpIfFalse {
                    target,
                    fallthrough,
                    ..
                }
                | RegionTerminator::JumpIfTrue {
                    target,
                    fallthrough,
                    ..
                } => vec![target, fallthrough],
                RegionTerminator::JumpIf {
                    if_true, if_false, ..
                } => vec![if_true, if_false],
                RegionTerminator::Return { finally, .. }
                | RegionTerminator::ReturnReference { finally, .. } => {
                    finally.into_iter().collect()
                }
                RegionTerminator::Exit { finally, .. } => finally.into_iter().collect(),
            };
            targets
                .into_iter()
                .filter_map(|target| block_indices.get(&target).copied())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    (0..region.blocks.len())
        .map(|start| {
            let mut reachable = BTreeSet::new();
            let mut pending = successors[start].clone();
            while let Some(block) = pending.pop() {
                if reachable.insert(block) {
                    pending.extend(successors[block].iter().copied());
                }
            }
            reachable
        })
        .collect()
}

fn use_may_follow(
    reachability: &[BTreeSet<usize>],
    block: usize,
    instruction: usize,
    use_block: usize,
    use_instruction: usize,
) -> bool {
    if use_block == block {
        use_instruction > instruction
    } else {
        reachability
            .get(block)
            .is_some_and(|reachable| reachable.contains(&use_block))
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
            let fact = if !local_storage
                .get(&local)
                .is_some_and(|storage| storage.is_promoted())
            {
                // References, request globals, and suspension-backed locals can
                // change through storage that is not represented by StoreLocal
                // instructions in this region. Do not specialize their loaded
                // values from the stores that happen to be visible here.
                SsaValueFact::UNKNOWN
            } else if region.parameter_locals.contains(&local) {
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
    let reference_dimension_loads = find_reference_dimension_loads(region, &local_storage);
    for block in &region.blocks {
        for instruction in &block.instructions {
            let RegionInstructionKind::LoadLocal { dst, .. } = instruction.kind else {
                continue;
            };
            if let Some(fact) = register_facts.get_mut(&dst) {
                let globals_proxy = matches!(
                    instruction.kind,
                    RegionInstructionKind::LoadLocal { local, .. }
                        if local_storage.get(&local) == Some(&LocalStorageClass::Globals)
                );
                if globals_proxy || borrowed_local_loads.contains(&instruction.continuation_id) {
                    fact.ownership = SsaOwnership::Borrowed;
                } else if fact.has_runtime_lifecycle() {
                    fact.ownership = SsaOwnership::Owned;
                }
            }
        }
    }
    // Borrow classification is intentionally decided after the main value
    // fixed point because it depends on complete use ranges. Propagate that
    // final ownership through instructions whose result is the same SSA
    // owner as an input. Without this pass, a borrowed local used as the
    // right-hand side of a property assignment became an `Owned` assignment
    // result, and the trailing expression `Discard` released the local's
    // frame owner even though the property store had only retained its own
    // independent owner.
    for _ in 0..iteration_limit {
        let previous_registers = register_facts.clone();
        for block in &region.blocks {
            for instruction in &block.instructions {
                let alias = match instruction.kind {
                    RegionInstructionKind::Move { dst, src }
                    | RegionInstructionKind::AssignLocalResult {
                        dst, value: src, ..
                    }
                    | RegionInstructionKind::AssignProperty {
                        dst, value: src, ..
                    } => Some((dst, src)),
                    _ => None,
                };
                if let Some((dst, src)) = alias {
                    register_facts.insert(
                        dst,
                        operand_fact(constants, &local_facts, &register_facts, src),
                    );
                }
            }
        }
        if register_facts == previous_registers {
            break;
        }
    }
    let reachability = block_reachability(region);
    let (moved_local_stores, mut elided_discards) =
        find_moved_local_stores(region, &local_storage, &register_facts, &reachability);
    let (moved_register_copies, moved_copy_discards) =
        find_moved_register_copies(region, &register_facts, &reachability);
    elided_discards.extend(moved_copy_discards);
    let (consumed_call_operands, call_operand_discards) =
        find_consumed_call_operands(region, &register_facts);
    elided_discards.extend(call_operand_discards);
    // Compiled call inputs are borrowed for the duration of the callee. Keep
    // an explicit boundary owner instead of moving an SSA owner into the
    // call: the caller can then release that boundary owner on every returned
    // status without needing a post-effect last-owner transition.
    let owned_entry_parameters = find_owned_entry_parameters(region, &local_storage);
    let mut frame_cleanup_locals =
        find_frame_cleanup_locals(region, &moved_local_stores, &local_storage);
    frame_cleanup_locals.extend(owned_entry_parameters.iter().copied());
    for local in &owned_entry_parameters {
        if let Some(fact) = local_facts.get_mut(local) {
            fact.ownership = SsaOwnership::Owned;
        }
    }

    ExecutableValueFlow {
        local_storage,
        local_facts,
        register_facts,
        borrowed_local_loads,
        reference_dimension_loads,
        moved_local_stores,
        moved_register_copies,
        consumed_call_operands,
        elided_discards,
        frame_cleanup_locals,
        owned_entry_parameters,
        ssa,
    }
}

/// Build the ownership facts required by the streaming baseline without
/// constructing the optimizing tier's whole-function SSA graph.
///
/// A promoted local owns its arena handle. A forward, same-block load can
/// borrow that handle until its last use when the local is not mutated in
/// between. Marking that result as borrowed makes both the load and a trailing
/// IR `Discard` exact no-ops for refcounting while preserving the local's
/// owner. Reference-backed, request-global, and suspension-backed locals keep
/// using the runtime ownership path.
#[must_use]
pub fn analyze_baseline_value_ownership(region: &RegionGraph) -> ExecutableValueFlow {
    let local_storage = classify_locals(region);
    let borrowed_local_loads = find_borrowed_local_loads(region, &local_storage);
    let reference_dimension_loads = find_reference_dimension_loads(region, &local_storage);
    // Native entry arguments are borrowed for the duration of the callee.
    // Recording that contract lets the return boundary retain only when a
    // result aliases one of those borrowed frame values.
    let mut local_facts = region
        .parameter_locals
        .iter()
        .copied()
        .filter(|local| {
            local_storage
                .get(local)
                .is_some_and(|storage| storage.is_native_frame_local())
        })
        .map(|local| {
            (
                local,
                SsaValueFact {
                    class: SsaValueClass::MixedHandle,
                    certainty: super::SsaCertainty::Unknown,
                    ownership: SsaOwnership::Borrowed,
                    integer_range: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut register_facts = BTreeMap::new();
    for block in &region.blocks {
        for instruction in &block.instructions {
            if let RegionInstructionKind::LoadLocal { dst, .. } = instruction.kind {
                register_facts.insert(
                    dst,
                    SsaValueFact {
                        class: SsaValueClass::MixedHandle,
                        certainty: super::SsaCertainty::Unknown,
                        ownership: if borrowed_local_loads.contains(&instruction.continuation_id) {
                            SsaOwnership::Borrowed
                        } else {
                            // A non-borrowed local fetch creates one explicit
                            // boundary owner even when baseline compilation
                            // does not know the PHP value class.
                            SsaOwnership::Owned
                        },
                        integer_range: None,
                    },
                );
                continue;
            }
            if let Some((register, fact)) =
                instruction_result_fact(&instruction.kind, &[], &local_facts, &register_facts)
                && fact.ownership == SsaOwnership::Owned
            {
                register_facts.insert(register, fact);
            }
        }
    }

    let reachability = block_reachability(region);
    let (moved_local_stores, moved_store_discards) =
        find_moved_local_stores(region, &local_storage, &register_facts, &reachability);
    let (consumed_call_operands, call_operand_discards) =
        find_consumed_call_operands(region, &register_facts);
    let mut elided_discards = moved_store_discards;
    elided_discards.extend(call_operand_discards);
    let owned_entry_parameters = find_owned_entry_parameters(region, &local_storage);
    let mut frame_cleanup_locals =
        find_frame_cleanup_locals(region, &moved_local_stores, &local_storage);
    frame_cleanup_locals.extend(owned_entry_parameters.iter().copied());

    // The streaming baseline does not build whole-function SSA, but every
    // non-parameter native-frame local that receives a value owns its current
    // slot at frame exit. This includes retained stores as well as last-use
    // moves. Restricting cleanup to optimizing move decisions leaked ordinary
    // `$local = new Object` values until request teardown.
    frame_cleanup_locals.extend(
        region
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction.kind {
                RegionInstructionKind::StoreLocal { local, .. }
                | RegionInstructionKind::AssignLocalResult { local, .. }
                    if !region.parameter_locals.contains(&local)
                        && local_storage
                            .get(&local)
                            .is_some_and(|storage| storage.is_native_frame_local()) =>
                {
                    Some(local)
                }
                _ => None,
            }),
    );
    for local in &frame_cleanup_locals {
        local_facts
            .entry(*local)
            .and_modify(|fact| fact.ownership = SsaOwnership::Owned)
            .or_insert(SsaValueFact {
                class: SsaValueClass::MixedHandle,
                certainty: super::SsaCertainty::Unknown,
                ownership: SsaOwnership::Owned,
                integer_range: None,
            });
    }

    ExecutableValueFlow {
        local_storage,
        local_facts,
        register_facts,
        borrowed_local_loads,
        reference_dimension_loads,
        moved_local_stores,
        consumed_call_operands,
        elided_discards,
        frame_cleanup_locals,
        owned_entry_parameters,
        ..ExecutableValueFlow::default()
    }
}

fn find_owned_entry_parameters(
    region: &RegionGraph,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
) -> BTreeSet<LocalId> {
    let by_value_parameters = region
        .params
        .iter()
        .filter_map(|parameter| (!parameter.by_ref).then_some(parameter.local))
        .collect::<BTreeSet<_>>();
    region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .flat_map(|instruction| instruction_mutated_locals(&instruction.kind))
        .filter(|local| {
            by_value_parameters.contains(local)
                && storage
                    .get(local)
                    .is_some_and(|class| class.is_native_frame_local())
        })
        .collect()
}

/// Finds native frame slots that retain an owner until the frame exits.
///
/// Passing a value or reference to a call does not transfer this owner. A
/// direct compiled call retains ordinary values and borrows prepared
/// references; a baseline continuation receives its own published state.
/// Removing a local merely because one of its loads reaches a call therefore
/// leaks the authoritative frame owner after an ordinary native return.
fn find_frame_cleanup_locals(
    region: &RegionGraph,
    moved_stores: &BTreeSet<u32>,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
) -> BTreeSet<LocalId> {
    region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .flat_map(|instruction| match instruction.kind {
            RegionInstructionKind::StoreLocal { local, .. }
                if moved_stores.contains(&instruction.continuation_id)
                    && storage.get(&local).is_some_and(|class| class.is_promoted()) =>
            {
                vec![local]
            }
            RegionInstructionKind::BindReference { target, source } => vec![target, source],
            RegionInstructionKind::BindReferenceDim { target, .. }
            | RegionInstructionKind::BindReferenceFromProperty { target, .. }
            | RegionInstructionKind::BindReferenceFromPropertyDim { target, .. } => vec![target],
            RegionInstructionKind::BindReferenceIntoDim { source, .. }
            | RegionInstructionKind::BindReferenceProperty { source, .. }
            | RegionInstructionKind::BindReferenceIntoPropertyDim { source, .. } => vec![source],
            RegionInstructionKind::BindReferenceDimFromProperty { array, .. } => vec![array],
            _ => Vec::new(),
        })
        .filter(|local| {
            storage
                .get(local)
                .is_some_and(|class| class.is_native_frame_local())
        })
        .collect()
}

fn find_moved_local_stores(
    region: &RegionGraph,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
    register_facts: &BTreeMap<RegId, SsaValueFact>,
    reachability: &[BTreeSet<usize>],
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
            if !storage.get(&local).is_some_and(|storage| {
                storage.is_promoted() || *storage == LocalStorageClass::MemoryReference
            }) || fact.ownership != SsaOwnership::Owned
                || terminator_uses.contains(&register)
            {
                continue;
            }
            let remaining = uses
                .get(&register)
                .into_iter()
                .flatten()
                .filter(|&&(use_block, use_index, _, _)| {
                    use_may_follow(
                        reachability,
                        block_index,
                        instruction_index,
                        use_block,
                        use_index,
                    )
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

fn find_moved_register_copies(
    region: &RegionGraph,
    register_facts: &BTreeMap<RegId, SsaValueFact>,
    reachability: &[BTreeSet<usize>],
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

    let mut moved = BTreeSet::new();
    let mut elided_discards = BTreeSet::new();
    for (block_index, block) in region.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            let RegionInstructionKind::Move {
                src: RegionOperand::Register(source),
                ..
            } = instruction.kind
            else {
                continue;
            };
            if register_facts.get(&source).is_none_or(|fact| {
                fact.ownership != SsaOwnership::Owned || !fact.has_runtime_lifecycle()
            }) || terminator_uses.contains(&source)
            {
                continue;
            }
            let remaining = uses
                .get(&source)
                .into_iter()
                .flatten()
                .filter(|&&(use_block, use_index, _, _)| {
                    use_may_follow(
                        reachability,
                        block_index,
                        instruction_index,
                        use_block,
                        use_index,
                    )
                })
                .copied()
                .collect::<Vec<_>>();
            match remaining.as_slice() {
                [] => {
                    moved.insert(instruction.continuation_id);
                }
                [(use_block, use_index, true, discard_continuation)]
                    if *use_block == block_index && *use_index > instruction_index =>
                {
                    moved.insert(instruction.continuation_id);
                    elided_discards.insert(*discard_continuation);
                }
                _ => {}
            }
        }
    }
    (moved, elided_discards)
}

/// Find call operands whose register owner ends at that call.
///
/// Native call ABIs borrow their packed operands while they execute and return
/// an independently owned result. Region IR, however, does not emit a trailing
/// `Discard` when the call itself is the last use of an expression. Release
/// exactly those last-use owners. A register that is read later must stay live,
/// and a repeated operand in one call still represents one owner.
fn find_consumed_call_operands(
    region: &RegionGraph,
    register_facts: &BTreeMap<RegId, SsaValueFact>,
) -> (BTreeSet<(u32, RegId)>, BTreeSet<u32>) {
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

    let mut consumed = BTreeSet::new();
    let mut elided_discards = BTreeSet::new();
    for (block_index, block) in region.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            let RegionInstructionKind::NativeCall(call) = &instruction.kind else {
                continue;
            };
            if !matches!(call.target, RegionCallTarget::Semantic { .. })
                && call.direct_compiled_target().is_none()
                && call.direct_compiled_unpack_target().is_none()
                && !native_call_uses_prepared_dynamic_callable(call)
            {
                continue;
            }
            let operand_registers = call
                .operands
                .iter()
                .flatten()
                .filter_map(|operand| match operand {
                    RegionOperand::Register(register) => Some(*register),
                    _ => None,
                })
                .collect::<BTreeSet<_>>();
            for register in operand_registers {
                if register_facts
                    .get(&register)
                    .is_none_or(|fact| fact.ownership != SsaOwnership::Owned)
                    || terminator_uses.contains(&register)
                {
                    continue;
                }
                let remaining = uses
                    .get(&register)
                    .into_iter()
                    .flatten()
                    .filter(|&&(use_block, use_index, _, _)| {
                        (use_block, use_index) != (block_index, instruction_index)
                    })
                    .copied()
                    .collect::<Vec<_>>();
                match remaining.as_slice() {
                    [] => {
                        consumed.insert((instruction.continuation_id, register));
                    }
                    [(use_block, use_index, true, discard_continuation)]
                        if *use_block == block_index && *use_index > instruction_index =>
                    {
                        consumed.insert((instruction.continuation_id, register));
                        elided_discards.insert(*discard_continuation);
                    }
                    _ => {}
                }
            }
        }
    }
    (consumed, elided_discards)
}

fn find_borrowed_local_loads(
    region: &RegionGraph,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
) -> BTreeSet<u32> {
    let mut uses = BTreeMap::<RegId, Vec<(usize, usize)>>::new();
    let mut terminator_uses = BTreeSet::new();
    let mut local_mutations = BTreeMap::<(usize, LocalId), Vec<usize>>::new();
    let mut borrow_barriers = BTreeMap::<usize, Vec<usize>>::new();
    for (block_index, block) in region.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            for register in instruction.register_uses() {
                uses.entry(register)
                    .or_default()
                    .push((block_index, instruction_index));
            }
            for local in instruction_mutated_locals(&instruction.kind) {
                local_mutations
                    .entry((block_index, local))
                    .or_default()
                    .push(instruction_index);
            }
            let borrow_barrier = match &instruction.kind {
                RegionInstructionKind::NativeCall(call) => {
                    !native_call_preserves_borrowed_arguments(call)
                }
                RegionInstructionKind::ArrayCallback(_)
                | RegionInstructionKind::PregCallbackArray(_) => true,
                RegionInstructionKind::NativeDynamicCode(_)
                | RegionInstructionKind::NativeSuspend(_) => true,
                _ => false,
            };
            if borrow_barrier {
                borrow_barriers
                    .entry(block_index)
                    .or_default()
                    .push(instruction_index);
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
            {
                continue;
            }
            if terminator_uses.contains(&dst) && uses.get(&dst).is_none_or(Vec::is_empty) {
                let retained_result = matches!(
                    block.terminator,
                    RegionTerminator::Return {
                        value: RegionOperand::Register(register),
                        ..
                    } if register == dst
                ) || matches!(
                    block.terminator,
                    RegionTerminator::Exit {
                        value: Some(RegionOperand::Register(register)),
                        ..
                    } if register == dst
                );
                if !retained_result {
                    continue;
                }
                let mutated_after_load =
                    local_mutations
                        .get(&(block_index, local))
                        .is_some_and(|positions| {
                            positions.iter().any(|position| *position > load_index)
                        });
                let crosses_barrier = borrow_barriers.get(&block_index).is_some_and(|positions| {
                    positions.iter().any(|position| *position > load_index)
                });
                if !mutated_after_load && !crosses_barrier {
                    // The local keeps its frame owner until cleanup. A direct
                    // return/exit of the loaded value therefore borrows that
                    // owner; terminator lowering retains the independent ABI
                    // result before frame cleanup releases the local.
                    borrowed.insert(instruction.continuation_id);
                }
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
            let mutation_between =
                local_mutations
                    .get(&(block_index, local))
                    .is_some_and(|positions| {
                        let after_load =
                            positions.partition_point(|position| *position <= load_index);
                        positions
                            .get(after_load)
                            .is_some_and(|position| *position < last_use)
                    });
            if mutation_between {
                continue;
            }
            let crosses_barrier = borrow_barriers.get(&block_index).is_some_and(|positions| {
                let after_load = positions.partition_point(|position| *position <= load_index);
                positions
                    .get(after_load)
                    .is_some_and(|position| *position <= last_use)
            });
            if crosses_barrier {
                continue;
            }
            borrowed.insert(instruction.continuation_id);
        }
    }
    borrowed
}

fn find_reference_dimension_loads(
    region: &RegionGraph,
    storage: &BTreeMap<LocalId, LocalStorageClass>,
) -> BTreeMap<u32, RegId> {
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

    let mut passthrough = BTreeMap::new();
    for (block_index, block) in region.blocks.iter().enumerate() {
        for (load_index, instruction) in block.instructions.iter().enumerate() {
            let RegionInstructionKind::LoadLocal { dst, local, .. } = instruction.kind else {
                continue;
            };
            if storage.get(&local) != Some(&LocalStorageClass::MemoryReference)
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
            let is_typed_reference_consumer = |use_index: usize| {
                matches!(
                    block.instructions[use_index].kind,
                    RegionInstructionKind::FetchDim {
                        array: RegionOperand::Register(array),
                        ..
                    } if array == dst
                )
            };
            if !register_uses
                .iter()
                .any(|&(_, use_index)| is_typed_reference_consumer(use_index))
                || register_uses.iter().any(|&(_, use_index)| {
                    !is_typed_reference_consumer(use_index)
                        && !matches!(
                            block.instructions[use_index].kind,
                            RegionInstructionKind::Discard {
                                src: RegionOperand::Register(discarded)
                            } if discarded == dst
                        )
                })
            {
                continue;
            }
            let last_use = register_uses
                .iter()
                .map(|&(_, use_index)| use_index)
                .max()
                .expect("non-empty reference dimension uses");
            if block.instructions[(load_index + 1)..last_use]
                .iter()
                .any(|instruction| instruction_mutated_locals(&instruction.kind).contains(&local))
            {
                continue;
            }
            passthrough.insert(instruction.continuation_id, dst);
        }
    }
    passthrough
}

/// A statically registered positional builtin borrows ordinary by-value
/// arguments for the duration of its synchronous adapter call. The caller's
/// promoted local remains the owning root, so retaining a second arena handle
/// before the call and releasing it afterwards is redundant. Calls whose
/// signature or binding shape is not fully known remain ownership barriers.
fn native_call_uses_prepared_dynamic_callable(call: &super::RegionNativeCall) -> bool {
    if !matches!(
        call.target,
        RegionCallTarget::Callable { .. }
            | RegionCallTarget::Closure { function: None, .. }
            | RegionCallTarget::Pipe { .. }
    ) || call.returns_by_reference
        || matches!(call.result, super::RegionCallResult::ReferenceLocal(_))
        || call.argument_operand_offset != 1
        || !call.operands.iter().all(Option::is_some)
    {
        return false;
    }
    let fixed = call.operands.len() == call.args.len().saturating_add(1)
        && call
            .args
            .iter()
            .all(|argument| argument.name.is_none() && !argument.unpack);
    let unpack = matches!(
        call.target,
        RegionCallTarget::Callable { .. } | RegionCallTarget::Closure { function: None, .. }
    ) && call.operands.len() == 2
        && call.trailing_unpack_argument() == Some(0);
    fixed || unpack
}

fn native_call_preserves_borrowed_arguments(call: &super::RegionNativeCall) -> bool {
    // A statically packed compiled call retains every ordinary operand before
    // entering the callee and borrows prepared reference cells from their
    // owning locals. A LoadLocal feeding such a call can therefore borrow its
    // frame-local owner: classifying that register as an expiring standalone
    // owner would add a last-owner validation and an unnecessary baseline
    // continuation before every direct method receiver.
    if call.direct_compiled_target().is_some() {
        return true;
    }
    if native_call_uses_prepared_dynamic_callable(call) {
        // The optimizing callable boundary admits only a published same-unit
        // by-value binding and retains every fixed or unpacked visible
        // argument across the compiled native call. Unsupported callable
        // shapes leave through one baseline continuation before invocation,
        // so a promoted local remains the authoritative owner on both paths.
        return true;
    }
    let RegionCallTarget::Function {
        name,
        function: None,
    } = &call.target
    else {
        return false;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    !normalized.contains('\\')
        && php_runtime::api::BuiltinRegistry::new()
            .get(&normalized)
            .is_some()
        && call
            .args
            .iter()
            .all(|argument| argument.name.is_none() && !argument.unpack)
        && call
            .args
            .iter()
            .enumerate()
            .all(|(index, _)| !call.builtin_argument_requires_reference(index))
}

fn instruction_mutated_locals(kind: &RegionInstructionKind) -> Vec<LocalId> {
    let mut locals = Vec::new();
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
        } => locals.push(*target),
        RegionInstructionKind::BindReference { target, source } => {
            locals.extend([*target, *source]);
        }
        RegionInstructionKind::BindReferenceDim { target, array, .. }
        | RegionInstructionKind::BindReferenceFromPropertyDim {
            target,
            object: RegionOperand::Local(array),
            ..
        } => locals.extend([*target, *array]),
        RegionInstructionKind::BindReferenceIntoDim { array, source, .. }
        | RegionInstructionKind::BindReferenceDimFromProperty {
            array,
            object: RegionOperand::Local(source),
            ..
        } => locals.extend([*array, *source]),
        RegionInstructionKind::BindReferenceProperty { source, .. }
        | RegionInstructionKind::BindReferenceIntoPropertyDim { source, .. } => {
            locals.push(*source);
        }
        RegionInstructionKind::BindReferenceFromProperty { target, .. }
        | RegionInstructionKind::BindReferenceFromPropertyDim { target, .. } => {
            locals.push(*target);
        }
        RegionInstructionKind::NativeCall(call) => {
            if let RegionCallResult::ReferenceLocal(target) = call.result {
                locals.push(target);
            }
            locals.extend(
                call.args
                    .iter()
                    .enumerate()
                    .filter_map(|(index, argument)| {
                        (call.argument_requires_reference_binding(index))
                            .then_some(argument.by_ref_local)
                            .flatten()
                    }),
            );
            locals.extend(
                call.args
                    .iter()
                    .enumerate()
                    .filter_map(|(index, argument)| {
                        call.argument_requires_reference_binding(index)
                            .then_some(argument.by_ref_dim.as_ref().map(|target| target.local))
                            .flatten()
                    }),
            );
        }
        RegionInstructionKind::ArrayCallback(call) => {
            locals.extend(call.mutable_local);
        }
        RegionInstructionKind::PregCallbackArray(call) => {
            locals.extend(call.count_local);
        }
        RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::MakeClosure {
            captures,
            ..
        }) => {
            locals.extend(
                captures
                    .iter()
                    .filter(|capture| capture.by_ref)
                    .map(|capture| capture.local),
            );
        }
        _ => {}
    }
    locals
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
    let mut request_globals = BTreeSet::new();
    let mut suspension = BTreeSet::new();
    for block in &region.blocks {
        for instruction in &block.instructions {
            match &instruction.kind {
                RegionInstructionKind::BindReference { target, source } => {
                    references.extend([*target, *source]);
                }
                RegionInstructionKind::BindReferenceDim { target, .. }
                | RegionInstructionKind::BindReferenceFromPropertyDim { target, .. } => {
                    references.insert(*target);
                }
                RegionInstructionKind::BindReferenceIntoDim { source, .. } => {
                    references.insert(*source);
                }
                RegionInstructionKind::BindReferenceProperty { source, .. }
                | RegionInstructionKind::BindReferenceIntoPropertyDim { source, .. } => {
                    references.insert(*source);
                }
                RegionInstructionKind::BindReferenceFromProperty { target, .. }
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
                    if let RegionCallTarget::Semantic {
                        operation: RegionSemanticOp::BindGlobal { local, .. },
                        ..
                    } = &call.target
                    {
                        request_globals.insert(*local);
                    }
                    if let RegionCallResult::ReferenceLocal(local) = call.result {
                        references.insert(local);
                    }
                    references.extend(call.args.iter().enumerate().filter_map(
                        |(index, argument)| {
                            call.argument_requires_reference_binding(index)
                                .then_some(argument.by_ref_local)
                                .flatten()
                        },
                    ));
                }
                RegionInstructionKind::NativeDynamicCode(
                    RegionNativeDynamicCode::MakeClosure { captures, .. },
                ) => {
                    references.extend(
                        captures
                            .iter()
                            .filter(|capture| capture.by_ref)
                            .map(|capture| capture.local),
                    );
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
            let compiler_generated = name.is_some_and(php_ir::is_compiler_generated_local_name);
            let storage = if name == Some("GLOBALS") {
                LocalStorageClass::Globals
            } else if name.is_some_and(|name| SUPERGLOBALS.contains(&name)) {
                LocalStorageClass::Superglobal
            } else if request_globals.contains(&local) {
                LocalStorageClass::RequestGlobal
            } else if region.flags.is_top_level && !compiler_generated {
                LocalStorageClass::RequestGlobal
            } else if references.contains(&local) {
                LocalStorageClass::MemoryReference
            } else if suspension.contains(&local) {
                LocalStorageClass::SuspensionPersistent
            } else if region.parameter_locals.contains(&local) {
                // By-value parameters are initialized by the native frame
                // binder before entry. Their last load need not keep the
                // local live afterwards, so post-instruction liveness cannot
                // be used as an initialization test.
                LocalStorageClass::SsaPlain
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
        return parameter.type_.as_ref().map_or(
            SsaValueFact {
                class: SsaValueClass::MixedHandle,
                certainty: super::SsaCertainty::Unknown,
                ownership: SsaOwnership::Borrowed,
                integer_range: None,
            },
            type_fact,
        );
    }
    if region.parameter_locals.contains(&local) {
        // The native entry ABI also prepends an implicit `$this` and closure
        // captures.  They are live borrowed handles at entry even though they
        // are not declared PHP parameters.  Treating them as uninitialized
        // lets optimizing ownership and type decisions discard a live
        // receiver or capture.
        return SsaValueFact {
            class: SsaValueClass::MixedHandle,
            certainty: super::SsaCertainty::Unknown,
            ownership: SsaOwnership::Borrowed,
            integer_range: None,
        };
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
        RegionOperand::I64(value) => {
            SsaValueFact::exact(SsaValueClass::Int, SsaOwnership::ImmortalConstant)
                .with_integer_range(SsaIntegerRange::exact(value))
        }
        RegionOperand::Constant(index) => constants
            .get(index as usize)
            .map_or_else(|| reserved_constant_fact(index), constant_fact),
        RegionOperand::LinkedConstant { class, .. } => {
            SsaValueFact::exact(class, SsaOwnership::Borrowed)
        }
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
        }
        | RegionInstructionKind::AssignProperty {
            dst, value: src, ..
        } => Some((*dst, fact(*src))),
        RegionInstructionKind::LoadLocal { dst, local, quiet } => {
            let mut fact = locals.get(local).copied().unwrap_or(SsaValueFact::UNKNOWN);
            if *quiet && fact.class == SsaValueClass::Uninitialized {
                fact = SsaValueFact::exact(SsaValueClass::Null, SsaOwnership::ImmortalConstant);
            }
            Some((*dst, fact))
        }
        RegionInstructionKind::Binary { dst, op, lhs, rhs } => {
            let lhs = fact(*lhs);
            let rhs = fact(*rhs);
            // Every binary producer publishes one independent result owner.
            // Its runtime class can remain dynamic (for example integer
            // overflow becoming a float), but its ownership cannot: no
            // binary operation aliases either input.  Leaving these results
            // at SsaOwnership::Unknown prevents last-use moves and keeps the
            // superseded generic lifecycle path alive.
            let owned_dynamic = SsaValueFact {
                class: SsaValueClass::MixedHandle,
                certainty: super::SsaCertainty::Unknown,
                ownership: SsaOwnership::Owned,
                integer_range: None,
            };
            let both_arrays =
                lhs.class == SsaValueClass::ArrayHandle && rhs.class == SsaValueClass::ArrayHandle;
            let both_integer = lhs.class == SsaValueClass::Int && rhs.class == SsaValueClass::Int;
            let both_numeric = matches!(lhs.class, SsaValueClass::Int | SsaValueClass::Float)
                && matches!(rhs.class, SsaValueClass::Int | SsaValueClass::Float);
            let has_float = lhs.class == SsaValueClass::Float || rhs.class == SsaValueClass::Float;
            let output = if both_arrays && *op == RegionBinaryOp::Add {
                SsaValueFact::known(SsaValueClass::ArrayHandle, SsaOwnership::Owned)
            } else if both_numeric {
                match op {
                    RegionBinaryOp::Mod
                    | RegionBinaryOp::BitAnd
                    | RegionBinaryOp::BitOr
                    | RegionBinaryOp::BitXor
                    | RegionBinaryOp::ShiftLeft
                    | RegionBinaryOp::ShiftRight
                        if both_integer =>
                    {
                        SsaValueFact::known(SsaValueClass::Int, SsaOwnership::Owned)
                    }
                    RegionBinaryOp::Add | RegionBinaryOp::Sub | RegionBinaryOp::Mul
                        if has_float =>
                    {
                        SsaValueFact::known(SsaValueClass::Float, SsaOwnership::Owned)
                    }
                    RegionBinaryOp::Add | RegionBinaryOp::Sub | RegionBinaryOp::Mul
                        if both_integer =>
                    {
                        let range = match (*op, lhs.integer_range, rhs.integer_range) {
                            (RegionBinaryOp::Add, Some(lhs), Some(rhs)) => lhs.checked_add(rhs),
                            (RegionBinaryOp::Sub, Some(lhs), Some(rhs)) => lhs.checked_sub(rhs),
                            (RegionBinaryOp::Mul, Some(lhs), Some(rhs)) => lhs.checked_mul(rhs),
                            _ => None,
                        };
                        range.map_or(owned_dynamic, |range| {
                            SsaValueFact::known(SsaValueClass::Int, SsaOwnership::Owned)
                                .with_integer_range(range)
                        })
                    }
                    RegionBinaryOp::Div if has_float => {
                        SsaValueFact::known(SsaValueClass::Float, SsaOwnership::Owned)
                    }
                    RegionBinaryOp::Div => owned_dynamic,
                    RegionBinaryOp::Concat => {
                        SsaValueFact::known(SsaValueClass::StringHandle, SsaOwnership::Owned)
                    }
                    RegionBinaryOp::Pow
                    | RegionBinaryOp::Mod
                    | RegionBinaryOp::BitAnd
                    | RegionBinaryOp::BitOr
                    | RegionBinaryOp::BitXor
                    | RegionBinaryOp::ShiftLeft
                    | RegionBinaryOp::ShiftRight
                    | RegionBinaryOp::Add
                    | RegionBinaryOp::Sub
                    | RegionBinaryOp::Mul => owned_dynamic,
                }
            } else {
                owned_dynamic
            };
            Some((*dst, output))
        }
        RegionInstructionKind::Unary { dst, op, src } => {
            let input = fact(*src);
            let output = match op {
                RegionUnaryOp::Not => SsaValueFact::known(SsaValueClass::Bool, SsaOwnership::Owned),
                RegionUnaryOp::Plus | RegionUnaryOp::BitNot
                    if input.class == SsaValueClass::Int =>
                {
                    SsaValueFact::known(SsaValueClass::Int, SsaOwnership::Owned)
                }
                RegionUnaryOp::Plus | RegionUnaryOp::Minus
                    if input.class == SsaValueClass::Float =>
                {
                    SsaValueFact::known(SsaValueClass::Float, SsaOwnership::Owned)
                }
                RegionUnaryOp::BitNot if input.class == SsaValueClass::StringHandle => {
                    SsaValueFact::known(SsaValueClass::StringHandle, SsaOwnership::Owned)
                }
                RegionUnaryOp::BitNot if input.class == SsaValueClass::Float => {
                    SsaValueFact::known(SsaValueClass::Int, SsaOwnership::Owned)
                }
                RegionUnaryOp::Minus if input.class == SsaValueClass::Int => SsaValueFact::UNKNOWN,
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
            SsaValueFact::known(SsaValueClass::Int, SsaOwnership::Owned).with_integer_range(
                SsaIntegerRange {
                    minimum: -1,
                    maximum: 1,
                },
            ),
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
                    RegionCallTarget::Function { name, .. } => {
                        fixed_function_result_fact(name).unwrap_or(SsaValueFact::UNKNOWN)
                    }
                    RegionCallTarget::Method { .. }
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
        RegionInstructionKind::ArrayCallback(call) => Some((
            call.result,
            match call.operation {
                super::RegionArrayCallbackOperation::Map
                | super::RegionArrayCallbackOperation::FilterValue
                | super::RegionArrayCallbackOperation::FilterKey
                | super::RegionArrayCallbackOperation::FilterValueAndKey => {
                    SsaValueFact::known(SsaValueClass::ArrayHandle, SsaOwnership::Owned)
                }
                super::RegionArrayCallbackOperation::All
                | super::RegionArrayCallbackOperation::Any
                | super::RegionArrayCallbackOperation::Usort
                | super::RegionArrayCallbackOperation::Uasort
                | super::RegionArrayCallbackOperation::Uksort
                | super::RegionArrayCallbackOperation::Walk
                | super::RegionArrayCallbackOperation::WalkRecursive => {
                    SsaValueFact::known(SsaValueClass::Bool, SsaOwnership::Owned)
                }
                super::RegionArrayCallbackOperation::PregReplace => {
                    SsaValueFact::known(SsaValueClass::StringHandle, SsaOwnership::Owned)
                }
                super::RegionArrayCallbackOperation::Reduce
                | super::RegionArrayCallbackOperation::Find
                | super::RegionArrayCallbackOperation::FindKey => SsaValueFact::UNKNOWN,
            },
        )),
        RegionInstructionKind::PregCallbackArray(call) => Some((
            call.result,
            SsaValueFact::known(SsaValueClass::StringHandle, SsaOwnership::Owned),
        )),
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
    let fact = SsaValueFact::exact(class, SsaOwnership::ImmortalConstant);
    match constant {
        IrConstant::Int(value) => fact.with_integer_range(SsaIntegerRange::exact(*value)),
        _ => fact,
    }
}

fn fixed_function_result_fact(name: &str) -> Option<SsaValueFact> {
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    let name = normalized.to_ascii_lowercase();
    let integer = |minimum, maximum| {
        SsaValueFact::known(SsaValueClass::Int, SsaOwnership::Owned)
            .with_integer_range(SsaIntegerRange { minimum, maximum })
    };
    match name.as_str() {
        // The byte-compare ABIs return a C-compatible signed comparison
        // result. Keeping the complete i32 range remains conservative while
        // proving that short reduction trees cannot overflow a PHP integer.
        "strcmp" | "strcasecmp" | "strncmp" | "strncasecmp" | "strnatcmp" | "strnatcasecmp"
        | "substr_compare" => Some(integer(i64::from(i32::MIN), i64::from(i32::MAX))),
        "ord" => Some(integer(0, 255)),
        "strlen" | "mb_strlen" | "count" | "sizeof" => Some(integer(0, i64::MAX)),
        _ => None,
    }
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
    let mut joined = SsaValueFact::known(
        left.class,
        if left.ownership == right.ownership {
            left.ownership
        } else {
            SsaOwnership::Unknown
        },
    );
    joined.integer_range = match (left.integer_range, right.integer_range) {
        (Some(left), Some(right)) => Some(left.union(right)),
        _ => None,
    };
    joined
}

#[cfg(test)]
mod tests {
    use php_ir::instruction::{IrCallArg, IrCallArgValueKind, IrCallDimTarget};
    use php_ir::{
        BinaryOp, FunctionFlags, InstructionKind, IrBuilder, IrParam, IrReturnType, IrSpan,
        Operand, UnitId,
    };

    use super::*;
    use crate::region_ir::{RegionNativeCall, build_baseline_region};

    #[test]
    fn prepared_dynamic_callable_consumes_an_owned_argument_at_its_last_use() {
        let mut builder = IrBuilder::new(UnitId::new(4_239));
        let file = builder.add_file("dynamic-call-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("dynamic_call_owner", FunctionFlags::default(), span);
        let callable = builder.intern_local(function, "callback");
        builder.push_param(
            function,
            IrParam {
                name: "callback".to_owned(),
                local: callable,
                required: true,
                default: None,
                type_: Some(IrReturnType::Callable),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let block = builder.append_block(function);
        let source = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: source },
            span,
        );
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallCallable {
                dst: result,
                callee: Operand::Local(callable),
                args: vec![IrCallArg {
                    name: None,
                    value: Operand::Register(source),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);
        let call = region.blocks[0]
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.kind,
                    RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Callable { .. },
                        ..
                    })
                )
            })
            .expect("prepared dynamic callable");
        assert!(flow.consumes_call_operand(call.continuation_id, source));
        flow.verify_ownership(&region)
            .expect("last-use dynamic-call ownership should verify");
    }

    #[test]
    fn prepared_runtime_closure_consumes_an_owned_argument_at_its_last_use() {
        let mut builder = IrBuilder::new(UnitId::new(4_236));
        let file = builder.add_file("runtime-closure-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let function =
            builder.start_function("runtime_closure_owner", FunctionFlags::default(), span);
        let closure = builder.intern_local(function, "closure");
        builder.push_param(
            function,
            IrParam {
                name: "closure".to_owned(),
                local: closure,
                required: true,
                default: None,
                type_: Some(IrReturnType::Callable),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let block = builder.append_block(function);
        let source = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: source },
            span,
        );
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallClosure {
                dst: result,
                callee: Operand::Local(closure),
                args: vec![IrCallArg {
                    name: None,
                    value: Operand::Register(source),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);
        let call = region.blocks[0]
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.kind,
                    RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Closure { function: None, .. },
                        ..
                    })
                )
            })
            .expect("prepared runtime closure");
        assert!(flow.consumes_call_operand(call.continuation_id, source));
        flow.verify_ownership(&region)
            .expect("last-use runtime-closure ownership should verify");
    }

    #[test]
    fn prepared_pipe_consumes_an_owned_input_at_its_last_use() {
        let mut builder = IrBuilder::new(UnitId::new(4_237));
        let file = builder.add_file("pipe-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("pipe_owner", FunctionFlags::default(), span);
        let callable = builder.intern_local(function, "callback");
        builder.push_param(
            function,
            IrParam {
                name: "callback".to_owned(),
                local: callable,
                required: true,
                default: None,
                type_: Some(IrReturnType::Callable),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let block = builder.append_block(function);
        let source = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: source },
            span,
        );
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::Pipe {
                dst: result,
                input: Operand::Register(source),
                callable: Operand::Local(callable),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);
        let call = region.blocks[0]
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.kind,
                    RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Pipe { .. },
                        ..
                    })
                )
            })
            .expect("prepared pipe");
        assert!(flow.consumes_call_operand(call.continuation_id, source));
        flow.verify_ownership(&region)
            .expect("last-use pipe ownership should verify");
    }

    #[test]
    fn direct_compiled_unpack_consumes_its_owned_source_array() {
        let mut builder = IrBuilder::new(UnitId::new(4_238));
        let file = builder.add_file("direct-unpack-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let callee = builder.start_function("unpack_target", FunctionFlags::default(), span);
        let callee_block = builder.append_block(callee);
        builder.terminate_return(callee, callee_block, None, span);
        builder.register_function_name("unpack_target", callee);

        let caller = builder.start_function("direct_unpack_owner", FunctionFlags::default(), span);
        let block = builder.append_block(caller);
        let source = builder.alloc_register(caller);
        builder.emit(
            caller,
            block,
            InstructionKind::NewArray { dst: source },
            span,
        );
        let result = builder.alloc_register(caller);
        builder.emit(
            caller,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "unpack_target".to_owned(),
                args: vec![IrCallArg {
                    name: None,
                    value: Operand::Register(source),
                    unpack: true,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(caller, block, Some(Operand::Register(result)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, caller).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);
        let call = region.blocks[0]
            .instructions
            .iter()
            .find_map(|instruction| match &instruction.kind {
                RegionInstructionKind::NativeCall(call)
                    if call.direct_compiled_unpack_target() == Some(callee) =>
                {
                    Some(instruction)
                }
                _ => None,
            })
            .expect("direct compiled unpack call");
        assert!(flow.consumes_call_operand(call.continuation_id, source));
        flow.verify_ownership(&region)
            .expect("last-use direct-unpack ownership should verify");
    }

    #[test]
    fn semantic_call_consumes_only_a_registers_final_owner_use() {
        let mut builder = IrBuilder::new(UnitId::new(4_240));
        let file = builder.add_file("semantic-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("semantic_owner", FunctionFlags::default(), span);
        let block = builder.append_block(function);
        let value = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: value },
            span,
        );
        let assigned = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::AssignStaticProperty {
                dst: assigned,
                class_name: "Holder".to_owned(),
                property: "slot".to_owned(),
                value: Operand::Register(value),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(assigned),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let semantic = region.blocks[0]
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.kind,
                    RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Semantic { .. },
                        ..
                    })
                )
            })
            .expect("static assignment semantic call");

        let baseline = analyze_baseline_value_ownership(&region);
        assert!(baseline.consumes_call_operand(semantic.continuation_id, value));
    }

    #[test]
    fn semantic_call_preserves_a_register_with_a_later_use() {
        let mut builder = IrBuilder::new(UnitId::new(4_241));
        let file = builder.add_file("semantic-live-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let function =
            builder.start_function("semantic_live_owner", FunctionFlags::default(), span);
        let block = builder.append_block(function);
        let value = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: value },
            span,
        );
        let assigned = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::AssignStaticProperty {
                dst: assigned,
                class_name: "Holder".to_owned(),
                property: "slot".to_owned(),
                value: Operand::Register(value),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Echo {
                src: Operand::Register(value),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(value),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(assigned),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let semantic = region.blocks[0]
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.kind,
                    RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Semantic { .. },
                        ..
                    })
                )
            })
            .expect("static assignment semantic call");

        let baseline = analyze_baseline_value_ownership(&region);
        assert!(!baseline.consumes_call_operand(semantic.continuation_id, value));
    }

    #[test]
    fn baseline_releases_a_non_parameter_local_owner_at_frame_exit() {
        let mut builder = IrBuilder::new(UnitId::new(4_242));
        let file = builder.add_file("baseline-frame-local-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let function =
            builder.start_function("baseline_frame_local_owner", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        let block = builder.append_block(function);
        let value = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: value },
            span,
        );
        let store = builder.emit(
            function,
            block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(value),
            },
            span,
        );
        let discard = builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(value),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let store_continuation = region.blocks[0].instructions[store.index()].continuation_id;
        let discard_continuation = region.blocks[0].instructions[discard.index()].continuation_id;

        let baseline = analyze_baseline_value_ownership(&region);
        assert!(baseline.moves_value_into_local(store_continuation));
        assert!(baseline.elides_discard(discard_continuation));
        assert!(baseline.releases_local_at_frame_exit(local));
        assert_eq!(baseline.local_fact(local).ownership, SsaOwnership::Owned);
    }

    #[test]
    fn prepared_reference_argument_keeps_its_frame_cleanup_owner() {
        let mut builder = IrBuilder::new(UnitId::new(4_246));
        let file = builder.add_file("reference-call-frame-owner.php");
        let span = IrSpan::new(file, 0, 1);

        let callee = builder.start_function("borrow_reference", FunctionFlags::default(), span);
        let callee_local = builder.intern_local(callee, "value");
        builder.push_param(
            callee,
            IrParam {
                name: "value".to_owned(),
                local: callee_local,
                required: true,
                default: None,
                type_: None,
                by_ref: true,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let callee_block = builder.append_block(callee);
        builder.terminate_return(callee, callee_block, None, span);
        builder.register_function_name("borrow_reference", callee);

        let caller =
            builder.start_function("reference_call_frame_owner", FunctionFlags::default(), span);
        let array = builder.intern_local(caller, "array");
        builder.push_param(
            caller,
            IrParam {
                name: "array".to_owned(),
                local: array,
                required: true,
                default: None,
                type_: Some(IrReturnType::Array),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let block = builder.append_block(caller);
        let zero = builder.intern_constant(IrConstant::Int(0));
        let argument = builder.alloc_register(caller);
        builder.emit(
            caller,
            block,
            InstructionKind::FetchDim {
                dst: argument,
                array: Operand::Local(array),
                key: Operand::Constant(zero),
                quiet: false,
                mode: php_ir::instruction::DimFetchMode::Lvalue,
            },
            span,
        );
        let result = builder.alloc_register(caller);
        builder.emit(
            caller,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "borrow_reference".to_owned(),
                args: vec![IrCallArg {
                    name: None,
                    value: Operand::Register(argument),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: Some(IrCallDimTarget {
                        local: array,
                        dims: vec![Operand::Constant(zero)],
                    }),
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(caller, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, caller).expect("reference call region");
        let reference_local = region.blocks[0]
            .instructions
            .iter()
            .find_map(|instruction| match instruction.kind {
                RegionInstructionKind::BindReferenceDim { target, .. } => Some(target),
                _ => None,
            })
            .expect("prepared reference local");
        let call = region.blocks[0]
            .instructions
            .iter()
            .find_map(|instruction| match &instruction.kind {
                RegionInstructionKind::NativeCall(call) => Some(call),
                _ => None,
            })
            .expect("prepared native call");
        assert_eq!(call.args[0].by_ref_local, Some(reference_local));

        for flow in [
            analyze_executable_value_flow(&region, &unit.constants),
            analyze_baseline_value_ownership(&region),
        ] {
            assert!(
                flow.releases_local_at_frame_exit(reference_local),
                "the call borrows the prepared reference; its frame owner must be released"
            );
            assert_eq!(
                flow.local_storage(reference_local),
                LocalStorageClass::MemoryReference
            );
        }
    }

    #[test]
    fn final_store_moves_owner_after_prior_dominating_use() {
        let mut builder = IrBuilder::new(UnitId::new(4_243));
        let file = builder.add_file("prior-use-owner-move.php");
        let span = IrSpan::new(file, 0, 1);
        let function =
            builder.start_function("prior_use_owner_move", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        let producer = builder.append_block(function);
        let consumer = builder.append_block(function);
        let value = builder.alloc_register(function);
        builder.emit(
            function,
            producer,
            InstructionKind::NewArray { dst: value },
            span,
        );
        builder.emit(
            function,
            producer,
            InstructionKind::Echo {
                src: Operand::Register(value),
            },
            span,
        );
        builder.terminate_jump(function, producer, consumer, span);
        let store = builder.emit(
            function,
            consumer,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(value),
            },
            span,
        );
        let discard = builder.emit(
            function,
            consumer,
            InstructionKind::Discard {
                src: Operand::Register(value),
            },
            span,
        );
        builder.terminate_return(function, consumer, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let store_continuation =
            region.blocks[consumer.index()].instructions[store.index()].continuation_id;
        let discard_continuation =
            region.blocks[consumer.index()].instructions[discard.index()].continuation_id;

        for flow in [
            analyze_baseline_value_ownership(&region),
            analyze_executable_value_flow(&region, &unit.constants),
        ] {
            assert!(flow.moves_value_into_local(store_continuation));
            assert!(flow.elides_discard(discard_continuation));
            flow.verify_ownership(&region)
                .expect("prior uses must not invalidate a final ownership move");
        }
    }

    #[test]
    fn returned_local_load_borrows_until_frame_cleanup() {
        let mut builder = IrBuilder::new(UnitId::new(4_244));
        let file = builder.add_file("returned-frame-local-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let function =
            builder.start_function("returned_frame_local_owner", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        let block = builder.append_block(function);
        let value = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: value },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(value),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(value),
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

        for flow in [
            analyze_baseline_value_ownership(&region),
            analyze_executable_value_flow(&region, &unit.constants),
        ] {
            assert!(flow.releases_local_at_frame_exit(local));
            assert_eq!(flow.register_fact(loaded).ownership, SsaOwnership::Borrowed);
        }
    }

    #[test]
    fn property_assignment_result_preserves_borrowed_input_ownership() {
        let mut builder = IrBuilder::new(UnitId::new(4_245));
        let file = builder.add_file("borrowed-property-assignment.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function(
            "borrowed_property_assignment",
            FunctionFlags::default(),
            span,
        );
        let object_local = builder.intern_local(function, "object");
        builder.push_param(
            function,
            IrParam {
                name: "object".to_owned(),
                local: object_local,
                required: true,
                default: None,
                type_: Some(IrReturnType::Mixed),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let value_local = builder.intern_local(function, "value");
        let block = builder.append_block(function);
        let value = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: value },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::StoreLocal {
                local: value_local,
                src: Operand::Register(value),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(value),
            },
            span,
        );
        let object = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: object,
                local: object_local,
            },
            span,
        );
        let borrowed = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: borrowed,
                local: value_local,
            },
            span,
        );
        let assigned = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::AssignProperty {
                dst: assigned,
                object: Operand::Register(object),
                property: "output".to_owned(),
                value: Operand::Register(borrowed),
            },
            span,
        );
        let discarded = builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(assigned),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);

        assert_eq!(
            flow.register_fact(borrowed).ownership,
            SsaOwnership::Borrowed
        );
        assert_eq!(
            flow.register_fact(assigned).ownership,
            SsaOwnership::Borrowed
        );
        assert!(
            !flow.elides_discard(region.blocks[0].instructions[discarded.index()].continuation_id)
        );
        assert!(!crate::region_ir::value_release_required(
            flow.register_fact(assigned)
        ));
        flow.verify_ownership(&region)
            .expect("borrowed property assignment result should verify");
    }

    #[test]
    fn mutable_by_value_parameter_owns_its_native_frame_slot() {
        let mut builder = IrBuilder::new(UnitId::new(4_243));
        let file = builder.add_file("mutable-parameter-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let function =
            builder.start_function("mutable_parameter_owner", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        builder.push_param(
            function,
            IrParam {
                name: "value".to_owned(),
                local,
                required: true,
                default: None,
                type_: Some(IrReturnType::Mixed),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let block = builder.append_block(function);
        let replacement = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: replacement },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(replacement),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");

        for flow in [
            analyze_executable_value_flow(&region, &unit.constants),
            analyze_baseline_value_ownership(&region),
        ] {
            assert!(flow.owns_parameter_at_entry(local));
            assert!(flow.releases_local_at_frame_exit(local));
            assert_eq!(flow.local_fact(local).ownership, SsaOwnership::Owned);
        }
    }

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
    fn keeps_compiler_generated_top_level_reference_in_native_frame() {
        let mut builder = IrBuilder::new(UnitId::new(4_200));
        let file = builder.add_file("top-level-compiler-local.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function(
            "{main}",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            span,
        );
        let visible = builder.intern_local(function, "visible");
        let compiler = builder.intern_local(function, "__phrust:by-ref-static-property:1");
        let block = builder.append_block(function);
        builder.emit(
            function,
            block,
            InstructionKind::BindReference {
                target: compiler,
                source: visible,
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);

        assert_eq!(
            flow.local_storage(compiler),
            LocalStorageClass::MemoryReference
        );
        assert_eq!(
            flow.local_storage(visible),
            LocalStorageClass::RequestGlobal
        );
    }

    #[test]
    fn classifies_function_global_binding_as_request_global_storage() {
        let mut builder = IrBuilder::new(UnitId::new(4_203));
        let file = builder.add_file("function-global-reference.php");
        let span = IrSpan::new(file, 0, 1);
        let function =
            builder.start_function("function_global_reference", FunctionFlags::default(), span);
        let source = builder.intern_local(function, "source");
        let global = builder.intern_local(function, "shared");
        let block = builder.append_block(function);
        builder.emit(
            function,
            block,
            InstructionKind::BindGlobal {
                local: global,
                name: "shared".to_owned(),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::BindReference {
                target: global,
                source,
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);

        assert_eq!(flow.local_storage(global), LocalStorageClass::RequestGlobal);
        assert_eq!(
            flow.local_storage(source),
            LocalStorageClass::MemoryReference
        );
    }

    #[test]
    fn keeps_static_local_contents_unknown_after_visible_store() {
        let mut builder = IrBuilder::new(UnitId::new(4_202));
        let file = builder.add_file("static-flow.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("flow", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        let block = builder.append_block(function);
        let null = builder.intern_constant(IrConstant::Null);
        let set = builder.intern_constant(IrConstant::String("set".into()));
        builder.emit(
            function,
            block,
            InstructionKind::InitStaticLocal {
                local,
                name: "value".to_owned(),
                default: Operand::Constant(null),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Constant(set),
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

        assert_eq!(
            flow.local_storage(local),
            LocalStorageClass::MemoryReference
        );
        assert_eq!(flow.local_fact(local), SsaValueFact::UNKNOWN);
        assert_eq!(
            flow.register_fact(loaded),
            SsaValueFact {
                class: SsaValueClass::MixedHandle,
                certainty: crate::region_ir::SsaCertainty::Unknown,
                ownership: SsaOwnership::Owned,
                integer_range: None,
            }
        );
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
    fn baseline_borrow_does_not_cross_native_call_boundary() {
        let mut builder = IrBuilder::new(UnitId::new(4_205));
        let file = builder.add_file("baseline-call-borrow.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("borrow_call", FunctionFlags::default(), span);
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
        let loaded = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal { dst: loaded, local },
            span,
        );
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "consume".to_owned(),
                args: vec![IrCallArg {
                    name: None,
                    value: Operand::Register(loaded),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");

        let baseline = analyze_baseline_value_ownership(&region);
        assert!(!baseline.can_borrow_local_load(region.blocks[0].instructions[0].continuation_id));
        assert_eq!(
            baseline.register_fact(loaded).ownership,
            SsaOwnership::Owned
        );
    }

    #[test]
    fn by_reference_call_location_uses_reference_capable_local_storage() {
        let mut builder = IrBuilder::new(UnitId::new(4_209));
        let file = builder.add_file("speculative-call-reference.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("caller", FunctionFlags::default(), span);
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
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "cross_unit_target".to_owned(),
                args: vec![IrCallArg {
                    name: None,
                    value: Operand::Local(local),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: Some(local),
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);

        assert_eq!(
            flow.local_storage(local),
            LocalStorageClass::MemoryReference,
            "a by-reference call can publish a cell into the caller local and therefore cannot use SSA-plain storage"
        );
    }

    #[test]
    fn baseline_borrows_local_through_known_by_value_builtin() {
        let mut builder = IrBuilder::new(UnitId::new(4_206));
        let file = builder.add_file("baseline-builtin-borrow.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("borrow_builtin", FunctionFlags::default(), span);
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
        let loaded = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal { dst: loaded, local },
            span,
        );
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "strlen".to_owned(),
                args: vec![IrCallArg {
                    name: None,
                    value: Operand::Register(loaded),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");

        let baseline = analyze_baseline_value_ownership(&region);
        assert!(baseline.can_borrow_local_load(region.blocks[0].instructions[0].continuation_id));
        assert_eq!(
            baseline.register_fact(loaded).ownership,
            SsaOwnership::Borrowed
        );
        baseline
            .verify_ownership(&region)
            .expect("known by-value builtin borrow should verify");
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

    #[test]
    fn final_ssa_copy_transfers_owned_handle_without_retain_or_discard() {
        let mut builder = IrBuilder::new(UnitId::new(4_212));
        let file = builder.add_file("ssa-register-move.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("register_move", FunctionFlags::default(), span);
        let block = builder.append_block(function);
        let source = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::NewArray { dst: source },
            span,
        );
        let destination = builder.alloc_register(function);
        let moved = builder.emit(
            function,
            block,
            InstructionKind::Move {
                dst: destination,
                src: Operand::Register(source),
            },
            span,
        );
        let discarded = builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(source),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(destination)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);

        assert!(flow.moves_value_into_register(
            region.blocks[0].instructions[moved.index()].continuation_id
        ));
        assert!(
            flow.elides_discard(region.blocks[0].instructions[discarded.index()].continuation_id)
        );
        flow.verify_ownership(&region)
            .expect("last-use register move should verify");
    }

    #[test]
    fn dynamic_binary_result_is_one_owned_value_and_moves_at_last_use() {
        let mut builder = IrBuilder::new(UnitId::new(4_214));
        let file = builder.add_file("binary-result-owner.php");
        let span = IrSpan::new(file, 0, 1);
        let function =
            builder.start_function("binary_result_owner", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "result");
        let block = builder.append_block(function);
        let left = builder.intern_constant(IrConstant::Int(i64::MAX));
        let right = builder.intern_constant(IrConstant::Int(1));
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: result,
                op: BinaryOp::Add,
                lhs: Operand::Constant(left),
                rhs: Operand::Constant(right),
            },
            span,
        );
        let stored = builder.emit(
            function,
            block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(result),
            },
            span,
        );
        let discarded = builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(result),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);

        let result_fact = flow.register_fact(result);
        assert_eq!(result_fact.class, SsaValueClass::MixedHandle);
        assert_eq!(result_fact.certainty, super::super::SsaCertainty::Unknown);
        assert_eq!(result_fact.ownership, SsaOwnership::Owned);
        assert!(
            flow.moves_value_into_local(
                region.blocks[0].instructions[stored.index()].continuation_id
            )
        );
        assert!(
            flow.elides_discard(region.blocks[0].instructions[discarded.index()].continuation_id)
        );
        flow.verify_ownership(&region)
            .expect("binary last-use owner move should verify");
    }

    #[test]
    fn direct_call_consumes_its_final_argument_owner() {
        let mut builder = IrBuilder::new(UnitId::new(4_213));
        let file = builder.add_file("ssa-call-move.php");
        let span = IrSpan::new(file, 0, 1);
        let callee = builder.start_function("consume_array", FunctionFlags::default(), span);
        let callee_local = builder.intern_local(callee, "value");
        builder.push_param(
            callee,
            IrParam {
                name: "value".to_owned(),
                local: callee_local,
                required: true,
                default: None,
                type_: Some(IrReturnType::Array),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let callee_block = builder.append_block(callee);
        builder.terminate_return(callee, callee_block, None, span);
        builder.register_function_name("consume_array", callee);

        let caller = builder.start_function("call_move", FunctionFlags::default(), span);
        let block = builder.append_block(caller);
        let source = builder.alloc_register(caller);
        builder.emit(
            caller,
            block,
            InstructionKind::NewArray { dst: source },
            span,
        );
        let result = builder.alloc_register(caller);
        builder.emit(
            caller,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "consume_array".to_owned(),
                args: vec![IrCallArg {
                    name: None,
                    value: Operand::Register(source),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        let discarded = builder.emit(
            caller,
            block,
            InstructionKind::Discard {
                src: Operand::Register(source),
            },
            span,
        );
        builder.terminate_return(caller, block, Some(Operand::Register(result)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, caller).expect("region");
        let flow = analyze_executable_value_flow(&region, &unit.constants);
        let call = region.blocks[0]
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.kind, RegionInstructionKind::NativeCall(_)))
            .expect("native direct call");
        assert!(flow.consumes_call_operand(call.continuation_id, source));
        assert!(
            flow.elides_discard(region.blocks[0].instructions[discarded.index()].continuation_id)
        );
        flow.verify_ownership(&region)
            .expect("last-use direct-call ownership should verify");
    }
}
