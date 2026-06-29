//! Dense VM bytecode design skeleton.
//!
//! This module owns a compact execution-format representation for the VM. It
//! lowers from verified rich IR, but it does not replace the existing
//! interpreter path yet.

use std::collections::BTreeMap;

use php_ir::ids::{LocalId, RegId};
use php_ir::instruction::UnaryOp;
use php_ir::instruction::{IrCallArg, IrCallArgValueKind, Terminator, TerminatorKind};
use php_ir::rule_selection::{RuleKind, RuleSelection, RuleSelectionReport};
use php_ir::source_map::{IrSourceMapTarget, IrSpan};
use php_ir::{BinaryOp, CompareOp, Instruction, InstructionKind, IrFunction, IrUnit, Operand};

/// Dense bytecode format version.
pub const DENSE_BYTECODE_VERSION: u32 = 1;

/// Numeric dense opcode discriminants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DenseOpcode {
    /// No operation.
    Nop = 0,
    /// `rN = constants[N]`.
    LoadConst = 1,
    /// `rN = operand`.
    Move = 2,
    /// `rN = local[N]`.
    LoadLocal = 3,
    /// `local[N] = operand`.
    StoreLocal = 4,
    /// `rN = lhs + rhs`.
    BinaryAdd = 5,
    /// `rN = lhs - rhs`.
    BinarySub = 14,
    /// `rN = lhs * rhs`.
    BinaryMul = 15,
    /// `rN = lhs / rhs`.
    BinaryDiv = 16,
    /// `rN = lhs % rhs`.
    BinaryMod = 17,
    /// `rN = lhs . rhs`.
    BinaryConcat = 6,
    /// `rN = lhs ** rhs`.
    BinaryPow = 18,
    /// `rN = lhs & rhs`.
    BinaryBitAnd = 19,
    /// `rN = lhs | rhs`.
    BinaryBitOr = 20,
    /// `rN = lhs ^ rhs`.
    BinaryBitXor = 21,
    /// `rN = lhs << rhs`.
    BinaryShiftLeft = 22,
    /// `rN = lhs >> rhs`.
    BinaryShiftRight = 23,
    /// `rN = lhs == rhs`.
    CompareEqual = 24,
    /// `rN = lhs != rhs`.
    CompareNotEqual = 25,
    /// `rN = lhs === rhs`.
    CompareIdentical = 26,
    /// `rN = lhs !== rhs`.
    CompareNotIdentical = 27,
    /// `rN = lhs < rhs`.
    CompareLess = 28,
    /// `rN = lhs <= rhs`.
    CompareLessEqual = 29,
    /// `rN = lhs > rhs`.
    CompareGreater = 30,
    /// `rN = lhs >= rhs`.
    CompareGreaterEqual = 31,
    /// `rN = lhs <=> rhs`.
    CompareSpaceship = 32,
    /// `rN = +src`.
    UnaryPlus = 33,
    /// `rN = -src`.
    UnaryMinus = 34,
    /// `rN = !src`.
    UnaryNot = 35,
    /// `rN = ~src`.
    UnaryBitNot = 36,
    /// `rN = function(args...)`.
    CallFunction = 37,
    /// `rN = constants[N]; echo rN`.
    LoadConstEcho = 38,
    /// `rN = local[N]; echo rN`.
    LoadLocalEcho = 39,
    /// `rN = lhs . rhs; echo rN`.
    BinaryConcatEcho = 40,
    /// `rN = []`.
    NewArray = 41,
    /// Insert or append one value into an array register.
    ArrayInsert = 42,
    /// `rN = array[key]`.
    FetchDim = 43,
    /// `local[dims...] = value`.
    AssignDim = 44,
    /// `local[dims...][] = value`.
    AppendDim = 45,
    /// Initialize a by-value foreach iterator.
    ForeachInit = 46,
    /// Advance a by-value foreach iterator.
    ForeachNext = 47,
    /// Emit one operand to output.
    Echo = 7,
    /// Jump to a dense block index.
    Jump = 8,
    /// Jump to a dense block index when the condition is false.
    JumpIfFalse = 9,
    /// Jump to a dense block index when the condition is true.
    JumpIfTrue = 10,
    /// Jump to one of two dense block indexes.
    JumpIf = 11,
    /// Return from the current function.
    Return = 12,
    /// Drop an unused operand value.
    Discard = 13,
}

impl DenseOpcode {
    #[must_use]
    const fn is_terminator(self) -> bool {
        matches!(
            self,
            Self::Jump | Self::JumpIfFalse | Self::JumpIfTrue | Self::JumpIf | Self::Return
        )
    }

    /// Stable opcode spelling for reports and snapshots.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nop => "nop",
            Self::LoadConst => "load_const",
            Self::Move => "move",
            Self::LoadLocal => "load_local",
            Self::StoreLocal => "store_local",
            Self::BinaryAdd => "binary_add",
            Self::BinarySub => "binary_sub",
            Self::BinaryMul => "binary_mul",
            Self::BinaryDiv => "binary_div",
            Self::BinaryMod => "binary_mod",
            Self::BinaryConcat => "binary_concat",
            Self::BinaryPow => "binary_pow",
            Self::BinaryBitAnd => "binary_bit_and",
            Self::BinaryBitOr => "binary_bit_or",
            Self::BinaryBitXor => "binary_bit_xor",
            Self::BinaryShiftLeft => "binary_shift_left",
            Self::BinaryShiftRight => "binary_shift_right",
            Self::CompareEqual => "compare_equal",
            Self::CompareNotEqual => "compare_not_equal",
            Self::CompareIdentical => "compare_identical",
            Self::CompareNotIdentical => "compare_not_identical",
            Self::CompareLess => "compare_less",
            Self::CompareLessEqual => "compare_less_equal",
            Self::CompareGreater => "compare_greater",
            Self::CompareGreaterEqual => "compare_greater_equal",
            Self::CompareSpaceship => "compare_spaceship",
            Self::UnaryPlus => "unary_plus",
            Self::UnaryMinus => "unary_minus",
            Self::UnaryNot => "unary_not",
            Self::UnaryBitNot => "unary_bit_not",
            Self::CallFunction => "call_function",
            Self::LoadConstEcho => "load_const_echo",
            Self::LoadLocalEcho => "load_local_echo",
            Self::BinaryConcatEcho => "binary_concat_echo",
            Self::NewArray => "new_array",
            Self::ArrayInsert => "array_insert",
            Self::FetchDim => "fetch_dim",
            Self::AssignDim => "assign_dim",
            Self::AppendDim => "append_dim",
            Self::ForeachInit => "foreach_init",
            Self::ForeachNext => "foreach_next",
            Self::Echo => "echo",
            Self::Jump => "jump",
            Self::JumpIfFalse => "jump_if_false",
            Self::JumpIfTrue => "jump_if_true",
            Self::JumpIf => "jump_if",
            Self::Return => "return",
            Self::Discard => "discard",
        }
    }

    /// Whether this opcode was emitted by the superinstruction selection pass.
    #[must_use]
    pub const fn is_superinstruction(self) -> bool {
        matches!(
            self,
            Self::LoadConstEcho | Self::LoadLocalEcho | Self::BinaryConcatEcho
        )
    }
}

/// Dense side-table span ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DenseSpanId(u32);

impl DenseSpanId {
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Dense name/string side-table ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DenseNameId(u32);

impl DenseNameId {
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Dense inline-cache side-table ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DenseCacheSlotId(u32);

impl DenseCacheSlotId {
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Predecoded operand kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DenseOperandKind {
    /// Register index.
    Register,
    /// Local slot index.
    Local,
    /// Constant-pool index.
    Constant,
}

/// Predecoded operand index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DenseOperand {
    /// Operand arena.
    pub kind: DenseOperandKind,
    /// Zero-based index inside the owning arena.
    pub index: u32,
}

/// Dense instruction operands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DenseOperands {
    /// No operands.
    None,
    /// Register and constant indexes.
    RegConst { dst: u32, constant: u32 },
    /// Register destination and generic operand.
    RegOperand { dst: u32, src: DenseOperand },
    /// Local destination/source and generic operand.
    LocalOperand { local: u32, src: DenseOperand },
    /// Binary operation over two predecoded operands.
    Binary {
        dst: u32,
        lhs: DenseOperand,
        rhs: DenseOperand,
    },
    /// Simple positional direct function call.
    Call {
        dst: u32,
        name: u32,
        args: Vec<DenseCallArg>,
    },
    /// Register destination only.
    Dst { dst: u32 },
    /// Array insert/append operands.
    ArrayInsert {
        array: u32,
        key: Option<DenseOperand>,
        value: DenseOperand,
        by_ref_local: Option<u32>,
    },
    /// Array dimension fetch operands.
    FetchDim {
        dst: u32,
        array: DenseOperand,
        key: DenseOperand,
        quiet: bool,
    },
    /// Local array dimension assignment/append operands.
    AssignDim {
        dst: u32,
        local: u32,
        dims: Vec<DenseOperand>,
        value: DenseOperand,
    },
    /// Foreach iterator initialization.
    ForeachInit { iterator: u32, source: DenseOperand },
    /// Foreach iterator advance.
    ForeachNext {
        has_value: u32,
        iterator: u32,
        key: Option<u32>,
        value: u32,
    },
    /// One generic operand.
    Operand { src: DenseOperand },
    /// One dense block target.
    Jump { target: u32 },
    /// Conditional dense block target.
    JumpIf {
        condition: DenseOperand,
        target: u32,
    },
    /// Conditional dense true/false block targets.
    JumpIfElse {
        condition: DenseOperand,
        if_true: u32,
        if_false: u32,
    },
    /// Optional return operand.
    Return { value: Option<DenseOperand> },
}

/// One dense function-call argument with call-shape metadata preserved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseCallArg {
    /// Optional named-argument name side-table index.
    pub name: Option<u32>,
    /// Evaluated value operand.
    pub value: DenseOperand,
    /// Source value class for by-reference send compatibility.
    pub value_kind: IrCallArgValueKind,
    /// Caller local when this argument can satisfy a by-reference parameter.
    pub by_ref_local: Option<u32>,
    /// Caller array dimension when this argument can satisfy a by-reference parameter.
    pub by_ref_dim: Option<DenseCallDimTarget>,
    /// Caller property when this argument can satisfy a by-reference parameter.
    pub by_ref_property: Option<DenseCallPropertyTarget>,
}

/// Dense by-reference array-dimension call target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseCallDimTarget {
    /// Root caller local.
    pub local: u32,
    /// Evaluated dimension operands.
    pub dims: Vec<DenseOperand>,
}

/// Dense by-reference property call target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseCallPropertyTarget {
    /// Evaluated object operand.
    pub object: DenseOperand,
    /// Static property name side-table index.
    pub property: u32,
}

/// One dense instruction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseInstruction {
    /// Numeric opcode.
    pub opcode: DenseOpcode,
    /// Predecoded operands.
    pub operands: DenseOperands,
    /// Source-span side-table ID.
    pub span: DenseSpanId,
    /// Optional cache slot for future IC/quickening sites.
    pub cache_slot: Option<DenseCacheSlotId>,
}

