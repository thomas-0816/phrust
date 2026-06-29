//! Metadata-only OSR entry maps for dense bytecode loops.

use std::collections::{BTreeMap, BTreeSet};

use php_ir::source_map::IrSpan;

use crate::bytecode::{
    DenseBytecodeUnit, DenseFunction, DenseInstruction, DenseOpcode, DenseOperandKind,
    DenseOperands,
};

/// Stable OSR entry identifier within one map.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OsrEntryId(u32);

impl OsrEntryId {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Abstract VM slot that must be available at an OSR entry.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OsrVmSlot {
    Local(u32),
    Register(u32),
    Iterator(u32),
}

impl OsrVmSlot {
    #[must_use]
    pub fn as_label(self) -> String {
        match self {
            Self::Local(index) => format!("local:{index}"),
            Self::Register(index) => format!("reg:{index}"),
            Self::Iterator(index) => format!("iter:{index}"),
        }
    }
}

/// Abstract target location for a live OSR slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OsrTargetLocation {
    VmLocal,
    VmRegister,
    VmIterator,
}

impl OsrTargetLocation {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VmLocal => "vm-local",
            Self::VmRegister => "vm-register",
            Self::VmIterator => "vm-iterator",
        }
    }
}

/// Coarse value class known at an OSR entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OsrValueClass {
    Unknown,
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    Mixed,
}

impl OsrValueClass {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Float => "float",
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
            Self::Mixed => "mixed",
        }
    }
}

/// Reference/COW safety classification for an OSR live slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OsrRefCowSafety {
    GuardedByValue,
    ReferenceOrCowState,
    IteratorState,
    Unknown,
}

impl OsrRefCowSafety {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GuardedByValue => "guarded-by-value",
            Self::ReferenceOrCowState => "reference-or-cow-state",
            Self::IteratorState => "iterator-state",
            Self::Unknown => "unknown",
        }
    }
}

/// One live slot required by an OSR entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OsrLiveSlot {
    pub slot: OsrVmSlot,
    pub target_location: OsrTargetLocation,
    pub value_class: OsrValueClass,
    pub ref_cow_safety: OsrRefCowSafety,
}

/// Unsupported PHP state kind that may be annotated before dense opcodes exist.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OsrUnsupportedStateKind {
    ByReferenceForeach,
    TryFinally,
    Generator,
    Fiber,
    DynamicCall,
}

impl OsrUnsupportedStateKind {
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ByReferenceForeach => "by_ref_foreach_state",
            Self::TryFinally => "try_finally_state",
            Self::Generator => "generator_state",
            Self::Fiber => "fiber_state",
            Self::DynamicCall => "dynamic_call",
        }
    }
}

/// Optional loop-state annotations supplied by future frontend/runtime analyses.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OsrLoopStateAnnotations {
    unsupported: BTreeMap<(u32, u32), BTreeSet<OsrUnsupportedStateKind>>,
}

impl OsrLoopStateAnnotations {
    pub fn mark_unsupported(
        &mut self,
        function: u32,
        loop_header_block: u32,
        reason: OsrUnsupportedStateKind,
    ) {
        self.unsupported
            .entry((function, loop_header_block))
            .or_default()
            .insert(reason);
    }

    fn unsupported_reasons(&self, function: u32, loop_header_block: u32) -> Vec<String> {
        self.unsupported
            .get(&(function, loop_header_block))
            .into_iter()
            .flat_map(|reasons| reasons.iter())
            .map(|reason| reason.reason().to_string())
            .collect()
    }
}

/// One metadata-only OSR entry candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OsrEntry {
    pub id: OsrEntryId,
    pub function: u32,
    pub loop_header_block: u32,
    pub loop_header_bytecode_offset: u32,
    pub backedge_block: u32,
    pub source_span: Option<IrSpan>,
    pub required_live_slots: Vec<OsrLiveSlot>,
    pub entry_guard_requirements: Vec<String>,
    pub unsupported_reasons: Vec<String>,
    pub fake_control_predecessor: Option<u32>,
}

impl OsrEntry {
    #[must_use]
    pub fn representable(&self) -> bool {
        self.unsupported_reasons.is_empty()
    }
}

/// Aggregate OSR entry counters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OsrEntryReport {
    pub osr_entry_candidates: u64,
    pub osr_entry_representable: u64,
    pub osr_entry_rejected_by_reason: BTreeMap<String, u64>,
    pub osr_live_slots: u64,
}

/// Dense-bytecode OSR entry map.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OsrEntryMap {
    pub entries: Vec<OsrEntry>,
    pub report: OsrEntryReport,
}

