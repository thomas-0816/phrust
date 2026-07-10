//! Conservative last-use move planning for dense bytecode (Runtime lever R3).
//!
//! Array/string writes deep-copy their contents when the value looks shared
//! (`Rc::make_mut` sees a refcount above one) because a prior register read left
//! a transient clone alive in a register. This analysis proves, for a narrow and
//! provably-safe subset of register reads, that the read is the register's last
//! use, so the dense executor can *move* the value out of the register instead
//! of cloning it. Moving consumes the transient `Rc`, so a later write sees the
//! sole owner and mutates in place.
//!
//! The same proof also drives an *array-read register release* (see
//! [`LastUseMovePlan::is_array_release_eligible`]). A dimension fetch such as
//! `$map["b"]` first loads `$map` into a register (an `Rc` handle clone), then
//! reads the element. That register clone lingers after the fetch, so a
//! following in-place write to the same local (`$map["c"] = …`) sees a shared
//! array and copy-on-write-separates the whole contents. When the fetch is that
//! register's block-local last use, the executor drops the transient handle
//! right after the (always owned) element value is extracted, returning the
//! owning local to sole ownership so its next write mutates in place. The fetch
//! result is a value copy (references are dereferenced and cloned), never an
//! alias into the array, and only *shared* handles are released — dropping one
//! merely decrements the refcount, freeing no contents and running no
//! destructors — so behavior stays byte-identical.
//!
//! The whole feature is default-off: [`crate::vm::VmOptions::last_use_moves`]
//! gates both building this plan and consulting it. With the flag off the plan
//! is never built and the executor's read path is byte-identical to today.
//!
//! # Safety of the marked subset
//!
//! A register read is marked move-eligible only when *all* of these hold:
//!
//! 1. The register appears (as a def or use) in exactly one dense basic block —
//!    it is *block-local*. A block-local register is not live-out of any block
//!    other than its own, because it appears in no other block.
//! 2. Within that block the register is *defined before its first use* (its
//!    first textual occurrence is a write). Dense blocks are straight-line, so
//!    textual order equals execution order. This guarantees the register is not
//!    live-in to the block, hence not live-out across a self-loop back-edge
//!    either: every re-entry redefines it before reading it.
//! 3. The read being marked is the *textually-last* use of the register in the
//!    block. Combined with (1) and (2) the register is provably dead afterward.
//! 4. That last-use instruction reads the register *exactly once* across all of
//!    its operands (moving one operand must not strand a second read).
//! 5. The last-use instruction is one of the small set of value-consuming
//!    opcodes the executor actually converts (its single movable source
//!    operand), *or* it is a dimension fetch (`FetchDim`/`LoadConstFetchDim`)
//!    whose source array is that register — the array-read release site.
//!
//! Every other register read stays a clone (current behavior). Reads feeding
//! by-reference sends, closures/captures, foreach iterators, or any raw-register
//! opcode are never the marked movable operand, and they are still counted as
//! uses so they correctly veto marking an earlier read.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::bytecode::{
    DenseCallArg, DenseFunction, DenseInstruction, DenseOpcode, DenseOperand, DenseOperandKind,
    DenseOperands,
};

/// Sentinel block id used before an instruction's owning block is known.
const NO_BLOCK: u32 = u32::MAX;

/// Reasons a candidate register read is left cloning instead of moved. Stable
/// strings so counters/reports can attribute why coverage was not taken.
mod reason {
    pub(super) const MULTI_BLOCK: &str = "multi_block";
    pub(super) const USED_BEFORE_DEF: &str = "used_before_def";
    pub(super) const NO_DEF: &str = "no_def";
    pub(super) const MULTIPLE_READS_IN_LAST_USE: &str = "multiple_reads_in_last_use";
    pub(super) const UNMOVABLE_LAST_USE_SITE: &str = "unmovable_last_use_site";
}

/// Per-register accounting gathered in a single forward pass over a function's
/// dense instructions.
#[derive(Default)]
struct RegisterFacts {
    /// The single block the register was first seen in, or `None` before that.
    block: Option<u32>,
    /// True once the register is seen in more than one block.
    multi_block: bool,
    /// First instruction index that writes the register.
    first_def: Option<usize>,
    /// First instruction index that reads the register.
    first_use: Option<usize>,
    /// Last instruction index that reads the register.
    last_use: Option<usize>,
}