/// Dense basic-block metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseBlock {
    /// Dense block index within its function.
    pub id: u32,
    /// First instruction index in the function instruction array.
    pub first_instruction: u32,
    /// Number of instructions, including the terminator.
    pub instruction_len: u32,
    /// Dense instruction index of the block terminator.
    pub terminator: u32,
}

/// One dense function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseFunction {
    /// Function name for debug output.
    pub name: String,
    /// Register count from rich IR.
    pub register_count: u32,
    /// Local count from rich IR.
    pub local_count: u32,
    /// Dense blocks.
    pub blocks: Vec<DenseBlock>,
    /// Dense instruction array.
    pub instructions: Vec<DenseInstruction>,
}

/// Dense source-map target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DenseSourceMapTarget {
    /// Function table entry.
    Function { function: u32 },
    /// Dense block.
    Block { function: u32, block: u32 },
    /// Dense instruction inside a dense block.
    Instruction {
        function: u32,
        block: u32,
        instruction: u32,
    },
    /// Dense block terminator.
    Terminator { function: u32, block: u32 },
}

/// Dense source-map entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseSourceMapEntry {
    /// Dense target.
    pub target: DenseSourceMapTarget,
    /// Stable frontend origin label.
    pub origin: String,
    /// Span side-table ID.
    pub span: DenseSpanId,
}

/// Dense VM bytecode unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseBytecodeUnit {
    /// Dense bytecode format version.
    pub version: u32,
    /// Constant-pool length from rich IR.
    pub constant_count: u32,
    /// File-table length from rich IR.
    pub file_count: u32,
    /// Dense functions.
    pub functions: Vec<DenseFunction>,
    /// Source span side table.
    pub spans: Vec<IrSpan>,
    /// String/name side table reserved for future supported opcodes.
    pub names: Vec<String>,
    /// Cache slot side table reserved for future IC/quickening sites.
    pub cache_slots: Vec<String>,
    /// Dense source map.
    pub source_map: Vec<DenseSourceMapEntry>,
}

/// Superinstruction selection summary for counters and smoke tests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SuperinstructionSelectionReport {
    /// Adjacent instruction pairs considered valid fusion candidates.
    pub candidates: u64,
    /// Candidate pairs grouped by stable superinstruction spelling.
    pub candidates_by_kind: BTreeMap<String, u64>,
    /// Superinstructions emitted into the dense instruction stream.
    pub emitted: u64,
    /// Emitted superinstructions grouped by stable opcode spelling.
    pub emitted_by_kind: BTreeMap<String, u64>,
    /// Patterns intentionally left unfused, grouped by stable reason.
    pub skipped_by_reason: BTreeMap<String, u64>,
}

/// Request-local dense-bytecode block profile used by conservative layout.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BytecodeLayoutProfile {
    /// Dense block entries keyed as `f{function}:b{block}`.
    pub block_entries: BTreeMap<String, u64>,
}

impl BytecodeLayoutProfile {
    /// Returns true when no block-frequency data is available.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.block_entries.is_empty()
    }

    #[must_use]
    fn block_entry_count(&self, function: u32, block: u32) -> u64 {
        self.block_entries
            .get(&dense_block_key(function, block))
            .copied()
            .unwrap_or_default()
    }
}

/// One dense function layout result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BytecodeFunctionLayout {
    /// Function table index.
    pub function: u32,
    /// Whether this function's block descriptor order changed.
    pub changed: bool,
    /// Old block index to new block index.
    pub old_to_new: Vec<u32>,
    /// New block order expressed as old block indexes.
    pub new_to_old: Vec<u32>,
}

/// Summary of a dense-bytecode block layout pass.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BytecodeLayoutReport {
    /// Layout mode requested by the caller.
    pub mode: &'static str,
    /// Stable skip/reject reasons.
    pub skipped_by_reason: BTreeMap<String, u64>,
    /// Per-function mapping tables.
    pub functions: Vec<BytecodeFunctionLayout>,
}

impl BytecodeLayoutReport {
    #[must_use]
    fn source(reason: &'static str) -> Self {
        let mut report = Self {
            mode: "source",
            ..Self::default()
        };
        report.record_skip(reason);
        report
    }

    fn record_skip(&mut self, reason: &'static str) {
        *self
            .skipped_by_reason
            .entry(reason.to_string())
            .or_default() += 1;
    }

    /// Number of function block orders changed by the pass.
    #[must_use]
    pub fn changed_functions(&self) -> u64 {
        self.functions
            .iter()
            .filter(|function| function.changed)
            .count() as u64
    }
}

impl SuperinstructionSelectionReport {
    fn record_candidate(&mut self, opcode: DenseOpcode) {
        self.candidates += 1;
        *self
            .candidates_by_kind
            .entry(opcode.as_str().to_string())
            .or_default() += 1;
    }

    fn record_emitted(&mut self, opcode: DenseOpcode) {
        self.emitted += 1;
        *self
            .emitted_by_kind
            .entry(opcode.as_str().to_string())
            .or_default() += 1;
    }

    fn record_skipped(&mut self, reason: &'static str) {
        *self
            .skipped_by_reason
            .entry(reason.to_string())
            .or_default() += 1;
    }
}

impl DenseBytecodeUnit {
    /// Lower a rich IR unit into the dense design skeleton.
    pub fn lower_from_ir(unit: &IrUnit) -> Result<Self, DenseLowerError> {
        lower_unit(unit)
    }

    /// Verify dense bytecode invariants.
    pub fn verify(&self) -> Result<(), Vec<DenseVerifyError>> {
        verify_dense_unit(self)
    }

    /// Select conservative producer-plus-echo superinstructions in place.
    pub fn select_superinstructions(&mut self) -> SuperinstructionSelectionReport {
        let mut report = SuperinstructionSelectionReport::default();
        for function in &mut self.functions {
            select_function_superinstructions(function, &mut report);
        }
        report
    }

    /// Select report-only dense bytecode rules without mutating instruction order.
    #[must_use]
    pub fn select_rule_metadata(&self) -> RuleSelectionReport {
        let mut report = RuleSelectionReport::default();
        for function in &self.functions {
            select_dense_function_rules(function, &mut report);
        }
        report
    }

    /// Build metadata-only OSR entry maps for loop headers.
    #[must_use]
    pub fn osr_entry_map(&self) -> crate::osr::OsrEntryMap {
        crate::osr::analyze_dense_osr_entries(self)
    }

    /// Apply a conservative profile-guided dense block descriptor layout.
    ///
    /// Instruction arrays are not reordered. Branch operands and source-map
    /// block targets are remapped so dense instruction IDs and source spans stay
    /// stable.
    pub fn apply_profiled_layout(
        &mut self,
        profile: Option<&BytecodeLayoutProfile>,
    ) -> BytecodeLayoutReport {
        let Some(profile) = profile else {
            return BytecodeLayoutReport::source("missing_profile");
        };
        if profile.is_empty() {
            return BytecodeLayoutReport::source("empty_profile");
        }

        let mut report = BytecodeLayoutReport {
            mode: "profiled",
            ..BytecodeLayoutReport::default()
        };
        let mut mappings = Vec::with_capacity(self.functions.len());
        for (function_index, function) in self.functions.iter_mut().enumerate() {
            let mapping = apply_profiled_function_layout(function_index as u32, function, profile);
            if !mapping.changed {
                report.record_skip("identity_order");
            }
            mappings.push(mapping);
        }
        remap_source_map_targets(&mut self.source_map, &mappings);
        report.functions = mappings;
        report
    }

    /// Render a stable debug snapshot.
    #[must_use]
    pub fn to_snapshot_text(&self) -> String {
        render_snapshot(self)
    }
}

fn apply_profiled_function_layout(
    function_index: u32,
    function: &mut DenseFunction,
    profile: &BytecodeLayoutProfile,
) -> BytecodeFunctionLayout {
    let block_count = function.blocks.len();
    let identity: Vec<u32> = (0..block_count as u32).collect();
    if block_count <= 1 {
        return BytecodeFunctionLayout {
            function: function_index,
            changed: false,
            old_to_new: identity.clone(),
            new_to_old: identity,
        };
    }

    let order = schedule_profiled_blocks(function_index, function, profile);
    let changed = order
        .iter()
        .enumerate()
        .any(|(new_index, old_index)| new_index as u32 != *old_index);
    if !changed {
        return BytecodeFunctionLayout {
            function: function_index,
            changed: false,
            old_to_new: identity.clone(),
            new_to_old: identity,
        };
    }

    let mut old_to_new = vec![0; block_count];
    for (new_index, old_index) in order.iter().copied().enumerate() {
        old_to_new[old_index as usize] = new_index as u32;
    }

    for old_block in 0..block_count as u32 {
        rewrite_terminator_for_layout(function, old_block, &old_to_new);
    }

    let mut blocks = Vec::with_capacity(block_count);
    for (new_index, old_index) in order.iter().copied().enumerate() {
        let mut block = function.blocks[old_index as usize].clone();
        block.id = new_index as u32;
        blocks.push(block);
    }
    function.blocks = blocks;

    BytecodeFunctionLayout {
        function: function_index,
        changed,
        old_to_new,
        new_to_old: order,
    }
}

fn schedule_profiled_blocks(
    function_index: u32,
    function: &DenseFunction,
    profile: &BytecodeLayoutProfile,
) -> Vec<u32> {
    let block_count = function.blocks.len();
    let mut scheduled = vec![false; block_count];
    let mut order = Vec::with_capacity(block_count);
    let mut current = 0_u32;

    loop {
        if scheduled[current as usize] {
            break;
        }
        order.push(current);
        scheduled[current as usize] = true;
        let Some(next) = most_probable_unscheduled_successor(
            function_index,
            function,
            current,
            profile,
            &scheduled,
        ) else {
            break;
        };
        current = next;
    }

    for block in 0..block_count as u32 {
        if !scheduled[block as usize] {
            order.push(block);
            scheduled[block as usize] = true;
        }
    }
    order
}

fn most_probable_unscheduled_successor(
    function_index: u32,
    function: &DenseFunction,
    block: u32,
    profile: &BytecodeLayoutProfile,
    scheduled: &[bool],
) -> Option<u32> {
    let mut candidates: Vec<_> = dense_successors(function, block)
        .into_iter()
        .filter(|successor| {
            (*successor as usize) < scheduled.len() && !scheduled[*successor as usize]
        })
        .collect();
    candidates.sort_by(|left, right| {
        profile
            .block_entry_count(function_index, *right)
            .cmp(&profile.block_entry_count(function_index, *left))
            .then_with(|| left.cmp(right))
    });
    candidates.into_iter().next()
}