/// Builds metadata-only OSR entries from dense-bytecode loop backedges.
#[must_use]
pub fn analyze_dense_osr_entries(unit: &DenseBytecodeUnit) -> OsrEntryMap {
    analyze_dense_osr_entries_with_annotations(unit, &OsrLoopStateAnnotations::default())
}

/// Builds metadata-only OSR entries with external unsupported-state annotations.
#[must_use]
pub fn analyze_dense_osr_entries_with_annotations(
    unit: &DenseBytecodeUnit,
    annotations: &OsrLoopStateAnnotations,
) -> OsrEntryMap {
    let mut map = OsrEntryMap::default();
    for (function_index, function) in unit.functions.iter().enumerate() {
        let loops = dense_loop_headers(function);
        for (header, backedge) in loops {
            let id = OsrEntryId::new(map.entries.len() as u32);
            let entry = build_osr_entry(
                unit,
                function_index as u32,
                function,
                header,
                backedge,
                id,
                annotations,
            );
            map.report.osr_entry_candidates += 1;
            map.report.osr_live_slots += entry.required_live_slots.len() as u64;
            if entry.representable() {
                map.report.osr_entry_representable += 1;
            } else {
                for reason in &entry.unsupported_reasons {
                    *map.report
                        .osr_entry_rejected_by_reason
                        .entry(reason.clone())
                        .or_default() += 1;
                }
            }
            map.entries.push(entry);
        }
    }
    map
}

fn build_osr_entry(
    unit: &DenseBytecodeUnit,
    function_index: u32,
    function: &DenseFunction,
    header: u32,
    backedge: u32,
    id: OsrEntryId,
    annotations: &OsrLoopStateAnnotations,
) -> OsrEntry {
    let loop_header_bytecode_offset = function
        .blocks
        .iter()
        .find(|block| block.id == header)
        .map_or(0, |block| block.first_instruction);
    let source_span = function
        .instructions
        .get(loop_header_bytecode_offset as usize)
        .and_then(|instruction| unit.spans.get(instruction.span.index()).cloned());
    let mut unsupported_reasons = inspect_loop_body(function, header, backedge);
    unsupported_reasons.extend(annotations.unsupported_reasons(function_index, header));
    unsupported_reasons.sort();
    unsupported_reasons.dedup();

    OsrEntry {
        id,
        function: function_index,
        loop_header_block: header,
        loop_header_bytecode_offset,
        backedge_block: backedge,
        source_span,
        required_live_slots: collect_live_slots(function, header, backedge),
        entry_guard_requirements: vec![
            "vm-frame-layout".to_string(),
            "no-active-exception".to_string(),
            "no-generator-or-fiber".to_string(),
            "reference-cow-guards".to_string(),
        ],
        unsupported_reasons,
        fake_control_predecessor: Some(backedge),
    }
}

fn dense_loop_headers(function: &DenseFunction) -> Vec<(u32, u32)> {
    let mut loops = BTreeMap::new();
    for block in &function.blocks {
        let Some(terminator) = function.instructions.get(block.terminator as usize) else {
            continue;
        };
        for successor in instruction_successors(terminator) {
            if successor <= block.id {
                loops
                    .entry(successor)
                    .and_modify(|backedge: &mut u32| *backedge = (*backedge).max(block.id))
                    .or_insert(block.id);
            }
        }
    }
    loops.into_iter().collect()
}

fn instruction_successors(instruction: &DenseInstruction) -> Vec<u32> {
    match (instruction.opcode, &instruction.operands) {
        (DenseOpcode::Jump, DenseOperands::Jump { target }) => vec![*target],
        (
            DenseOpcode::JumpIfFalse | DenseOpcode::JumpIfTrue,
            DenseOperands::JumpIf { target, .. },
        ) => vec![*target],
        (
            DenseOpcode::JumpIf,
            DenseOperands::JumpIfElse {
                if_true, if_false, ..
            },
        ) => vec![*if_true, *if_false],
        _ => Vec::new(),
    }
}

fn inspect_loop_body(function: &DenseFunction, header: u32, backedge: u32) -> Vec<String> {
    let mut reasons = BTreeSet::new();
    for instruction in loop_instructions(function, header, backedge) {
        match (&instruction.opcode, &instruction.operands) {
            (DenseOpcode::CallFunction, _) => {
                reasons.insert("dynamic_call".to_string());
            }
            (DenseOpcode::ForeachInit | DenseOpcode::ForeachNext, _) => {
                reasons.insert("foreach_state_model_missing".to_string());
            }
            (
                DenseOpcode::ArrayInsert,
                DenseOperands::ArrayInsert {
                    by_ref_local: Some(_),
                    ..
                },
            ) => {
                reasons.insert("by_ref_foreach_state".to_string());
            }
            (
                DenseOpcode::ArrayInsert
                | DenseOpcode::AssignDim
                | DenseOpcode::AppendDim
                | DenseOpcode::FetchDim
                | DenseOpcode::NewArray,
                _,
            ) => {
                reasons.insert("array_cow_state".to_string());
            }
            (DenseOpcode::Echo, _) => {
                reasons.insert("output_state".to_string());
            }
            _ => {}
        }
    }
    reasons.into_iter().collect()
}