/// Move-eligibility plan for one dense function. Cheap to consult during
/// execution: a hash-set membership test keyed by the dense instruction index
/// and register index.
#[derive(Clone, Debug, Default)]
pub struct LastUseMovePlan {
    /// Packed `(instruction_index << 32) | register_index` keys marking the
    /// single movable source register read of an instruction.
    eligible: HashSet<u64>,
    /// Packed keys marking a dimension fetch's source-array register read whose
    /// block-local last use this is, so the executor may drop the transient
    /// array handle after extracting the (owned) element value.
    array_release_eligible: HashSet<u64>,
    /// Count of register reads marked move-eligible.
    eligible_reads: u64,
    /// Count of array-read source registers marked release-eligible.
    array_release_reads: u64,
    /// Candidate registers rejected, grouped by stable reason.
    ineligible_by_reason: BTreeMap<&'static str, u64>,
    /// Registers that are written but never read anywhere in the function
    /// (per the same exhaustive def/use classifier the move proof rests on).
    /// A value-producing opcode whose destination is such a register may skip
    /// the register store entirely and move its value into its other consumer
    /// — e.g. `$a[$k] = $v;` as a statement moves `$v` into the array slot
    /// instead of cloning it for a result register nothing ever reads.
    dead_writes: HashSet<u32>,
}

impl LastUseMovePlan {
    /// Packs an instruction index and register index into a single key.
    #[must_use]
    const fn key(instruction_index: u32, register: u32) -> u64 {
        ((instruction_index as u64) << 32) | (register as u64)
    }

    /// Returns whether the register read at `instruction_index` for `register`
    /// is a provably-safe last use that may be moved instead of cloned.
    #[must_use]
    pub fn is_move_eligible(&self, instruction_index: u32, register: u32) -> bool {
        self.eligible
            .contains(&Self::key(instruction_index, register))
    }

    /// Number of register reads marked move-eligible.
    #[must_use]
    pub fn eligible_reads(&self) -> u64 {
        self.eligible_reads
    }

    /// Returns whether the source-array register read at `instruction_index`
    /// for `register` is a provably-safe last use of a dimension fetch whose
    /// transient array handle may be dropped after the element value is read.
    #[must_use]
    pub fn is_array_release_eligible(&self, instruction_index: u32, register: u32) -> bool {
        self.array_release_eligible
            .contains(&Self::key(instruction_index, register))
    }

    /// Number of array-read source registers marked release-eligible.
    #[must_use]
    pub fn array_release_reads(&self) -> u64 {
        self.array_release_reads
    }

    /// Returns whether `register` is written but never read in this function,
    /// so a value-producing opcode may skip its result-register store.
    #[must_use]
    pub fn is_dead_write(&self, register: u32) -> bool {
        self.dead_writes.contains(&register)
    }

    /// Number of never-read written registers.
    #[must_use]
    pub fn dead_write_registers(&self) -> u64 {
        self.dead_writes.len() as u64
    }