fn dense_successors(function: &DenseFunction, block: u32) -> Vec<u32> {
    let Some(block_meta) = function.blocks.get(block as usize) else {
        return Vec::new();
    };
    let Some(terminator) = function.instructions.get(block_meta.terminator as usize) else {
        return Vec::new();
    };
    let fallthrough = || {
        let next = block + 1;
        ((next as usize) < function.blocks.len()).then_some(next)
    };
    match (terminator.opcode, &terminator.operands) {
        (DenseOpcode::Jump, DenseOperands::Jump { target }) => vec![*target],
        (
            DenseOpcode::JumpIfFalse | DenseOpcode::JumpIfTrue,
            DenseOperands::JumpIf { target, .. },
        ) => {
            let mut successors = vec![*target];
            if let Some(next) = fallthrough() {
                successors.push(next);
            }
            successors
        }
        (
            DenseOpcode::JumpIf,
            DenseOperands::JumpIfElse {
                if_true, if_false, ..
            },
        ) => vec![*if_true, *if_false],
        _ => Vec::new(),
    }
}

fn rewrite_terminator_for_layout(function: &mut DenseFunction, old_block: u32, old_to_new: &[u32]) {
    let Some(block) = function.blocks.get(old_block as usize) else {
        return;
    };
    let Some(instruction) = function.instructions.get_mut(block.terminator as usize) else {
        return;
    };
    match (&mut instruction.opcode, &mut instruction.operands) {
        (DenseOpcode::Jump, DenseOperands::Jump { target }) => {
            *target = old_to_new[*target as usize];
        }
        (opcode @ (DenseOpcode::JumpIfFalse | DenseOpcode::JumpIfTrue), operands) => {
            let DenseOperands::JumpIf { condition, target } = *operands else {
                return;
            };
            let next_old = old_block + 1;
            let next_new = old_to_new[next_old as usize];
            let target_new = old_to_new[target as usize];
            let (if_true, if_false) = if *opcode == DenseOpcode::JumpIfTrue {
                (target_new, next_new)
            } else {
                (next_new, target_new)
            };
            *opcode = DenseOpcode::JumpIf;
            *operands = DenseOperands::JumpIfElse {
                condition,
                if_true,
                if_false,
            };
        }
        (
            DenseOpcode::JumpIf,
            DenseOperands::JumpIfElse {
                if_true, if_false, ..
            },
        ) => {
            *if_true = old_to_new[*if_true as usize];
            *if_false = old_to_new[*if_false as usize];
        }
        _ => {}
    }
}

fn remap_source_map_targets(
    source_map: &mut [DenseSourceMapEntry],
    mappings: &[BytecodeFunctionLayout],
) {
    for entry in source_map {
        match &mut entry.target {
            DenseSourceMapTarget::Block { function, block }
            | DenseSourceMapTarget::Instruction {
                function, block, ..
            }
            | DenseSourceMapTarget::Terminator { function, block } => {
                let Some(mapping) = mappings.get(*function as usize) else {
                    continue;
                };
                let Some(new_block) = mapping.old_to_new.get(*block as usize) else {
                    continue;
                };
                *block = *new_block;
            }
            DenseSourceMapTarget::Function { .. } => {}
        }
    }
}

#[must_use]
pub fn dense_block_key(function: u32, block: u32) -> String {
    format!("f{function}:b{block}")
}

fn select_function_superinstructions(
    function: &mut DenseFunction,
    report: &mut SuperinstructionSelectionReport,
) {
    for block_index in 0..function.blocks.len() {
        let (first, terminator) = {
            let block = &function.blocks[block_index];
            (block.first_instruction as usize, block.terminator as usize)
        };
        let mut index = first;
        while index + 1 < terminator {
            let next_operands = function.instructions[index + 1].operands.clone();
            if function.instructions[index + 1].opcode != DenseOpcode::Echo {
                index += 1;
                continue;
            }
            let DenseOperands::Operand { src: echo_src } = next_operands else {
                report.record_skipped("echo_operand_shape");
                index += 1;
                continue;
            };
            let opcode = function.instructions[index].opcode;
            let operands = function.instructions[index].operands.clone();
            let Some(fused) = select_echo_fusion(opcode, &operands, echo_src) else {
                report.record_skipped("unsupported_producer_echo_pair");
                index += 1;
                continue;
            };
            report.record_candidate(fused);
            function.instructions[index].opcode = fused;
            function.instructions[index + 1].opcode = DenseOpcode::Nop;
            function.instructions[index + 1].operands = DenseOperands::None;
            report.record_emitted(fused);
            index += 2;
        }
    }
}

fn select_echo_fusion(
    opcode: DenseOpcode,
    operands: &DenseOperands,
    echo_src: DenseOperand,
) -> Option<DenseOpcode> {
    let echoed_register = match echo_src.kind {
        DenseOperandKind::Register => echo_src.index,
        DenseOperandKind::Local | DenseOperandKind::Constant => return None,
    };
    match (opcode, operands) {
        (DenseOpcode::LoadConst, DenseOperands::RegConst { dst, .. })
            if *dst == echoed_register =>
        {
            Some(DenseOpcode::LoadConstEcho)
        }
        (DenseOpcode::LoadLocal, DenseOperands::RegOperand { dst, src })
            if *dst == echoed_register && src.kind == DenseOperandKind::Local =>
        {
            Some(DenseOpcode::LoadLocalEcho)
        }
        (DenseOpcode::BinaryConcat, DenseOperands::Binary { dst, .. })
            if *dst == echoed_register =>
        {
            Some(DenseOpcode::BinaryConcatEcho)
        }
        _ => None,
    }
}

fn select_dense_function_rules(function: &DenseFunction, report: &mut RuleSelectionReport) {
    for block in &function.blocks {
        let first = block.first_instruction as usize;
        let terminator = block.terminator as usize;
        let mut index = first;
        while index <= terminator && index < function.instructions.len() {
            if let Some((kind, fused_child)) = select_dense_pair_rule(function, index, terminator) {
                let parent = report.next_id();
                report.push(RuleSelection::selected(
                    parent,
                    kind,
                    vec![index as u32, fused_child as u32],
                ));
                let child = report.next_id();
                report.push(RuleSelection::fused_child(
                    child,
                    parent,
                    vec![fused_child as u32],
                ));
                index = fused_child + 1;
                continue;
            }

            let instruction = &function.instructions[index];
            let id = report.next_id();
            match select_dense_single_rule(instruction) {
                Some(kind) => report.push(RuleSelection::selected(id, kind, vec![index as u32])),
                None => report.push(RuleSelection::skipped(
                    id,
                    vec![index as u32],
                    dense_skip_reason(instruction.opcode),
                )),
            }
            index += 1;
        }
    }
}

fn select_dense_pair_rule(
    function: &DenseFunction,
    index: usize,
    terminator: usize,
) -> Option<(RuleKind, usize)> {
    if index + 1 > terminator || index + 1 >= function.instructions.len() {
        return None;
    }

    let first = &function.instructions[index];
    let second = &function.instructions[index + 1];
    if second.opcode == DenseOpcode::Echo {
        let DenseOperands::Operand { src: echo_src } = second.operands else {
            return None;
        };
        return match select_echo_fusion(first.opcode, &first.operands, echo_src) {
            Some(DenseOpcode::LoadConstEcho) => Some((RuleKind::LoadConstEcho, index + 1)),
            Some(DenseOpcode::LoadLocalEcho) => Some((RuleKind::LoadLocalEcho, index + 1)),
            Some(DenseOpcode::BinaryConcatEcho) => Some((RuleKind::ConcatEcho, index + 1)),
            _ => None,
        };
    }

    if is_compare_opcode(first.opcode)
        && matches!(
            second.opcode,
            DenseOpcode::JumpIfFalse | DenseOpcode::JumpIfTrue | DenseOpcode::JumpIf
        )
        && compare_result_feeds_branch(&first.operands, &second.operands)
    {
        return Some((RuleKind::CompareAndBranch, index + 1));
    }

    None
}

fn select_dense_single_rule(instruction: &DenseInstruction) -> Option<RuleKind> {
    match instruction.opcode {
        DenseOpcode::Nop => Some(RuleKind::NoRule),
        DenseOpcode::LoadConst => Some(RuleKind::Const),
        DenseOpcode::Move | DenseOpcode::LoadLocal | DenseOpcode::StoreLocal => {
            Some(RuleKind::Move)
        }
        DenseOpcode::BinaryAdd
        | DenseOpcode::BinarySub
        | DenseOpcode::BinaryMul
        | DenseOpcode::BinaryDiv
        | DenseOpcode::BinaryMod
        | DenseOpcode::BinaryBitAnd
        | DenseOpcode::BinaryBitOr
        | DenseOpcode::BinaryBitXor
        | DenseOpcode::BinaryShiftLeft
        | DenseOpcode::BinaryShiftRight => Some(RuleKind::BinaryInt),
        DenseOpcode::BinaryConcat => Some(RuleKind::BinaryString),
        DenseOpcode::CompareEqual
        | DenseOpcode::CompareNotEqual
        | DenseOpcode::CompareIdentical
        | DenseOpcode::CompareNotIdentical
        | DenseOpcode::CompareLess
        | DenseOpcode::CompareLessEqual
        | DenseOpcode::CompareGreater
        | DenseOpcode::CompareGreaterEqual
        | DenseOpcode::CompareSpaceship => Some(RuleKind::Compare),
        DenseOpcode::LoadConstEcho => Some(RuleKind::LoadConstEcho),
        DenseOpcode::LoadLocalEcho => Some(RuleKind::LoadLocalEcho),
        DenseOpcode::BinaryConcatEcho => Some(RuleKind::ConcatEcho),
        DenseOpcode::FetchDim => {
            if matches!(
                instruction.operands,
                DenseOperands::FetchDim {
                    key: DenseOperand {
                        kind: DenseOperandKind::Constant,
                        ..
                    },
                    ..
                }
            ) {
                Some(RuleKind::PackedFetchConst)
            } else {
                None
            }
        }
        DenseOpcode::Return => Some(RuleKind::ReturnValue),
        DenseOpcode::Jump
        | DenseOpcode::JumpIfFalse
        | DenseOpcode::JumpIfTrue
        | DenseOpcode::JumpIf
        | DenseOpcode::Echo
        | DenseOpcode::Discard => Some(RuleKind::NoRule),
        DenseOpcode::CallFunction
        | DenseOpcode::NewArray
        | DenseOpcode::ArrayInsert
        | DenseOpcode::AssignDim
        | DenseOpcode::AppendDim
        | DenseOpcode::ForeachInit
        | DenseOpcode::ForeachNext
        | DenseOpcode::UnaryPlus
        | DenseOpcode::UnaryMinus
        | DenseOpcode::UnaryNot
        | DenseOpcode::UnaryBitNot
        | DenseOpcode::BinaryPow => None,
    }
}

fn is_compare_opcode(opcode: DenseOpcode) -> bool {
    matches!(
        opcode,
        DenseOpcode::CompareEqual
            | DenseOpcode::CompareNotEqual
            | DenseOpcode::CompareIdentical
            | DenseOpcode::CompareNotIdentical
            | DenseOpcode::CompareLess
            | DenseOpcode::CompareLessEqual
            | DenseOpcode::CompareGreater
            | DenseOpcode::CompareGreaterEqual
            | DenseOpcode::CompareSpaceship
    )
}