fn collect_live_slots(function: &DenseFunction, header: u32, backedge: u32) -> Vec<OsrLiveSlot> {
    let mut slots = BTreeSet::new();
    for local in 0..function.local_count {
        slots.insert(OsrVmSlot::Local(local));
    }
    for instruction in loop_instructions(function, header, backedge) {
        collect_instruction_slots(instruction, &mut slots);
    }
    slots
        .into_iter()
        .map(|slot| {
            let target_location = match slot {
                OsrVmSlot::Local(_) => OsrTargetLocation::VmLocal,
                OsrVmSlot::Register(_) => OsrTargetLocation::VmRegister,
                OsrVmSlot::Iterator(_) => OsrTargetLocation::VmIterator,
            };
            let ref_cow_safety = match slot {
                OsrVmSlot::Iterator(_) => OsrRefCowSafety::IteratorState,
                _ => OsrRefCowSafety::GuardedByValue,
            };
            OsrLiveSlot {
                slot,
                target_location,
                value_class: OsrValueClass::Unknown,
                ref_cow_safety,
            }
        })
        .collect()
}

fn collect_instruction_slots(instruction: &DenseInstruction, slots: &mut BTreeSet<OsrVmSlot>) {
    match &instruction.operands {
        DenseOperands::None => {}
        DenseOperands::RegConst { dst, .. } | DenseOperands::Dst { dst } => {
            slots.insert(OsrVmSlot::Register(*dst));
        }
        DenseOperands::RegOperand { dst, src } => {
            slots.insert(OsrVmSlot::Register(*dst));
            collect_operand_slot(*src, slots);
        }
        DenseOperands::LocalOperand { local, src } => {
            slots.insert(OsrVmSlot::Local(*local));
            collect_operand_slot(*src, slots);
        }
        DenseOperands::StaticLocal { local, default, .. } => {
            slots.insert(OsrVmSlot::Local(*local));
            collect_operand_slot(*default, slots);
        }
        DenseOperands::Binary { dst, lhs, rhs } => {
            slots.insert(OsrVmSlot::Register(*dst));
            collect_operand_slot(*lhs, slots);
            collect_operand_slot(*rhs, slots);
        }
        DenseOperands::Call { dst, args, .. } => {
            slots.insert(OsrVmSlot::Register(*dst));
            for arg in args {
                collect_operand_slot(arg.value, slots);
                if let Some(local) = arg.by_ref_local {
                    slots.insert(OsrVmSlot::Local(local));
                }
            }
        }
        DenseOperands::MethodCall {
            dst, object, args, ..
        } => {
            slots.insert(OsrVmSlot::Register(*dst));
            collect_operand_slot(*object, slots);
            for arg in args {
                collect_operand_slot(arg.value, slots);
                if let Some(local) = arg.by_ref_local {
                    slots.insert(OsrVmSlot::Local(local));
                }
            }
        }
        DenseOperands::StaticCall { dst, args, .. } => {
            slots.insert(OsrVmSlot::Register(*dst));
            for arg in args {
                collect_operand_slot(arg.value, slots);
                if let Some(local) = arg.by_ref_local {
                    slots.insert(OsrVmSlot::Local(local));
                }
            }
        }
        DenseOperands::ArrayInsert {
            array,
            key,
            value,
            by_ref_local,
        } => {
            slots.insert(OsrVmSlot::Register(*array));
            if let Some(key) = key {
                collect_operand_slot(*key, slots);
            }
            collect_operand_slot(*value, slots);
            if let Some(local) = by_ref_local {
                slots.insert(OsrVmSlot::Local(*local));
            }
        }
        DenseOperands::FetchDim {
            dst, array, key, ..
        } => {
            slots.insert(OsrVmSlot::Register(*dst));
            collect_operand_slot(*array, slots);
            collect_operand_slot(*key, slots);
        }
        DenseOperands::AssignDim {
            dst,
            local,
            dims,
            value,
        } => {
            slots.insert(OsrVmSlot::Register(*dst));
            slots.insert(OsrVmSlot::Local(*local));
            for dim in dims {
                collect_operand_slot(*dim, slots);
            }
            collect_operand_slot(*value, slots);
        }
        DenseOperands::ForeachInit { iterator, source } => {
            slots.insert(OsrVmSlot::Iterator(*iterator));
            collect_operand_slot(*source, slots);
        }
        DenseOperands::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            slots.insert(OsrVmSlot::Register(*has_value));
            slots.insert(OsrVmSlot::Iterator(*iterator));
            if let Some(key) = key {
                slots.insert(OsrVmSlot::Register(*key));
            }
            slots.insert(OsrVmSlot::Register(*value));
        }
        DenseOperands::FetchProperty {
            dst,
            object,
            property: _,
        } => {
            slots.insert(OsrVmSlot::Register(*dst));
            collect_operand_slot(*object, slots);
        }
        DenseOperands::AssignProperty {
            dst,
            object,
            property: _,
            value,
        } => {
            slots.insert(OsrVmSlot::Register(*dst));
            collect_operand_slot(*object, slots);
            collect_operand_slot(*value, slots);
        }
        DenseOperands::Operand { src } => {
            collect_operand_slot(*src, slots);
        }
        DenseOperands::Jump { .. } => {}
        DenseOperands::JumpIf { condition, .. } => {
            collect_operand_slot(*condition, slots);
        }
        DenseOperands::JumpIfElse { condition, .. } => {
            collect_operand_slot(*condition, slots);
        }
        DenseOperands::Return { value } => {
            if let Some(value) = value {
                collect_operand_slot(*value, slots);
            }
        }
        DenseOperands::Exit { value } => {
            if let Some(value) = value {
                collect_operand_slot(*value, slots);
            }
        }
    }
}