    /// Candidate registers left cloning, grouped by stable reason.
    #[must_use]
    pub fn ineligible_by_reason(&self) -> &BTreeMap<&'static str, u64> {
        &self.ineligible_by_reason
    }

    /// True when no reads were marked (nothing to move or release).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.eligible.is_empty() && self.array_release_eligible.is_empty()
    }

    fn record_ineligible(&mut self, reason: &'static str) {
        *self.ineligible_by_reason.entry(reason).or_default() += 1;
    }

    /// Builds the conservative last-use move plan for one dense function.
    #[must_use]
    pub fn analyze(function: &DenseFunction) -> Self {
        let mut plan = Self::default();
        let instruction_count = function.instructions.len();
        if instruction_count == 0 {
            return plan;
        }

        let instruction_block = instruction_block_map(function, instruction_count);

        // Forward pass: gather per-register block membership and def/use extents.
        let mut facts: HashMap<u32, RegisterFacts> = HashMap::new();
        let mut defs = Vec::new();
        let mut uses = Vec::new();
        for (index, instruction) in function.instructions.iter().enumerate() {
            defs.clear();
            uses.clear();
            collect_defs_uses(&instruction.operands, &mut defs, &mut uses);
            let block = instruction_block[index];
            for &register in &defs {
                let entry = facts.entry(register).or_default();
                note_block(entry, block);
                entry.first_def.get_or_insert(index);
            }
            for &register in &uses {
                let entry = facts.entry(register).or_default();
                note_block(entry, block);
                entry.first_use.get_or_insert(index);
                entry.last_use = Some(index);
            }
        }

        // Marking pass: only registers that clear every guard are eligible.
        for (&register, facts) in &facts {
            let Some(last_use) = facts.last_use else {
                // Never read anywhere: nothing to move, and any store into the
                // register is dead — its producer may keep the value for the
                // other consumer instead of cloning.
                if facts.first_def.is_some() {
                    plan.dead_writes.insert(register);
                }
                continue;
            };
            let Some(first_use) = facts.first_use else {
                continue;
            };
            if facts.multi_block {
                plan.record_ineligible(reason::MULTI_BLOCK);
                continue;
            }
            let Some(first_def) = facts.first_def else {
                plan.record_ineligible(reason::NO_DEF);
                continue;
            };
            if first_def >= first_use {
                // Read at or before the first write: the register may be
                // live-in, so a self-loop back-edge could observe it. Reject.
                plan.record_ineligible(reason::USED_BEFORE_DEF);
                continue;
            }
            let instruction = &function.instructions[last_use];
            if count_register_reads(&instruction.operands, register) != 1 {
                plan.record_ineligible(reason::MULTIPLE_READS_IN_LAST_USE);
                continue;
            }
            // A register's provably-dead last use is either a value-consuming
            // move site or a dimension fetch's source-array register. These
            // opcode sets are disjoint, so at most one branch matches.
            let is_value_move = movable_operand(instruction) == Some(register);
            let is_array_release = releasable_array_operand(instruction) == Some(register);
            if !is_value_move && !is_array_release {
                plan.record_ineligible(reason::UNMOVABLE_LAST_USE_SITE);
                #[cfg(debug_assertions)]
                if std::env::var_os("PHRUST_LAST_USE_DEBUG").is_some() {
                    eprintln!("[last-use-unmovable] {:?}", instruction.opcode);
                }
                continue;
            }
            let Ok(instruction_index) = u32::try_from(last_use) else {
                plan.record_ineligible(reason::UNMOVABLE_LAST_USE_SITE);
                continue;
            };
            if is_value_move {
                plan.eligible.insert(Self::key(instruction_index, register));
                plan.eligible_reads += 1;
            } else {
                plan.array_release_eligible
                    .insert(Self::key(instruction_index, register));
                plan.array_release_reads += 1;
            }
        }

        #[cfg(debug_assertions)]
        plan.debug_assert_safe(function, &instruction_block);

        plan
    }

    /// Independent cross-check (debug builds only): every marked read must be
    /// the sole occurrence in its block-local register's block and have no later
    /// use anywhere. This re-derives the invariants from scratch so an
    /// enumeration bug in `analyze` cannot silently produce an unsafe move.
    #[cfg(debug_assertions)]
    fn debug_assert_safe(&self, function: &DenseFunction, instruction_block: &[u32]) {
        let mut defs = Vec::new();
        let mut uses = Vec::new();
        // The move set and the array-release set carry identical liveness
        // invariants; re-derive both from scratch.
        for &packed in self.eligible.iter().chain(&self.array_release_eligible) {
            let instruction_index = (packed >> 32) as usize;
            let register = (packed & 0xffff_ffff) as u32;
            let marked_block = instruction_block[instruction_index];
            let mut saw_def_before_use = false;
            let mut first_use_seen = false;
            for (index, instruction) in function.instructions.iter().enumerate() {
                defs.clear();
                uses.clear();
                collect_defs_uses(&instruction.operands, &mut defs, &mut uses);
                let reads = uses.iter().filter(|&&r| r == register).count();
                let writes = defs.iter().filter(|&&r| r == register).count();
                if reads == 0 && writes == 0 {
                    continue;
                }
                // Block-local: the register appears only in the marked block.
                debug_assert_eq!(
                    instruction_block[index], marked_block,
                    "moved register r{register} appears in another block"
                );
                if !first_use_seen && writes > 0 && reads == 0 {
                    saw_def_before_use = true;
                }
                if reads > 0 {
                    first_use_seen = true;
                }
                // No read after the marked instruction, and exactly one read at it.
                if index > instruction_index {
                    debug_assert_eq!(
                        reads, 0,
                        "moved register r{register} is read after its marked last use"
                    );
                } else if index == instruction_index {
                    debug_assert_eq!(
                        reads, 1,
                        "moved register r{register} is not read exactly once at its last use"
                    );
                }
            }
            debug_assert!(
                saw_def_before_use,
                "moved register r{register} is not defined before its first use"
            );
        }
        // Dead-write registers must have no read anywhere in the function —
        // re-derive with the same classifier.
        for &register in &self.dead_writes {
            for instruction in &function.instructions {
                defs.clear();
                uses.clear();
                collect_defs_uses(&instruction.operands, &mut defs, &mut uses);
                debug_assert!(
                    !uses.contains(&register),
                    "dead-write register r{register} is read somewhere"
                );
            }
        }
    }
}