fn compare_result_feeds_branch(compare: &DenseOperands, branch: &DenseOperands) -> bool {
    let DenseOperands::Binary { dst, .. } = compare else {
        return false;
    };
    match branch {
        DenseOperands::JumpIf { condition, .. } | DenseOperands::JumpIfElse { condition, .. } => {
            condition.kind == DenseOperandKind::Register && condition.index == *dst
        }
        _ => false,
    }
}

fn dense_skip_reason(opcode: DenseOpcode) -> &'static str {
    match opcode {
        DenseOpcode::CallFunction => "effectful_call",
        DenseOpcode::NewArray
        | DenseOpcode::ArrayInsert
        | DenseOpcode::AssignDim
        | DenseOpcode::AppendDim
        | DenseOpcode::ForeachInit
        | DenseOpcode::ForeachNext => "php_value_or_memory_semantics",
        _ => "unsupported_shape",
    }
}

/// Stable dense lowerer error code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DenseLowerErrorCode {
    /// Unsupported instruction family for the current dense subset.
    UnsupportedInstruction,
    /// Unsupported terminator shape for the current dense subset.
    UnsupportedTerminator,
}

/// Dense lowerer error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseLowerError {
    /// Stable error code.
    pub code: DenseLowerErrorCode,
    /// Human-readable context.
    pub message: String,
}

/// Stable dense verifier error code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DenseVerifyErrorCode {
    /// Format version is not supported.
    InvalidVersion,
    /// Block index points outside the block table.
    InvalidBlock,
    /// Instruction index points outside the instruction table.
    InvalidInstruction,
    /// Register index points outside the function register file.
    InvalidRegister,
    /// Local index points outside the function local table.
    InvalidLocal,
    /// Constant index points outside the unit constant pool.
    InvalidConstant,
    /// Jump target points outside the function block table.
    InvalidJumpTarget,
    /// Span side-table ID or span range is invalid.
    InvalidSpan,
    /// Cache slot side-table ID is invalid.
    InvalidCacheSlot,
    /// Dense block does not end in exactly one terminator.
    InvalidTerminator,
    /// Opcode does not match its operand payload.
    InvalidOperandShape,
    /// Source-map target references missing dense data.
    InvalidSourceMap,
}

/// Dense verifier error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseVerifyError {
    /// Stable error code.
    pub code: DenseVerifyErrorCode,
    /// Human-readable context.
    pub message: String,
}

fn lower_unit(unit: &IrUnit) -> Result<DenseBytecodeUnit, DenseLowerError> {
    let mut dense = DenseBytecodeUnit {
        version: DENSE_BYTECODE_VERSION,
        constant_count: unit.constants.len() as u32,
        file_count: unit.files.len() as u32,
        functions: Vec::with_capacity(unit.functions.len()),
        spans: Vec::new(),
        names: Vec::new(),
        cache_slots: Vec::new(),
        source_map: Vec::new(),
    };
    for function in &unit.functions {
        dense.functions.push(lower_function(
            function,
            &mut dense.spans,
            &mut dense.names,
        )?);
    }
    for entry in unit.source_map.entries() {
        let span = push_span(&mut dense.spans, entry.span);
        dense.source_map.push(DenseSourceMapEntry {
            target: lower_source_map_target(&entry.target),
            origin: entry.origin.clone(),
            span,
        });
    }
    dense.verify().map_err(|errors| DenseLowerError {
        code: DenseLowerErrorCode::UnsupportedInstruction,
        message: format!("lowered dense bytecode failed verification: {errors:?}"),
    })?;
    Ok(dense)
}

fn lower_function(
    function: &IrFunction,
    spans: &mut Vec<IrSpan>,
    names: &mut Vec<String>,
) -> Result<DenseFunction, DenseLowerError> {
    let mut dense = DenseFunction {
        name: function.name.clone(),
        register_count: function.register_count,
        local_count: function.local_count,
        blocks: Vec::with_capacity(function.blocks.len()),
        instructions: Vec::new(),
    };
    for block in &function.blocks {
        let first_instruction = dense.instructions.len() as u32;
        for instruction in &block.instructions {
            dense
                .instructions
                .push(lower_instruction(instruction, spans, names)?);
        }
        let terminator = block.terminator.as_ref().ok_or_else(|| DenseLowerError {
            code: DenseLowerErrorCode::UnsupportedTerminator,
            message: format!("block {} has no terminator", block.id.raw()),
        })?;
        dense
            .instructions
            .push(lower_terminator(terminator, spans)?);
        let instruction_len = dense.instructions.len() as u32 - first_instruction;
        let terminator = dense.instructions.len() as u32 - 1;
        dense.blocks.push(DenseBlock {
            id: block.id.raw(),
            first_instruction,
            instruction_len,
            terminator,
        });
    }
    Ok(dense)
}

fn lower_instruction(
    instruction: &Instruction,
    spans: &mut Vec<IrSpan>,
    names: &mut Vec<String>,
) -> Result<DenseInstruction, DenseLowerError> {
    let span = push_span(spans, instruction.span);
    let (opcode, operands) = match &instruction.kind {
        InstructionKind::Nop => (DenseOpcode::Nop, DenseOperands::None),
        InstructionKind::LoadConst { dst, constant } => (
            DenseOpcode::LoadConst,
            DenseOperands::RegConst {
                dst: dst.raw(),
                constant: constant.raw(),
            },
        ),
        InstructionKind::Move { dst, src } => (
            DenseOpcode::Move,
            DenseOperands::RegOperand {
                dst: dst.raw(),
                src: lower_operand(*src),
            },
        ),
        InstructionKind::LoadLocal { dst, local } => (
            DenseOpcode::LoadLocal,
            DenseOperands::RegOperand {
                dst: dst.raw(),
                src: DenseOperand {
                    kind: DenseOperandKind::Local,
                    index: local.raw(),
                },
            },
        ),
        InstructionKind::StoreLocal { local, src } => (
            DenseOpcode::StoreLocal,
            DenseOperands::LocalOperand {
                local: local.raw(),
                src: lower_operand(*src),
            },
        ),
        InstructionKind::Binary { dst, op, lhs, rhs } => (
            dense_binary_opcode(*op),
            DenseOperands::Binary {
                dst: dst.raw(),
                lhs: lower_operand(*lhs),
                rhs: lower_operand(*rhs),
            },
        ),
        InstructionKind::Compare { dst, op, lhs, rhs } => (
            dense_compare_opcode(*op),
            DenseOperands::Binary {
                dst: dst.raw(),
                lhs: lower_operand(*lhs),
                rhs: lower_operand(*rhs),
            },
        ),
        InstructionKind::Unary { dst, op, src } => (
            dense_unary_opcode(*op),
            DenseOperands::RegOperand {
                dst: dst.raw(),
                src: lower_operand(*src),
            },
        ),
        InstructionKind::CallFunction { dst, name, args } => (
            DenseOpcode::CallFunction,
            DenseOperands::Call {
                dst: dst.raw(),
                name: push_name(names, name).index() as u32,
                args: lower_call_args(instruction, names, args)?,
            },
        ),
        InstructionKind::NewArray { dst } => {
            (DenseOpcode::NewArray, DenseOperands::Dst { dst: dst.raw() })
        }
        InstructionKind::ArrayInsert {
            array,
            key,
            value,
            by_ref_local,
        } => (
            DenseOpcode::ArrayInsert,
            DenseOperands::ArrayInsert {
                array: array.raw(),
                key: key.map(lower_operand),
                value: lower_operand(*value),
                by_ref_local: by_ref_local.map(LocalId::raw),
            },
        ),
        InstructionKind::ArraySpread { .. } => {
            return unsupported_instruction(instruction, "array spread".to_owned());
        }
        InstructionKind::FetchDim {
            dst,
            array,
            key,
            quiet,
        } => (
            DenseOpcode::FetchDim,
            DenseOperands::FetchDim {
                dst: dst.raw(),
                array: lower_operand(*array),
                key: lower_operand(*key),
                quiet: *quiet,
            },
        ),
        InstructionKind::AssignDim {
            dst,
            local,
            dims,
            value,
        } => (
            DenseOpcode::AssignDim,
            DenseOperands::AssignDim {
                dst: dst.raw(),
                local: local.raw(),
                dims: dims.iter().copied().map(lower_operand).collect(),
                value: lower_operand(*value),
            },
        ),
        InstructionKind::AppendDim {
            dst,
            local,
            dims,
            value,
        } => (
            DenseOpcode::AppendDim,
            DenseOperands::AssignDim {
                dst: dst.raw(),
                local: local.raw(),
                dims: dims.iter().copied().map(lower_operand).collect(),
                value: lower_operand(*value),
            },
        ),
        InstructionKind::ForeachInit { iterator, source } => (
            DenseOpcode::ForeachInit,
            DenseOperands::ForeachInit {
                iterator: iterator.raw(),
                source: lower_operand(*source),
            },
        ),
        InstructionKind::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => (
            DenseOpcode::ForeachNext,
            DenseOperands::ForeachNext {
                has_value: has_value.raw(),
                iterator: iterator.raw(),
                key: key.map(RegId::raw),
                value: value.raw(),
            },
        ),
        InstructionKind::Echo { src } => (
            DenseOpcode::Echo,
            DenseOperands::Operand {
                src: lower_operand(*src),
            },
        ),
        InstructionKind::Discard { src } => (
            DenseOpcode::Discard,
            DenseOperands::Operand {
                src: lower_operand(*src),
            },
        ),
        other => return unsupported_instruction(instruction, format!("{other:?}")),
    };
    Ok(DenseInstruction {
        opcode,
        operands,
        span,
        cache_slot: None,
    })
}

fn lower_terminator(
    terminator: &Terminator,
    spans: &mut Vec<IrSpan>,
) -> Result<DenseInstruction, DenseLowerError> {
    let span = push_span(spans, terminator.span);
    let (opcode, operands) = match &terminator.kind {
        TerminatorKind::Jump { target } => (
            DenseOpcode::Jump,
            DenseOperands::Jump {
                target: target.raw(),
            },
        ),
        TerminatorKind::JumpIfFalse { condition, target } => (
            DenseOpcode::JumpIfFalse,
            DenseOperands::JumpIf {
                condition: lower_operand(*condition),
                target: target.raw(),
            },
        ),
        TerminatorKind::JumpIfTrue { condition, target } => (
            DenseOpcode::JumpIfTrue,
            DenseOperands::JumpIf {
                condition: lower_operand(*condition),
                target: target.raw(),
            },
        ),
        TerminatorKind::JumpIf {
            condition,
            if_true,
            if_false,
        } => (
            DenseOpcode::JumpIf,
            DenseOperands::JumpIfElse {
                condition: lower_operand(*condition),
                if_true: if_true.raw(),
                if_false: if_false.raw(),
            },
        ),
        TerminatorKind::Return {
            value,
            by_ref_local,
        } => {
            if by_ref_local.is_some() {
                return Err(DenseLowerError {
                    code: DenseLowerErrorCode::UnsupportedTerminator,
                    message: "by-reference return is outside dense bytecode phase 09.03 subset"
                        .to_string(),
                });
            }
            (
                DenseOpcode::Return,
                DenseOperands::Return {
                    value: value.map(lower_operand),
                },
            )
        }
    };
    Ok(DenseInstruction {
        opcode,
        operands,
        span,
        cache_slot: None,
    })
}