fn collect_operand_slot(operand: crate::bytecode::DenseOperand, slots: &mut BTreeSet<OsrVmSlot>) {
    match operand.kind {
        DenseOperandKind::Register => {
            slots.insert(OsrVmSlot::Register(operand.index));
        }
        DenseOperandKind::Local => {
            slots.insert(OsrVmSlot::Local(operand.index));
        }
        DenseOperandKind::Constant => {}
    }
}

fn loop_instructions(
    function: &DenseFunction,
    header: u32,
    backedge: u32,
) -> impl Iterator<Item = &DenseInstruction> {
    let header_pos = function.blocks.iter().position(|block| block.id == header);
    let backedge_pos = function
        .blocks
        .iter()
        .position(|block| block.id == backedge);
    let (Some(header_pos), Some(backedge_pos)) = (header_pos, backedge_pos) else {
        return Vec::new().into_iter();
    };
    if header_pos > backedge_pos {
        return Vec::new().into_iter();
    }
    let mut instructions = Vec::new();
    for block in &function.blocks[header_pos..=backedge_pos] {
        let first = block.first_instruction as usize;
        let last = block.terminator as usize;
        instructions.extend(function.instructions[first..=last].iter());
    }
    instructions.into_iter()
}

#[cfg(test)]
mod tests {
    use php_ir::source_map::IrSpan;

    use super::{
        OsrLoopStateAnnotations, OsrUnsupportedStateKind, OsrVmSlot, analyze_dense_osr_entries,
        analyze_dense_osr_entries_with_annotations,
    };
    use crate::bytecode::{
        DENSE_BYTECODE_VERSION, DenseBlock, DenseBytecodeUnit, DenseInstruction, DenseOpcode,
        DenseOperand, DenseOperandKind, DenseOperands, DenseSpanId,
    };

    #[test]
    fn osr_simple_counted_loop_is_representable() {
        let dense = counted_loop(
            DenseOpcode::BinaryAdd,
            DenseOperands::Binary {
                dst: 1,
                lhs: reg(1),
                rhs: constant(0),
            },
        );

        let map = analyze_dense_osr_entries(&dense);

        assert_eq!(map.report.osr_entry_candidates, 1);
        assert_eq!(map.report.osr_entry_representable, 1);
        assert_eq!(map.entries[0].loop_header_block, 1);
        assert_eq!(map.entries[0].fake_control_predecessor, Some(2));
        assert!(
            map.entries[0]
                .required_live_slots
                .iter()
                .any(|slot| slot.slot == OsrVmSlot::Local(0))
        );
    }

    #[test]
    fn osr_by_value_foreach_is_rejected_until_state_model_exists() {
        let dense = counted_loop(
            DenseOpcode::ForeachNext,
            DenseOperands::ForeachNext {
                has_value: 1,
                iterator: 0,
                key: None,
                value: 2,
            },
        );

        let map = analyze_dense_osr_entries(&dense);

        assert_eq!(map.report.osr_entry_representable, 0);
        assert_eq!(
            map.report
                .osr_entry_rejected_by_reason
                .get("foreach_state_model_missing"),
            Some(&1)
        );
    }