/// Maps each instruction index to the id of the dense block that contains it.
/// Blocks own contiguous, non-overlapping instruction ranges; descriptor order
/// in the block table does not matter here.
fn instruction_block_map(function: &DenseFunction, instruction_count: usize) -> Vec<u32> {
    let mut instruction_block = vec![NO_BLOCK; instruction_count];
    for block in &function.blocks {
        let start = block.first_instruction as usize;
        let end = start.saturating_add(block.instruction_len as usize);
        for slot in instruction_block
            .iter_mut()
            .take(end.min(instruction_count))
            .skip(start.min(instruction_count))
        {
            *slot = block.id;
        }
    }
    instruction_block
}

/// Records a block appearance, flagging the register as multi-block when it is
/// seen in a second distinct block.
fn note_block(facts: &mut RegisterFacts, block: u32) {
    match facts.block {
        None => facts.block = Some(block),
        Some(seen) if seen != block => facts.multi_block = true,
        Some(_) => {}
    }
}

/// Returns the register the dense executor will move for `instruction`, when its
/// opcode is one of the converted value-consuming sites and its movable source
/// operand is a register. Returns `None` for every other opcode/operand shape,
/// so no read outside the converted set is ever marked.
fn movable_operand(instruction: &DenseInstruction) -> Option<u32> {
    match (instruction.opcode, &instruction.operands) {
        (DenseOpcode::Move, DenseOperands::RegOperand { src, .. }) => register_index(*src),
        // Plain register-to-local store: the value is written into the local,
        // so a provably-dead source register moves instead of cloning. The
        // discard variant is not planned here — its executor arm always takes
        // (it unsets the register immediately after the store anyway).
        (DenseOpcode::StoreLocal, DenseOperands::LocalOperand { src, .. }) => register_index(*src),
        (DenseOpcode::Cast, DenseOperands::Cast { src, .. }) => register_index(*src),
        (
            DenseOpcode::AssignDim | DenseOpcode::AppendDim,
            DenseOperands::AssignDim { value, .. },
        ) => register_index(*value),
        (
            DenseOpcode::ArrayInsert,
            DenseOperands::ArrayInsert {
                value,
                by_ref_local: None,
                ..
            },
        ) => register_index(*value),
        _ => None,
    }
}

/// Register holding the source array of a dimension fetch whose element result
/// is always an owned value copy (never an alias into the array). Releasing the
/// register at its last use lets the array's owning local reclaim sole
/// ownership so a following in-place write skips a copy-on-write separation.
///
/// Only the value-read fetches qualify: their handlers extract the element
/// through `effective_value`, which dereferences PHP references and clones, so
/// the fetch result never borrows the array storage. Returns `None` for every
/// other opcode and for non-register array operands (a local array operand is
/// read in place and must stay live). By-reference element binding uses
/// `BindReferenceDim`, not these opcodes, so an aliasing fetch is never marked.
fn releasable_array_operand(instruction: &DenseInstruction) -> Option<u32> {
    match (instruction.opcode, &instruction.operands) {
        (DenseOpcode::FetchDim, DenseOperands::FetchDim { array, .. }) => register_index(*array),
        (DenseOpcode::LoadConstFetchDim, DenseOperands::LoadConstFetchDim { array, .. }) => {
            register_index(*array)
        }
        _ => None,
    }
}