fn lower_operand(operand: Operand) -> DenseOperand {
    match operand {
        Operand::Register(id) => DenseOperand {
            kind: DenseOperandKind::Register,
            index: id.raw(),
        },
        Operand::Local(id) => DenseOperand {
            kind: DenseOperandKind::Local,
            index: id.raw(),
        },
        Operand::Constant(id) => DenseOperand {
            kind: DenseOperandKind::Constant,
            index: id.raw(),
        },
    }
}

fn lower_call_args(
    instruction: &Instruction,
    names: &mut Vec<String>,
    args: &[IrCallArg],
) -> Result<Vec<DenseCallArg>, DenseLowerError> {
    let mut lowered = Vec::with_capacity(args.len());
    for arg in args {
        if arg.unpack {
            return unsupported_instruction(
                instruction,
                "CallFunction with unpacked arguments".to_string(),
            );
        }
        lowered.push(DenseCallArg {
            name: arg
                .name
                .as_ref()
                .map(|name| push_name(names, name).index() as u32),
            value: lower_operand(arg.value),
            value_kind: arg.value_kind,
            by_ref_local: arg.by_ref_local.map(LocalId::raw),
            by_ref_dim: arg.by_ref_dim.as_ref().map(|target| DenseCallDimTarget {
                local: target.local.raw(),
                dims: target.dims.iter().copied().map(lower_operand).collect(),
            }),
            by_ref_property: arg
                .by_ref_property
                .as_ref()
                .map(|target| DenseCallPropertyTarget {
                    object: lower_operand(target.object),
                    property: push_name(names, &target.property).index() as u32,
                }),
        });
    }
    Ok(lowered)
}

fn dense_binary_opcode(op: BinaryOp) -> DenseOpcode {
    match op {
        BinaryOp::Add => DenseOpcode::BinaryAdd,
        BinaryOp::Sub => DenseOpcode::BinarySub,
        BinaryOp::Mul => DenseOpcode::BinaryMul,
        BinaryOp::Div => DenseOpcode::BinaryDiv,
        BinaryOp::Mod => DenseOpcode::BinaryMod,
        BinaryOp::Concat => DenseOpcode::BinaryConcat,
        BinaryOp::Pow => DenseOpcode::BinaryPow,
        BinaryOp::BitAnd => DenseOpcode::BinaryBitAnd,
        BinaryOp::BitOr => DenseOpcode::BinaryBitOr,
        BinaryOp::BitXor => DenseOpcode::BinaryBitXor,
        BinaryOp::ShiftLeft => DenseOpcode::BinaryShiftLeft,
        BinaryOp::ShiftRight => DenseOpcode::BinaryShiftRight,
    }
}

fn dense_compare_opcode(op: CompareOp) -> DenseOpcode {
    match op {
        CompareOp::Equal => DenseOpcode::CompareEqual,
        CompareOp::NotEqual => DenseOpcode::CompareNotEqual,
        CompareOp::Identical => DenseOpcode::CompareIdentical,
        CompareOp::NotIdentical => DenseOpcode::CompareNotIdentical,
        CompareOp::Less => DenseOpcode::CompareLess,
        CompareOp::LessEqual => DenseOpcode::CompareLessEqual,
        CompareOp::Greater => DenseOpcode::CompareGreater,
        CompareOp::GreaterEqual => DenseOpcode::CompareGreaterEqual,
        CompareOp::Spaceship => DenseOpcode::CompareSpaceship,
    }
}

fn dense_unary_opcode(op: UnaryOp) -> DenseOpcode {
    match op {
        UnaryOp::Plus => DenseOpcode::UnaryPlus,
        UnaryOp::Minus => DenseOpcode::UnaryMinus,
        UnaryOp::Not => DenseOpcode::UnaryNot,
        UnaryOp::BitNot => DenseOpcode::UnaryBitNot,
    }
}

fn lower_source_map_target(target: &IrSourceMapTarget) -> DenseSourceMapTarget {
    match target {
        IrSourceMapTarget::Function { function } => DenseSourceMapTarget::Function {
            function: function.raw(),
        },
        IrSourceMapTarget::Block { function, block } => DenseSourceMapTarget::Block {
            function: function.raw(),
            block: block.raw(),
        },
        IrSourceMapTarget::Instruction {
            function,
            block,
            instruction,
        } => DenseSourceMapTarget::Instruction {
            function: function.raw(),
            block: block.raw(),
            instruction: instruction.raw(),
        },
        IrSourceMapTarget::Terminator { function, block } => DenseSourceMapTarget::Terminator {
            function: function.raw(),
            block: block.raw(),
        },
    }
}

fn unsupported_instruction<T>(
    instruction: &Instruction,
    detail: String,
) -> Result<T, DenseLowerError> {
    Err(DenseLowerError {
        code: DenseLowerErrorCode::UnsupportedInstruction,
        message: format!(
            "instruction {} is outside the current dense bytecode subset: {detail}",
            instruction.id.raw()
        ),
    })
}

fn push_span(spans: &mut Vec<IrSpan>, span: IrSpan) -> DenseSpanId {
    let id = DenseSpanId::new(spans.len() as u32);
    spans.push(span);
    id
}

fn push_name(names: &mut Vec<String>, name: &str) -> DenseNameId {
    let id = DenseNameId::new(names.len() as u32);
    names.push(name.to_string());
    id
}