    #[test]
    fn osr_by_reference_foreach_annotation_is_rejected() {
        let dense = counted_loop(
            DenseOpcode::BinaryAdd,
            DenseOperands::Binary {
                dst: 1,
                lhs: reg(1),
                rhs: constant(0),
            },
        );
        let mut annotations = OsrLoopStateAnnotations::default();
        annotations.mark_unsupported(0, 1, OsrUnsupportedStateKind::ByReferenceForeach);

        let map = analyze_dense_osr_entries_with_annotations(&dense, &annotations);

        assert_eq!(map.report.osr_entry_representable, 0);
        assert_eq!(
            map.report
                .osr_entry_rejected_by_reason
                .get("by_ref_foreach_state"),
            Some(&1)
        );
    }

    #[test]
    fn osr_try_finally_generator_and_fiber_annotations_are_rejected() {
        let dense = counted_loop(
            DenseOpcode::BinaryAdd,
            DenseOperands::Binary {
                dst: 1,
                lhs: reg(1),
                rhs: constant(0),
            },
        );
        let mut annotations = OsrLoopStateAnnotations::default();
        annotations.mark_unsupported(0, 1, OsrUnsupportedStateKind::TryFinally);
        annotations.mark_unsupported(0, 1, OsrUnsupportedStateKind::Generator);
        annotations.mark_unsupported(0, 1, OsrUnsupportedStateKind::Fiber);

        let map = analyze_dense_osr_entries_with_annotations(&dense, &annotations);

        assert_eq!(map.report.osr_entry_representable, 0);
        assert!(
            map.entries[0]
                .unsupported_reasons
                .contains(&"try_finally_state".to_string())
        );
        assert!(
            map.entries[0]
                .unsupported_reasons
                .contains(&"generator_state".to_string())
        );
        assert!(
            map.entries[0]
                .unsupported_reasons
                .contains(&"fiber_state".to_string())
        );
    }

    #[test]
    fn osr_loop_with_dynamic_call_is_rejected() {
        let dense = counted_loop(
            DenseOpcode::CallFunction,
            DenseOperands::Call {
                dst: 1,
                name: 0,
                args: Vec::new(),
            },
        );

        let map = analyze_dense_osr_entries(&dense);

        assert_eq!(map.report.osr_entry_representable, 0);
        assert_eq!(
            map.report.osr_entry_rejected_by_reason.get("dynamic_call"),
            Some(&1)
        );
    }

    fn counted_loop(body_opcode: DenseOpcode, body_operands: DenseOperands) -> DenseBytecodeUnit {
        DenseBytecodeUnit {
            version: DENSE_BYTECODE_VERSION,
            constant_count: 1,
            file_count: 1,
            functions: vec![crate::bytecode::DenseFunction {
                name: "main".to_string(),
                register_count: 4,
                local_count: 1,
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
                        instruction_len: 2,
                        terminator: 3,
                    },
                    DenseBlock {
                        id: 3,
                        first_instruction: 4,
                        instruction_len: 1,
                        terminator: 4,
                    },
                ],
                instructions: vec![
                    DenseInstruction {
                        opcode: DenseOpcode::Jump,
                        operands: DenseOperands::Jump { target: 1 },
                        span: DenseSpanId::new(0),
                        cache_slot: None,
                    },
                    DenseInstruction {
                        opcode: DenseOpcode::JumpIfFalse,
                        operands: DenseOperands::JumpIf {
                            condition: reg(0),
                            target: 3,
                        },
                        span: DenseSpanId::new(0),
                        cache_slot: None,
                    },
                    DenseInstruction {
                        opcode: body_opcode,
                        operands: body_operands,
                        span: DenseSpanId::new(0),
                        cache_slot: None,
                    },
                    DenseInstruction {
                        opcode: DenseOpcode::Jump,
                        operands: DenseOperands::Jump { target: 1 },
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
            spans: vec![IrSpan::new(php_ir::FileId::new(0), 0, 1)],
            names: vec!["fn".to_string()],
            cache_slots: Vec::new(),
            source_map: Vec::new(),
        }
    }

    fn reg(index: u32) -> DenseOperand {
        DenseOperand {
            kind: DenseOperandKind::Register,
            index,
        }
    }

    fn constant(index: u32) -> DenseOperand {
        DenseOperand {
            kind: DenseOperandKind::Constant,
            index,
        }
    }
}