/// Register index of a register operand, or `None` for locals/constants.
fn register_index(operand: DenseOperand) -> Option<u32> {
    (operand.kind == DenseOperandKind::Register).then_some(operand.index)
}

/// Counts how many times `register` is read as a source operand in `operands`.
fn count_register_reads(operands: &DenseOperands, register: u32) -> usize {
    let mut defs = Vec::new();
    let mut uses = Vec::new();
    collect_defs_uses(operands, &mut defs, &mut uses);
    uses.iter().filter(|&&r| r == register).count()
}

/// Pushes the register index of an operand into `uses` when it is a register.
fn push_use(uses: &mut Vec<u32>, operand: DenseOperand) {
    if operand.kind == DenseOperandKind::Register {
        uses.push(operand.index);
    }
}

/// Collects every register read (`uses`) and register write (`defs`) of a dense
/// instruction. Exhaustive over [`DenseOperands`] with no wildcard arm, so a new
/// operand variant fails to compile until it is classified here — the analysis
/// can never silently miss a register use and mark an unsafe move. Raw `u32`
/// register fields are classified against the IR field types (register vs local
/// slot); local slots and name/block indices are intentionally excluded.
///
/// Shared with [`crate::deopt`] so the report-only initialized-liveness analysis
/// enumerates register definitions through this same exhaustive classification
/// instead of a second, drift-prone copy.
pub(crate) fn collect_defs_uses(
    operands: &DenseOperands,
    defs: &mut Vec<u32>,
    uses: &mut Vec<u32>,
) {
    match operands {
        DenseOperands::None => {}
        DenseOperands::RegConst { dst, .. } => defs.push(*dst),
        DenseOperands::RegOperand { dst, src } => {
            defs.push(*dst);
            push_use(uses, *src);
        }
        DenseOperands::LocalOperand { src, .. } => push_use(uses, *src),
        DenseOperands::Local { .. } => {}
        DenseOperands::StaticLocal { default, .. } => push_use(uses, *default),
        DenseOperands::LocalName { .. } => {}
        DenseOperands::RegName { dst, .. } => defs.push(*dst),
        DenseOperands::Cast { dst, src, .. } => {
            defs.push(*dst);
            push_use(uses, *src);
        }
        DenseOperands::Binary { dst, lhs, rhs } => {
            defs.push(*dst);
            push_use(uses, *lhs);
            push_use(uses, *rhs);
        }
        DenseOperands::Call { dst, args, .. } => {
            defs.push(*dst);
            collect_call_arg_uses(uses, args);
        }
        DenseOperands::NewObject { dst, args, .. } => {
            defs.push(*dst);
            collect_call_arg_uses(uses, args);
        }
        DenseOperands::CallableCall { dst, callee, args } => {
            defs.push(*dst);
            push_use(uses, *callee);
            collect_call_arg_uses(uses, args);
        }
        DenseOperands::ResolveCallable { dst, .. } => defs.push(*dst),
        DenseOperands::LoadConstPair {
            first_dst,
            second_dst,
            ..
        } => {
            defs.push(*first_dst);
            defs.push(*second_dst);
        }
        DenseOperands::LoadConstArrayInsert {
            value_dst,
            array,
            key,
            ..
        } => {
            defs.push(*value_dst);
            // `array` is a register holding the array, mutated in place.
            uses.push(*array);
            defs.push(*array);
            if let Some(key) = key {
                push_use(uses, *key);
            }
        }
        DenseOperands::Pipe {
            dst,
            input,
            callable,
        } => {
            defs.push(*dst);
            push_use(uses, *input);
            push_use(uses, *callable);
        }
        DenseOperands::MakeClosure { dst, captures, .. } => {
            defs.push(*dst);
            for capture in captures {
                push_use(uses, capture.src);
            }
        }
        DenseOperands::MethodCall {
            dst, object, args, ..
        } => {
            defs.push(*dst);
            push_use(uses, *object);
            collect_call_arg_uses(uses, args);
        }
        DenseOperands::StaticCall { dst, args, .. } => {
            defs.push(*dst);
            collect_call_arg_uses(uses, args);
        }
        DenseOperands::Dst { dst } => defs.push(*dst),
        DenseOperands::ArrayInsert {
            array, key, value, ..
        } => {
            // `array` is a register holding the array, mutated in place.
            uses.push(*array);
            defs.push(*array);
            if let Some(key) = key {
                push_use(uses, *key);
            }
            push_use(uses, *value);
        }
        DenseOperands::FetchDim {
            dst, array, key, ..
        } => {
            defs.push(*dst);
            push_use(uses, *array);
            push_use(uses, *key);
        }
        DenseOperands::LoadConstFetchDim {
            key_dst,
            dst,
            array,
            ..
        } => {
            defs.push(*key_dst);
            defs.push(*dst);
            push_use(uses, *array);
        }
        DenseOperands::LoadLocalLoadConst {
            first_dst,
            local,
            second_dst,
            ..
        } => {
            defs.push(*first_dst);
            defs.push(*second_dst);
            push_use(uses, *local);
        }
        DenseOperands::AssignDim {
            dst, dims, value, ..
        } => {
            defs.push(*dst);
            for dim in dims {
                push_use(uses, *dim);
            }
            push_use(uses, *value);
        }
        DenseOperands::BindReferenceDim { dims, .. } => {
            // `local` and `source` are local slots, not registers.
            for dim in dims {
                push_use(uses, *dim);
            }
        }
        DenseOperands::InstanceOf { dst, object, .. } => {
            defs.push(*dst);
            push_use(uses, *object);
        }
        DenseOperands::AssignPropertyDim {
            dst,
            object,
            dims,
            value,
            ..
        } => {
            defs.push(*dst);
            push_use(uses, *object);
            for dim in dims {
                push_use(uses, *dim);
            }
            push_use(uses, *value);
        }
        DenseOperands::UnsetPropertyDim { object, dims, .. } => {
            push_use(uses, *object);
            for dim in dims {
                push_use(uses, *dim);
            }
        }
        DenseOperands::AssignStaticProperty { dst, value, .. } => {
            defs.push(*dst);
            push_use(uses, *value);
        }
        DenseOperands::AssignDynamicProperty {
            dst,
            object,
            property,
            value,
        } => {
            defs.push(*dst);
            push_use(uses, *object);
            push_use(uses, *property);
            push_use(uses, *value);
        }
        DenseOperands::PropertyDimProbe {
            dst, object, dims, ..
        } => {
            defs.push(*dst);
            push_use(uses, *object);
            for dim in dims {
                push_use(uses, *dim);
            }
        }
        DenseOperands::IssetDim { dst, dims, .. } => {
            defs.push(*dst);
            for dim in dims {
                push_use(uses, *dim);
            }
        }
        DenseOperands::EmptyDim { dst, dims, .. } => {
            defs.push(*dst);
            for dim in dims {
                push_use(uses, *dim);
            }
        }
        DenseOperands::UnsetDim { dims, .. } => {
            for dim in dims {
                push_use(uses, *dim);
            }
        }
        DenseOperands::ForeachInit { iterator, source } => {
            defs.push(*iterator);
            push_use(uses, *source);
        }
        DenseOperands::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            defs.push(*has_value);
            uses.push(*iterator);
            defs.push(*iterator);
            if let Some(key) = key {
                defs.push(*key);
            }
            defs.push(*value);
        }
        DenseOperands::ForeachCleanup { iterator } => uses.push(*iterator),
        DenseOperands::FetchProperty { dst, object, .. } => {
            defs.push(*dst);
            push_use(uses, *object);
        }
        DenseOperands::AssignProperty {
            dst, object, value, ..
        } => {
            defs.push(*dst);
            push_use(uses, *object);
            push_use(uses, *value);
        }
        DenseOperands::Operand { src } => push_use(uses, *src),
        DenseOperands::Jump { .. } => {}
        DenseOperands::JumpIf { condition, .. } => push_use(uses, *condition),
        DenseOperands::JumpIfElse { condition, .. } => push_use(uses, *condition),
        DenseOperands::Return { value } => {
            if let Some(value) = value {
                push_use(uses, *value);
            }
        }
        DenseOperands::Exit { value } => {
            if let Some(value) = value {
                push_use(uses, *value);
            }
        }
        DenseOperands::Include { dst, path, .. } => {
            defs.push(*dst);
            push_use(uses, *path);
        }
        DenseOperands::DeclareFunction { .. } => {}
        DenseOperands::DeclareClass { .. } => {}
        DenseOperands::FetchClassConstant { dst, .. } => defs.push(*dst),
    }
}