fn verify_dense_unit(unit: &DenseBytecodeUnit) -> Result<(), Vec<DenseVerifyError>> {
    let mut errors = Vec::new();
    if unit.version != DENSE_BYTECODE_VERSION {
        errors.push(error(
            DenseVerifyErrorCode::InvalidVersion,
            format!("unsupported dense bytecode version {}", unit.version),
        ));
    }
    for (function_index, function) in unit.functions.iter().enumerate() {
        verify_function(unit, function_index, function, &mut errors);
    }
    for entry in &unit.source_map {
        verify_span_id(unit, entry.span, &mut errors);
        verify_source_map_target(unit, &entry.target, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn verify_function(
    unit: &DenseBytecodeUnit,
    function_index: usize,
    function: &DenseFunction,
    errors: &mut Vec<DenseVerifyError>,
) {
    for (block_index, block) in function.blocks.iter().enumerate() {
        if block.id as usize != block_index {
            errors.push(error(
                DenseVerifyErrorCode::InvalidBlock,
                format!(
                    "function {function_index} block {block_index} has id {}",
                    block.id
                ),
            ));
        }
        let first = block.first_instruction as usize;
        let len = block.instruction_len as usize;
        let end = first.saturating_add(len);
        if len == 0 || end > function.instructions.len() {
            errors.push(error(
                DenseVerifyErrorCode::InvalidInstruction,
                format!(
                    "function {function_index} block {block_index} invalid instruction range {first}..{end}"
                ),
            ));
            continue;
        }
        let expected_terminator = end - 1;
        if block.terminator as usize != expected_terminator {
            errors.push(error(
                DenseVerifyErrorCode::InvalidTerminator,
                format!(
                    "function {function_index} block {block_index} terminator {} should be {expected_terminator}",
                    block.terminator
                ),
            ));
        }
        for index in first..end {
            let instruction = &function.instructions[index];
            if index == expected_terminator {
                if !instruction.opcode.is_terminator() {
                    errors.push(error(
                        DenseVerifyErrorCode::InvalidTerminator,
                        format!(
                            "function {function_index} block {block_index} final opcode {:?} is not a terminator",
                            instruction.opcode
                        ),
                    ));
                }
            } else if instruction.opcode.is_terminator() {
                errors.push(error(
                    DenseVerifyErrorCode::InvalidTerminator,
                    format!(
                        "function {function_index} block {block_index} has terminator before block end at {index}"
                    ),
                ));
            }
            verify_instruction(unit, function, instruction, errors);
        }
    }
}

fn verify_instruction(
    unit: &DenseBytecodeUnit,
    function: &DenseFunction,
    instruction: &DenseInstruction,
    errors: &mut Vec<DenseVerifyError>,
) {
    verify_span_id(unit, instruction.span, errors);
    if let Some(cache_slot) = instruction.cache_slot
        && cache_slot.index() >= unit.cache_slots.len()
    {
        errors.push(error(
            DenseVerifyErrorCode::InvalidCacheSlot,
            format!("cache slot {} is outside side table", cache_slot.index()),
        ));
    }
    match (instruction.opcode, &instruction.operands) {
        (DenseOpcode::Nop, DenseOperands::None) => {}
        (
            DenseOpcode::LoadConst | DenseOpcode::LoadConstEcho,
            DenseOperands::RegConst { dst, constant },
        ) => {
            verify_register(*dst, function, errors);
            verify_constant(*constant, unit, errors);
        }
        (DenseOpcode::Move, DenseOperands::RegOperand { dst, src })
        | (
            DenseOpcode::LoadLocal | DenseOpcode::LoadLocalEcho,
            DenseOperands::RegOperand { dst, src },
        ) => {
            verify_register(*dst, function, errors);
            verify_operand(*src, unit, function, errors);
            if matches!(
                instruction.opcode,
                DenseOpcode::LoadLocal | DenseOpcode::LoadLocalEcho
            ) && src.kind != DenseOperandKind::Local
            {
                errors.push(error(
                    DenseVerifyErrorCode::InvalidOperandShape,
                    "load_local source must be a local operand".to_string(),
                ));
            }
        }
        (DenseOpcode::StoreLocal, DenseOperands::LocalOperand { local, src }) => {
            verify_local(*local, function, errors);
            verify_operand(*src, unit, function, errors);
        }
        (
            DenseOpcode::BinaryAdd
            | DenseOpcode::BinarySub
            | DenseOpcode::BinaryMul
            | DenseOpcode::BinaryDiv
            | DenseOpcode::BinaryMod
            | DenseOpcode::BinaryConcat
            | DenseOpcode::BinaryPow
            | DenseOpcode::BinaryBitAnd
            | DenseOpcode::BinaryBitOr
            | DenseOpcode::BinaryBitXor
            | DenseOpcode::BinaryShiftLeft
            | DenseOpcode::BinaryShiftRight
            | DenseOpcode::BinaryConcatEcho
            | DenseOpcode::CompareEqual
            | DenseOpcode::CompareNotEqual
            | DenseOpcode::CompareIdentical
            | DenseOpcode::CompareNotIdentical
            | DenseOpcode::CompareLess
            | DenseOpcode::CompareLessEqual
            | DenseOpcode::CompareGreater
            | DenseOpcode::CompareGreaterEqual
            | DenseOpcode::CompareSpaceship,
            DenseOperands::Binary { dst, lhs, rhs },
        ) => {
            verify_register(*dst, function, errors);
            verify_operand(*lhs, unit, function, errors);
            verify_operand(*rhs, unit, function, errors);
        }
        (
            DenseOpcode::UnaryPlus
            | DenseOpcode::UnaryMinus
            | DenseOpcode::UnaryNot
            | DenseOpcode::UnaryBitNot,
            DenseOperands::RegOperand { dst, src },
        ) => {
            verify_register(*dst, function, errors);
            verify_operand(*src, unit, function, errors);
        }
        (DenseOpcode::CallFunction, DenseOperands::Call { dst, name, args }) => {
            verify_register(*dst, function, errors);
            verify_name(*name, unit, errors);
            for arg in args {
                verify_call_arg(arg, unit, function, errors);
            }
        }
        (DenseOpcode::NewArray, DenseOperands::Dst { dst }) => {
            verify_register(*dst, function, errors);
        }
        (
            DenseOpcode::ArrayInsert,
            DenseOperands::ArrayInsert {
                array,
                key,
                value,
                by_ref_local,
            },
        ) => {
            verify_register(*array, function, errors);
            if let Some(key) = key {
                verify_operand(*key, unit, function, errors);
            }
            verify_operand(*value, unit, function, errors);
            if let Some(local) = by_ref_local {
                verify_local(*local, function, errors);
            }
        }
        (
            DenseOpcode::FetchDim,
            DenseOperands::FetchDim {
                dst, array, key, ..
            },
        ) => {
            verify_register(*dst, function, errors);
            verify_operand(*array, unit, function, errors);
            verify_operand(*key, unit, function, errors);
        }
        (
            DenseOpcode::AssignDim | DenseOpcode::AppendDim,
            DenseOperands::AssignDim {
                dst,
                local,
                dims,
                value,
            },
        ) => {
            verify_register(*dst, function, errors);
            verify_local(*local, function, errors);
            for dim in dims {
                verify_operand(*dim, unit, function, errors);
            }
            verify_operand(*value, unit, function, errors);
        }
        (DenseOpcode::ForeachInit, DenseOperands::ForeachInit { iterator, source }) => {
            verify_register(*iterator, function, errors);
            verify_operand(*source, unit, function, errors);
        }
        (
            DenseOpcode::ForeachNext,
            DenseOperands::ForeachNext {
                has_value,
                iterator,
                key,
                value,
            },
        ) => {
            verify_register(*has_value, function, errors);
            verify_register(*iterator, function, errors);
            if let Some(key) = key {
                verify_register(*key, function, errors);
            }
            verify_register(*value, function, errors);
        }
        (DenseOpcode::Echo, DenseOperands::Operand { src }) => {
            verify_operand(*src, unit, function, errors);
        }
        (DenseOpcode::Discard, DenseOperands::Operand { src }) => {
            verify_operand(*src, unit, function, errors);
        }
        (DenseOpcode::Jump, DenseOperands::Jump { target }) => {
            verify_jump_target(*target, function, errors);
        }
        (
            DenseOpcode::JumpIfFalse | DenseOpcode::JumpIfTrue,
            DenseOperands::JumpIf { condition, target },
        ) => {
            verify_operand(*condition, unit, function, errors);
            verify_jump_target(*target, function, errors);
        }
        (
            DenseOpcode::JumpIf,
            DenseOperands::JumpIfElse {
                condition,
                if_true,
                if_false,
            },
        ) => {
            verify_operand(*condition, unit, function, errors);
            verify_jump_target(*if_true, function, errors);
            verify_jump_target(*if_false, function, errors);
        }
        (DenseOpcode::Return, DenseOperands::Return { value }) => {
            if let Some(value) = value {
                verify_operand(*value, unit, function, errors);
            }
        }
        _ => errors.push(error(
            DenseVerifyErrorCode::InvalidOperandShape,
            format!(
                "opcode {:?} does not match operands {:?}",
                instruction.opcode, instruction.operands
            ),
        )),
    }
}

fn verify_operand(
    operand: DenseOperand,
    unit: &DenseBytecodeUnit,
    function: &DenseFunction,
    errors: &mut Vec<DenseVerifyError>,
) {
    match operand.kind {
        DenseOperandKind::Register => verify_register(operand.index, function, errors),
        DenseOperandKind::Local => verify_local(operand.index, function, errors),
        DenseOperandKind::Constant => verify_constant(operand.index, unit, errors),
    }
}

fn verify_call_arg(
    arg: &DenseCallArg,
    unit: &DenseBytecodeUnit,
    function: &DenseFunction,
    errors: &mut Vec<DenseVerifyError>,
) {
    if let Some(name) = arg.name {
        verify_name(name, unit, errors);
    }
    verify_operand(arg.value, unit, function, errors);
    if let Some(local) = arg.by_ref_local {
        verify_local(local, function, errors);
    }
    if let Some(target) = &arg.by_ref_dim {
        verify_local(target.local, function, errors);
        for dim in &target.dims {
            verify_operand(*dim, unit, function, errors);
        }
    }
    if let Some(target) = &arg.by_ref_property {
        verify_operand(target.object, unit, function, errors);
        verify_name(target.property, unit, errors);
    }
}

fn verify_register(index: u32, function: &DenseFunction, errors: &mut Vec<DenseVerifyError>) {
    if index >= function.register_count {
        errors.push(error(
            DenseVerifyErrorCode::InvalidRegister,
            format!(
                "register {index} is outside register_count {}",
                function.register_count
            ),
        ));
    }
}

fn verify_local(index: u32, function: &DenseFunction, errors: &mut Vec<DenseVerifyError>) {
    if index >= function.local_count {
        errors.push(error(
            DenseVerifyErrorCode::InvalidLocal,
            format!(
                "local {index} is outside local_count {}",
                function.local_count
            ),
        ));
    }
}

fn verify_constant(index: u32, unit: &DenseBytecodeUnit, errors: &mut Vec<DenseVerifyError>) {
    if index >= unit.constant_count {
        errors.push(error(
            DenseVerifyErrorCode::InvalidConstant,
            format!(
                "constant {index} is outside constant_count {}",
                unit.constant_count
            ),
        ));
    }
}

fn verify_name(index: u32, unit: &DenseBytecodeUnit, errors: &mut Vec<DenseVerifyError>) {
    if index as usize >= unit.names.len() {
        errors.push(error(
            DenseVerifyErrorCode::InvalidOperandShape,
            format!("name {index} is outside names side table"),
        ));
    }
}

fn verify_jump_target(index: u32, function: &DenseFunction, errors: &mut Vec<DenseVerifyError>) {
    if index as usize >= function.blocks.len() {
        errors.push(error(
            DenseVerifyErrorCode::InvalidJumpTarget,
            format!(
                "jump target {index} is outside block count {}",
                function.blocks.len()
            ),
        ));
    }
}

fn verify_span_id(unit: &DenseBytecodeUnit, span: DenseSpanId, errors: &mut Vec<DenseVerifyError>) {
    match unit.spans.get(span.index()) {
        Some(value)
            if value.file.index() < unit.file_count as usize && value.start <= value.end => {}
        Some(value) => errors.push(error(
            DenseVerifyErrorCode::InvalidSpan,
            format!(
                "span {} points at file {} range {}..{} with file_count {}",
                span.raw(),
                value.file.raw(),
                value.start,
                value.end,
                unit.file_count
            ),
        )),
        None => errors.push(error(
            DenseVerifyErrorCode::InvalidSpan,
            format!("span {} is outside side table", span.raw()),
        )),
    }
}

fn verify_source_map_target(
    unit: &DenseBytecodeUnit,
    target: &DenseSourceMapTarget,
    errors: &mut Vec<DenseVerifyError>,
) {
    match target {
        DenseSourceMapTarget::Function { function } => {
            if *function as usize >= unit.functions.len() {
                errors.push(error(
                    DenseVerifyErrorCode::InvalidSourceMap,
                    format!("source map function {function} is missing"),
                ));
            }
        }
        DenseSourceMapTarget::Block { function, block }
        | DenseSourceMapTarget::Terminator { function, block } => {
            if let Some(dense_function) = unit.functions.get(*function as usize) {
                if *block as usize >= dense_function.blocks.len() {
                    errors.push(error(
                        DenseVerifyErrorCode::InvalidSourceMap,
                        format!("source map block {block} is missing"),
                    ));
                }
            } else {
                errors.push(error(
                    DenseVerifyErrorCode::InvalidSourceMap,
                    format!("source map function {function} is missing"),
                ));
            }
        }
        DenseSourceMapTarget::Instruction {
            function,
            block,
            instruction,
        } => {
            if let Some(dense_function) = unit.functions.get(*function as usize) {
                if let Some(dense_block) = dense_function.blocks.get(*block as usize) {
                    if *instruction >= dense_block.instruction_len.saturating_sub(1) {
                        errors.push(error(
                            DenseVerifyErrorCode::InvalidSourceMap,
                            format!(
                                "source map instruction {instruction} is outside block instruction count {}",
                                dense_block.instruction_len.saturating_sub(1)
                            ),
                        ));
                    }
                } else {
                    errors.push(error(
                        DenseVerifyErrorCode::InvalidSourceMap,
                        format!("source map block {block} is missing"),
                    ));
                }
            } else {
                errors.push(error(
                    DenseVerifyErrorCode::InvalidSourceMap,
                    format!("source map function {function} is missing"),
                ));
            }
        }
    }
}

fn error(code: DenseVerifyErrorCode, message: String) -> DenseVerifyError {
    DenseVerifyError { code, message }
}

fn render_snapshot(unit: &DenseBytecodeUnit) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "dense_bytecode version={} constants={} files={} functions={} spans={} names={} cache_slots={}\n",
        unit.version,
        unit.constant_count,
        unit.file_count,
        unit.functions.len(),
        unit.spans.len(),
        unit.names.len(),
        unit.cache_slots.len()
    ));
    for (function_index, function) in unit.functions.iter().enumerate() {
        out.push_str(&format!(
            "function {function_index} {} regs={} locals={} blocks={} instructions={}\n",
            function.name,
            function.register_count,
            function.local_count,
            function.blocks.len(),
            function.instructions.len()
        ));
        for block in &function.blocks {
            out.push_str(&format!(
                "  block {} first={} len={} terminator={}\n",
                block.id, block.first_instruction, block.instruction_len, block.terminator
            ));
            let first = block.first_instruction as usize;
            let end = first + block.instruction_len as usize;
            for index in first..end {
                let instruction = &function.instructions[index];
                out.push_str(&format!(
                    "    {:04} {} {} span={}\n",
                    index,
                    instruction.opcode.as_str(),
                    render_operands(&instruction.operands),
                    instruction.span.raw()
                ));
            }
        }
    }
    out.push_str("spans:\n");
    for (index, span) in unit.spans.iter().enumerate() {
        out.push_str(&format!(
            "  span {index} file:{}@{}..{}\n",
            span.file.raw(),
            span.start,
            span.end
        ));
    }
    out
}

fn render_operands(operands: &DenseOperands) -> String {
    match operands {
        DenseOperands::None => "-".to_string(),
        DenseOperands::RegConst { dst, constant } => format!("r{dst} c{constant}"),
        DenseOperands::RegOperand { dst, src } => format!("r{dst} {}", render_operand(*src)),
        DenseOperands::LocalOperand { local, src } => format!("l{local} {}", render_operand(*src)),
        DenseOperands::Binary { dst, lhs, rhs } => {
            format!("r{dst} {} {}", render_operand(*lhs), render_operand(*rhs))
        }
        DenseOperands::Call { dst, name, args } => {
            let rendered_args: Vec<_> = args.iter().map(render_call_arg).collect();
            format!("r{dst} n{name} ({})", rendered_args.join(", "))
        }
        DenseOperands::Dst { dst } => format!("r{dst}"),
        DenseOperands::ArrayInsert {
            array,
            key,
            value,
            by_ref_local,
        } => {
            let key = key.map_or_else(|| "[]".to_string(), render_operand);
            let suffix = by_ref_local.map_or_else(String::new, |local| format!(" by_ref=l{local}"));
            format!("r{array} {key} {}{suffix}", render_operand(*value))
        }
        DenseOperands::FetchDim {
            dst,
            array,
            key,
            quiet,
        } => format!(
            "r{dst} {} {} quiet={quiet}",
            render_operand(*array),
            render_operand(*key)
        ),
        DenseOperands::AssignDim {
            dst,
            local,
            dims,
            value,
        } => {
            let dims: Vec<_> = dims.iter().copied().map(render_operand).collect();
            format!(
                "r{dst} l{local} [{}] {}",
                dims.join(", "),
                render_operand(*value)
            )
        }
        DenseOperands::ForeachInit { iterator, source } => {
            format!("r{iterator} {}", render_operand(*source))
        }
        DenseOperands::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            let key = key.map_or_else(|| "-".to_string(), |key| format!("r{key}"));
            format!("r{has_value} r{iterator} key={key} value=r{value}")
        }
        DenseOperands::Operand { src } => render_operand(*src),
        DenseOperands::Jump { target } => format!("b{target}"),
        DenseOperands::JumpIf { condition, target } => {
            format!("{} b{target}", render_operand(*condition))
        }
        DenseOperands::JumpIfElse {
            condition,
            if_true,
            if_false,
        } => format!("{} b{if_true} b{if_false}", render_operand(*condition)),
        DenseOperands::Return { value } => value.map_or_else(|| "-".to_string(), render_operand),
    }
}

fn render_operand(operand: DenseOperand) -> String {
    match operand.kind {
        DenseOperandKind::Register => format!("r{}", operand.index),
        DenseOperandKind::Local => format!("l{}", operand.index),
        DenseOperandKind::Constant => format!("c{}", operand.index),
    }
}