/// Collects register reads from call arguments, including the operands of any
/// by-reference dimension/property targets. Caller `local` slots (`by_ref_local`
/// and dim-target `local`) are local slots, not registers.
fn collect_call_arg_uses(uses: &mut Vec<u32>, args: &[DenseCallArg]) {
    for arg in args {
        push_use(uses, arg.value);
        if let Some(target) = &arg.by_ref_dim {
            for dim in &target.dims {
                push_use(uses, *dim);
            }
        }
        if let Some(target) = &arg.by_ref_property {
            push_use(uses, target.object);
        }
        if let Some(target) = &arg.by_ref_property_dim {
            push_use(uses, target.object);
            for dim in &target.dims {
                push_use(uses, *dim);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{DenseBlock, DenseFunction, DenseInstruction, DenseSpanId};

    fn reg(index: u32) -> DenseOperand {
        DenseOperand {
            kind: DenseOperandKind::Register,
            index,
        }
    }

    fn instr(opcode: DenseOpcode, operands: DenseOperands) -> DenseInstruction {
        DenseInstruction {
            opcode,
            operands,
            span: DenseSpanId::new(0),
            cache_slot: None,
        }
    }

    fn load_const(dst: u32) -> DenseInstruction {
        instr(
            DenseOpcode::LoadConst,
            DenseOperands::RegConst { dst, constant: 0 },
        )
    }

    fn move_reg(dst: u32, src: u32) -> DenseInstruction {
        instr(
            DenseOpcode::Move,
            DenseOperands::RegOperand { dst, src: reg(src) },
        )
    }

    fn fetch_dim(dst: u32, array: u32, key: u32) -> DenseInstruction {
        instr(
            DenseOpcode::FetchDim,
            DenseOperands::FetchDim {
                dst,
                array: reg(array),
                key: reg(key),
                quiet: false,
            },
        )
    }

    fn ret() -> DenseInstruction {
        instr(DenseOpcode::Return, DenseOperands::Return { value: None })
    }

    fn one_block(len: u32) -> Vec<DenseBlock> {
        vec![DenseBlock {
            id: 0,
            first_instruction: 0,
            instruction_len: len,
            terminator: len.saturating_sub(1),
        }]
    }

    fn function(instructions: Vec<DenseInstruction>, blocks: Vec<DenseBlock>) -> DenseFunction {
        DenseFunction {
            name: "t".to_owned(),
            register_count: 16,
            local_count: 4,
            blocks,
            instructions,
        }
    }

    #[test]
    fn marks_block_local_last_use_at_movable_site() {
        // r1 defined by LoadConst, moved once at its block-local last use.
        let plan = LastUseMovePlan::analyze(&function(
            vec![load_const(1), move_reg(2, 1), ret()],
            one_block(3),
        ));
        assert!(plan.is_move_eligible(1, 1));
        assert_eq!(plan.eligible_reads(), 1);
    }

    #[test]
    fn rejects_multi_block_register() {
        // r1 defined in block 0, used in block 1: not block-local.
        let plan = LastUseMovePlan::analyze(&function(
            vec![
                load_const(1),
                instr(DenseOpcode::Jump, DenseOperands::Jump { target: 1 }),
                move_reg(2, 1),
                ret(),
            ],
            vec![
                DenseBlock {
                    id: 0,
                    first_instruction: 0,
                    instruction_len: 2,
                    terminator: 1,
                },
                DenseBlock {
                    id: 1,
                    first_instruction: 2,
                    instruction_len: 2,
                    terminator: 3,
                },
            ],
        ));
        assert!(!plan.is_move_eligible(2, 1));
        assert!(plan.is_empty());
        assert_eq!(
            plan.ineligible_by_reason().get(reason::MULTI_BLOCK),
            Some(&1)
        );
    }

    #[test]
    fn rejects_register_read_twice_in_last_use_instruction() {
        // AppendDim reads r1 as both a dimension and the value: moving one read
        // would strand the other.
        let append = instr(
            DenseOpcode::AppendDim,
            DenseOperands::AssignDim {
                dst: 2,
                local: 0,
                dims: vec![reg(1)],
                value: reg(1),
            },
        );
        let plan =
            LastUseMovePlan::analyze(&function(vec![load_const(1), append, ret()], one_block(3)));
        assert!(!plan.is_move_eligible(1, 1));
        assert_eq!(
            plan.ineligible_by_reason()
                .get(reason::MULTIPLE_READS_IN_LAST_USE),
            Some(&1)
        );
    }

    #[test]
    fn rejects_use_before_def() {
        // r1 is read before it is written: it may be live-in, so not movable.
        let plan = LastUseMovePlan::analyze(&function(
            vec![move_reg(2, 1), load_const(1), ret()],
            one_block(3),
        ));
        assert!(!plan.is_move_eligible(0, 1));
        assert_eq!(
            plan.ineligible_by_reason().get(reason::USED_BEFORE_DEF),
            Some(&1)
        );
    }

    #[test]
    fn marks_block_local_last_use_array_read_of_fetch_dim() {
        // r1 (source array) and r2 (key) are defined then read once by the
        // fetch. r1 is the releasable source-array operand; r2 is not.
        let plan = LastUseMovePlan::analyze(&function(
            vec![load_const(1), load_const(2), fetch_dim(3, 1, 2), ret()],
            one_block(4),
        ));
        assert!(
            plan.is_array_release_eligible(2, 1),
            "source array releases"
        );
        assert!(!plan.is_move_eligible(2, 1), "release is not a value move");
        assert_eq!(plan.array_release_reads(), 1);
        assert!(
            !plan.is_array_release_eligible(2, 2),
            "the key register is not a releasable array operand"
        );
    }

    #[test]
    fn rejects_array_read_release_when_register_read_again() {
        // r1 is the source array of two fetches; only its textually-last read
        // may release, never the earlier one.
        let plan = LastUseMovePlan::analyze(&function(
            vec![
                load_const(1),
                load_const(2),
                fetch_dim(3, 1, 2),
                fetch_dim(4, 1, 2),
                ret(),
            ],
            one_block(5),
        ));
        assert!(
            !plan.is_array_release_eligible(2, 1),
            "an earlier array read must never be released"
        );
        assert!(
            plan.is_array_release_eligible(3, 1),
            "the last array read releases"
        );
        assert_eq!(plan.array_release_reads(), 1);
    }

    #[test]
    fn rejects_multi_block_array_read_release() {
        // r1 defined in block 0, read as a fetch source array in block 1: not
        // block-local, so it may be live across the edge and is never released.
        let plan = LastUseMovePlan::analyze(&function(
            vec![
                load_const(1),
                load_const(2),
                instr(DenseOpcode::Jump, DenseOperands::Jump { target: 1 }),
                fetch_dim(3, 1, 2),
                ret(),
            ],
            vec![
                DenseBlock {
                    id: 0,
                    first_instruction: 0,
                    instruction_len: 3,
                    terminator: 2,
                },
                DenseBlock {
                    id: 1,
                    first_instruction: 3,
                    instruction_len: 2,
                    terminator: 4,
                },
            ],
        ));
        assert!(!plan.is_array_release_eligible(3, 1));
        assert_eq!(plan.array_release_reads(), 0);
        assert_eq!(
            plan.ineligible_by_reason().get(reason::MULTI_BLOCK),
            Some(&2)
        );
    }

    #[test]
    fn marks_only_the_textually_last_read() {
        // r1 is read at two movable sites; only the last one is move-eligible.
        let plan = LastUseMovePlan::analyze(&function(
            vec![load_const(1), move_reg(2, 1), move_reg(3, 1), ret()],
            one_block(4),
        ));
        assert!(!plan.is_move_eligible(1, 1), "first read must stay a clone");
        assert!(plan.is_move_eligible(2, 1), "last read is move-eligible");
        assert_eq!(plan.eligible_reads(), 1);
    }
}