fn render_call_arg(arg: &DenseCallArg) -> String {
    let mut out = render_operand(arg.value);
    if let Some(name) = arg.name {
        out.push_str(&format!(" name=n{name}"));
    }
    if let Some(local) = arg.by_ref_local {
        out.push_str(&format!(" by_ref=l{local}"));
    }
    if let Some(target) = &arg.by_ref_dim {
        out.push_str(&format!(" by_ref_dim=l{}", target.local));
    }
    if let Some(target) = &arg.by_ref_property {
        out.push_str(&format!(" by_ref_prop=n{}", target.property));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        BytecodeLayoutProfile, DenseBlock, DenseBytecodeUnit, DenseFunction, DenseInstruction,
        DenseOpcode, DenseOperand, DenseOperandKind, DenseOperands, DenseSourceMapEntry,
        DenseSourceMapTarget, DenseSpanId, DenseVerifyErrorCode, dense_block_key,
    };
    use php_ir::{
        BinaryOp, FunctionFlags, InstructionKind, IrBuilder, IrConstant, IrSpan, Operand, UnitId,
        verify_unit,
    };
    use php_semantics::analyze_source;

    #[test]
    fn bytecode_lowering_snapshot_is_stable_for_manual_add() {
        let unit = manual_basic_unit();
        verify_unit(&unit).expect("manual IR verifies before dense lowering");
        let dense = DenseBytecodeUnit::lower_from_ir(&unit).expect("manual IR lowers to dense");
        dense.verify().expect("dense bytecode verifies");
        assert_eq!(
            dense.to_snapshot_text(),
            concat!(
                "dense_bytecode version=1 constants=2 files=1 functions=1 spans=4 names=0 cache_slots=0\n",
                "function 0 main regs=3 locals=0 blocks=1 instructions=4\n",
                "  block 0 first=0 len=4 terminator=3\n",
                "    0000 load_const r0 c0 span=0\n",
                "    0001 load_const r1 c1 span=1\n",
                "    0002 binary_add r2 r0 r1 span=2\n",
                "    0003 return r2 span=3\n",
                "spans:\n",
                "  span 0 file:0@6..7\n",
                "  span 1 file:0@10..11\n",
                "  span 2 file:0@6..11\n",
                "  span 3 file:0@6..11\n",
            )
        );
    }

    #[test]
    fn bytecode_lowering_covers_echo_literal_fixture_shape() {
        let frontend = analyze_source("<?php echo 1;");
        let result = php_ir::lower_frontend_result(
            &frontend,
            php_ir::LoweringOptions {
                source_path: "fixtures/bytecode/literals/valid/echo-int.php".to_string(),
                ..php_ir::LoweringOptions::default()
            },
        );
        result
            .verification
            .expect("fixture IR verifies before dense lowering");
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let dense = DenseBytecodeUnit::lower_from_ir(&result.unit)
            .expect("literal echo fixture lowers to dense bytecode");
        assert!(
            dense.functions[0]
                .instructions
                .iter()
                .any(|item| item.opcode == DenseOpcode::Echo)
        );
        assert_eq!(
            dense.functions[0]
                .instructions
                .last()
                .map(|item| item.opcode),
            Some(DenseOpcode::Return)
        );
    }

    #[test]
    fn bytecode_lowering_covers_scalar_binary_unary_and_compare_shapes() {
        let frontend = analyze_source("<?php echo 1 + 2 * 3, 2 ** 3, !false, 1 == \"1\", 2 <=> 3;");
        let result = php_ir::lower_frontend_result(
            &frontend,
            php_ir::LoweringOptions {
                source_path: "fixtures/runtime/valid/scalars/expressions.php".to_string(),
                ..php_ir::LoweringOptions::default()
            },
        );
        result
            .verification
            .expect("scalar expression IR verifies before dense lowering");
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let dense = DenseBytecodeUnit::lower_from_ir(&result.unit)
            .expect("scalar expression fixture lowers to dense bytecode");
        let opcodes: Vec<_> = dense.functions[0]
            .instructions
            .iter()
            .map(|item| item.opcode)
            .collect();
        assert!(opcodes.contains(&DenseOpcode::BinaryMul));
        assert!(opcodes.contains(&DenseOpcode::BinaryAdd));
        assert!(opcodes.contains(&DenseOpcode::BinaryPow));
        assert!(opcodes.contains(&DenseOpcode::UnaryNot));
        assert!(opcodes.contains(&DenseOpcode::CompareEqual));
        assert!(opcodes.contains(&DenseOpcode::CompareSpaceship));
    }

    #[test]
    fn bytecode_lowering_covers_simple_direct_function_calls() {
        let frontend = analyze_source(
            r#"<?php
function add($a, $b) {
    return $a + $b;
}
echo add(2, 3), "\n";
"#,
        );
        let result = php_ir::lower_frontend_result(
            &frontend,
            php_ir::LoweringOptions {
                source_path: "fixtures/runtime/valid/functions/two-args.php".to_string(),
                ..php_ir::LoweringOptions::default()
            },
        );
        result
            .verification
            .expect("direct call IR verifies before dense lowering");
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let dense =
            DenseBytecodeUnit::lower_from_ir(&result.unit).expect("direct call lowers to dense");
        assert!(dense.names.iter().any(|name| name == "add"));
        assert!(
            dense.functions[0]
                .instructions
                .iter()
                .any(|item| item.opcode == DenseOpcode::CallFunction)
        );
    }

    #[test]
    fn bytecode_lowering_covers_arrays_dims_and_foreach() {
        let frontend = analyze_source(
            r#"<?php
$items = [];
for ($i = 0; $i < 3; $i++) {
    $items[] = $i + 1;
}
$sum = $items[1];
foreach ($items as $value) {
    $sum += $value;
}
echo count($items), ":", $sum, "\n";
"#,
        );
        let result = php_ir::lower_frontend_result(
            &frontend,
            php_ir::LoweringOptions {
                source_path: "tests/fixtures/performance/perf_smoke/arrays_packed.php".to_string(),
                ..php_ir::LoweringOptions::default()
            },
        );
        result
            .verification
            .expect("array/foreach IR verifies before dense lowering");
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let dense =
            DenseBytecodeUnit::lower_from_ir(&result.unit).expect("array/foreach lowers to dense");
        let opcodes: Vec<_> = dense.functions[0]
            .instructions
            .iter()
            .map(|item| item.opcode)
            .collect();
        assert!(opcodes.contains(&DenseOpcode::NewArray));
        assert!(opcodes.contains(&DenseOpcode::AppendDim));
        assert!(opcodes.contains(&DenseOpcode::FetchDim));
        assert!(opcodes.contains(&DenseOpcode::ForeachInit));
        assert!(opcodes.contains(&DenseOpcode::ForeachNext));
        assert!(opcodes.contains(&DenseOpcode::CallFunction));
        assert!(dense.names.iter().any(|name| name == "count"));
    }

    #[test]
    fn superinstruction_selection_fuses_low_risk_echo_pairs() {
        let frontend = analyze_source(
            r#"<?php
$name = "world";
echo "hello ";
echo $name;
echo "a" . "b";
"#,
        );
        let result = php_ir::lower_frontend_result(
            &frontend,
            php_ir::LoweringOptions {
                source_path: "fixtures/performance/superinstructions/echo.php".to_string(),
                ..php_ir::LoweringOptions::default()
            },
        );
        result
            .verification
            .expect("superinstruction source IR verifies before dense lowering");
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let mut dense = DenseBytecodeUnit::lower_from_ir(&result.unit)
            .expect("superinstruction source lowers to dense bytecode");

        let report = dense.select_superinstructions();

        dense.verify().expect("selected bytecode still verifies");
        assert!(report.candidates >= 3, "{report:#?}");
        assert_eq!(
            report
                .emitted_by_kind
                .get(DenseOpcode::LoadConstEcho.as_str()),
            Some(&1)
        );
        assert_eq!(
            report
                .emitted_by_kind
                .get(DenseOpcode::LoadLocalEcho.as_str()),
            Some(&1)
        );
        assert_eq!(
            report
                .emitted_by_kind
                .get(DenseOpcode::BinaryConcatEcho.as_str()),
            Some(&1)
        );
        assert!(
            dense.functions[0]
                .instructions
                .iter()
                .any(|item| item.opcode == DenseOpcode::Nop)
        );
    }

    #[test]
    fn rule_selection_reports_concat_echo_and_skipped_child() {
        let dense = manual_dense_unit(vec![
            DenseInstruction {
                opcode: DenseOpcode::BinaryConcat,
                operands: DenseOperands::Binary {
                    dst: 0,
                    lhs: dense_const(0),
                    rhs: dense_const(1),
                },
                span: DenseSpanId::new(0),
                cache_slot: None,
            },
            DenseInstruction {
                opcode: DenseOpcode::Echo,
                operands: DenseOperands::Operand { src: dense_reg(0) },
                span: DenseSpanId::new(0),
                cache_slot: None,
            },
            DenseInstruction {
                opcode: DenseOpcode::Return,
                operands: DenseOperands::Return { value: None },
                span: DenseSpanId::new(0),
                cache_slot: None,
            },
        ]);

        let report = dense.select_rule_metadata();
        let dump = report.dump_text();

        assert_eq!(report.rule_selection_selected, 2);
        assert_eq!(report.rule_selection_fused, 1);
        assert!(dump.contains("concat_echo=1"));
        assert!(dump.contains("fused_into_r0=1"));
        assert!(dump.contains("r0 concat_echo sources=[0,1]"));
        assert!(dump.contains("r1 fused_into_r0 sources=[1] parent=r0 reason=fused_child"));
    }

    #[test]
    fn rule_selection_reports_compare_and_branch() {
        let dense = manual_dense_unit(vec![
            DenseInstruction {
                opcode: DenseOpcode::CompareLess,
                operands: DenseOperands::Binary {
                    dst: 0,
                    lhs: dense_const(0),
                    rhs: dense_const(1),
                },
                span: DenseSpanId::new(0),
                cache_slot: None,
            },
            DenseInstruction {
                opcode: DenseOpcode::JumpIfFalse,
                operands: DenseOperands::JumpIf {
                    condition: dense_reg(0),
                    target: 1,
                },
                span: DenseSpanId::new(0),
                cache_slot: None,
            },
            DenseInstruction {
                opcode: DenseOpcode::Return,
                operands: DenseOperands::Return { value: None },
                span: DenseSpanId::new(0),
                cache_slot: None,
            },
        ]);

        let report = dense.select_rule_metadata();
        let dump = report.dump_text();

        assert!(dump.contains("compare_and_branch=1"));
        assert!(dump.contains("r0 compare_and_branch sources=[0,1]"));
        assert!(dump.contains("r1 fused_into_r0 sources=[1] parent=r0 reason=fused_child"));
    }

    #[test]
    fn rule_selection_dump_is_stable_and_skips_effectful_call() {
        let dense = manual_dense_unit(vec![
            DenseInstruction {
                opcode: DenseOpcode::CallFunction,
                operands: DenseOperands::Call {
                    dst: 0,
                    name: 0,
                    args: Vec::new(),
                },
                span: DenseSpanId::new(0),
                cache_slot: None,
            },
            DenseInstruction {
                opcode: DenseOpcode::Return,
                operands: DenseOperands::Return {
                    value: Some(dense_reg(0)),
                },
                span: DenseSpanId::new(0),
                cache_slot: None,
            },
        ]);

        assert_eq!(
            dense.select_rule_metadata().dump_text(),
            concat!(
                "rule-selection\n",
                "candidates=2\n",
                "selected=1\n",
                "fused=0\n",
                "skipped=1\n",
                "by-kind:\n",
                "  return_value=1\n",
                "  skipped=1\n",
                "selections:\n",
                "  r0 skipped sources=[0] reason=effectful_call\n",
                "  r1 return_value sources=[1]\n",
            )
        );
    }

    #[test]
    fn profiled_layout_reorders_blocks_and_preserves_conditional_fallthrough() {
        let file = php_ir::FileId::new(0);
        let mut dense = DenseBytecodeUnit {
            version: super::DENSE_BYTECODE_VERSION,
            constant_count: 1,
            file_count: 1,
            functions: vec![DenseFunction {
                name: "main".to_string(),
                register_count: 0,
                local_count: 0,
                blocks: vec![
                    DenseBlock {
                        id: 0,
                        first_instruction: 0,
                        instruction_len: 1,
                        terminator: 0,
                    },
                    DenseBlock {
                        id: 1,
                        first_instruction: 1,
                        instruction_len: 1,
                        terminator: 1,
                    },
                    DenseBlock {
                        id: 2,
                        first_instruction: 2,
                        instruction_len: 1,
                        terminator: 2,
                    },
                ],
                instructions: vec![
                    DenseInstruction {
                        opcode: DenseOpcode::JumpIfFalse,
                        operands: DenseOperands::JumpIf {
                            condition: DenseOperand {
                                kind: DenseOperandKind::Constant,
                                index: 0,
                            },
                            target: 2,
                        },
                        span: DenseSpanId::new(0),
                        cache_slot: None,
                    },
                    DenseInstruction {
                        opcode: DenseOpcode::Return,
                        operands: DenseOperands::Return { value: None },
                        span: DenseSpanId::new(0),
                        cache_slot: None,
                    },
                    DenseInstruction {
                        opcode: DenseOpcode::Return,
                        operands: DenseOperands::Return { value: None },
                        span: DenseSpanId::new(0),
                        cache_slot: None,
                    },
                ],
            }],
            spans: vec![IrSpan::new(file, 0, 1)],
            names: Vec::new(),
            cache_slots: Vec::new(),
            source_map: vec![DenseSourceMapEntry {
                target: DenseSourceMapTarget::Block {
                    function: 0,
                    block: 2,
                },
                origin: "manual".to_string(),
                span: DenseSpanId::new(0),
            }],
        };
        dense.verify().expect("manual dense unit verifies");
        let mut profile = BytecodeLayoutProfile::default();
        profile.block_entries.insert(dense_block_key(0, 2), 10);
        profile.block_entries.insert(dense_block_key(0, 1), 1);

        let report = dense.apply_profiled_layout(Some(&profile));

        dense.verify().expect("layout-remapped dense unit verifies");
        assert_eq!(report.changed_functions(), 1);
        assert_eq!(report.functions[0].new_to_old, vec![0, 2, 1]);
        assert_eq!(dense.functions[0].blocks[1].first_instruction, 2);
        assert_eq!(
            dense.functions[0].instructions[0].opcode,
            DenseOpcode::JumpIf
        );
        assert_eq!(
            dense.functions[0].instructions[0].operands,
            DenseOperands::JumpIfElse {
                condition: DenseOperand {
                    kind: DenseOperandKind::Constant,
                    index: 0,
                },
                if_true: 2,
                if_false: 1,
            }
        );
        assert_eq!(
            dense.source_map[0].target,
            DenseSourceMapTarget::Block {
                function: 0,
                block: 1,
            }
        );
    }

    #[test]
    fn bytecode_lowering_rejects_unsupported_instruction_family() {
        let mut unit = manual_basic_unit();
        unit.functions[0].blocks[0].instructions[2].kind = InstructionKind::FetchConst {
            dst: php_ir::RegId::new(2),
            name: "PHP_VERSION".to_string(),
        };
        let error = DenseBytecodeUnit::lower_from_ir(&unit).expect_err("fetch const unsupported");
        assert_eq!(
            error.code,
            super::DenseLowerErrorCode::UnsupportedInstruction
        );
        assert!(error.message.contains("FetchConst"));
    }

    #[test]
    fn bytecode_verifier_rejects_invalid_indexes_and_terminators() {
        let unit = manual_basic_unit();
        let mut dense = DenseBytecodeUnit::lower_from_ir(&unit).expect("manual IR lowers");
        dense.constant_count = 1;
        dense.functions[0].register_count = 1;
        dense.functions[0].blocks[0].terminator = 2;
        dense.functions[0].instructions[0].span = DenseSpanId::new(99);
        let errors = dense
            .verify()
            .expect_err("mutated dense bytecode should fail");
        let codes: Vec<_> = errors.iter().map(|error| error.code).collect();
        assert!(codes.contains(&DenseVerifyErrorCode::InvalidConstant));
        assert!(codes.contains(&DenseVerifyErrorCode::InvalidRegister));
        assert!(codes.contains(&DenseVerifyErrorCode::InvalidTerminator));
        assert!(codes.contains(&DenseVerifyErrorCode::InvalidSpan));
    }

    #[test]
    fn bytecode_verifier_rejects_operand_shape_mismatch() {
        let unit = manual_basic_unit();
        let mut dense = DenseBytecodeUnit::lower_from_ir(&unit).expect("manual IR lowers");
        dense.functions[0].instructions[0] = DenseInstruction {
            opcode: DenseOpcode::LoadConst,
            operands: DenseOperands::None,
            span: DenseSpanId::new(0),
            cache_slot: None,
        };
        let errors = dense
            .verify()
            .expect_err("operand shape mismatch should fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == DenseVerifyErrorCode::InvalidOperandShape)
        );
    }

    fn manual_dense_unit(instructions: Vec<DenseInstruction>) -> DenseBytecodeUnit {
        DenseBytecodeUnit {
            version: super::DENSE_BYTECODE_VERSION,
            constant_count: 2,
            file_count: 1,
            functions: vec![DenseFunction {
                name: "main".to_string(),
                register_count: 4,
                local_count: 0,
                blocks: vec![DenseBlock {
                    id: 0,
                    first_instruction: 0,
                    instruction_len: instructions.len() as u32,
                    terminator: instructions.len().saturating_sub(1) as u32,
                }],
                instructions,
            }],
            spans: vec![IrSpan::new(php_ir::FileId::new(0), 0, 1)],
            names: vec!["fn".to_string()],
            cache_slots: Vec::new(),
            source_map: Vec::new(),
        }
    }

    fn dense_reg(index: u32) -> DenseOperand {
        DenseOperand {
            kind: DenseOperandKind::Register,
            index,
        }
    }

    fn dense_const(index: u32) -> DenseOperand {
        DenseOperand {
            kind: DenseOperandKind::Constant,
            index,
        }
    }

    fn manual_basic_unit() -> php_ir::IrUnit {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("fixtures/runtime/valid/scalars/echo.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let block = builder.append_block(function);
        let one = builder.add_constant(IrConstant::Int(1));
        let two = builder.add_constant(IrConstant::Int(2));
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        builder.emit_load_const(function, block, r0, one, IrSpan::new(file, 6, 7));
        builder.emit_load_const(function, block, r1, two, IrSpan::new(file, 10, 11));
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r2,
                op: BinaryOp::Add,
                lhs: Operand::Register(r0),
                rhs: Operand::Register(r1),
            },
            IrSpan::new(file, 6, 11),
        );
        builder.terminate_return(
            function,
            block,
            Some(Operand::Register(r2)),
            IrSpan::new(file, 6, 11),
        );
        builder.set_entry(function);
        builder.finish()
    }
}
