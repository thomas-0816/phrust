//! Small builder API for constructing IR in tests and future lowering.

use crate::block::BasicBlock;
use crate::constants::IrConstant;
use crate::function::{FunctionFlags, IrCapture, IrFunction, IrParam, IrReturnType};
use crate::ids::{BlockId, ConstId, FileId, FunctionId, InstrId, LocalId, RegId, UnitId};
use crate::instruction::{Instruction, InstructionKind, Terminator, TerminatorKind};
use crate::module::{
    AttributeEntry, ClassEntry, FileEntry, FunctionEntry, GlobalConstantEntry, IrUnit,
};
use crate::operand::Operand;
use crate::source_map::{IrSourceMapTarget, IrSpan};

/// Builder for one IR unit.
#[derive(Debug)]
pub struct IrBuilder {
    unit: IrUnit,
    /// Normalized-name index over `unit.function_table` for O(1) call-site
    /// resolution during lowering.
    function_name_index: std::collections::HashMap<String, FunctionId>,
}

impl IrBuilder {
    /// Creates a new builder.
    #[must_use]
    pub fn new(id: UnitId) -> Self {
        Self {
            unit: IrUnit::new(id),
            function_name_index: std::collections::HashMap::new(),
        }
    }

    /// Adds a source file table entry.
    pub fn add_file(&mut self, path: impl Into<String>) -> FileId {
        let id = FileId::new(self.unit.files.len() as u32);
        self.unit.files.push(FileEntry {
            id,
            path: path.into(),
        });
        self.unit.file_strict_types.push(false);
        id
    }

    /// Records the strict-types mode for one source file.
    pub fn set_file_strict_types(&mut self, file: FileId, strict_types: bool) {
        if let Some(slot) = self.unit.file_strict_types.get_mut(file.index()) {
            *slot = strict_types;
        }
    }

    /// Returns a snapshot of classes already lowered into this unit.
    #[must_use]
    pub fn classes(&self) -> &[ClassEntry] {
        &self.unit.classes
    }

    /// Adds a constant and returns its ID.
    pub fn add_constant(&mut self, constant: IrConstant) -> ConstId {
        let id = ConstId::new(self.unit.constants.len() as u32);
        self.unit.constants.push(constant);
        id
    }

    /// Interns a constant in deterministic first-use order.
    pub fn intern_constant(&mut self, constant: IrConstant) -> ConstId {
        if let Some((index, _)) = self
            .unit
            .constants
            .iter()
            .enumerate()
            .find(|(_, existing)| **existing == constant)
        {
            return ConstId::new(index as u32);
        }
        self.add_constant(constant)
    }

    /// Starts a function and returns its ID.
    pub fn start_function(
        &mut self,
        name: impl Into<String>,
        flags: FunctionFlags,
        span: IrSpan,
    ) -> FunctionId {
        let id = FunctionId::new(self.unit.functions.len() as u32);
        self.unit.functions.push(IrFunction::new(name, flags, span));
        id
    }

    /// Registers a normalized function lookup name.
    pub fn register_function_name(&mut self, name: impl Into<String>, function: FunctionId) {
        let name = name.into();
        self.function_name_index.insert(name.clone(), function);
        self.unit
            .function_table
            .push(FunctionEntry { name, function });
    }

    /// Registers a runtime-visible global constant name.
    pub fn register_constant_name(
        &mut self,
        name: impl Into<String>,
        value: ConstId,
        span: IrSpan,
    ) {
        self.unit.constant_table.push(GlobalConstantEntry {
            name: name.into(),
            value,
            span,
        });
    }

    /// Registers a class table entry.
    pub fn push_class(&mut self, mut class: ClassEntry) {
        class.id = crate::ids::ClassId::new(self.unit.classes.len() as u32);
        self.unit.classes.push(class);
    }

    /// Appends function parameter metadata.
    pub fn push_param(&mut self, function: FunctionId, param: IrParam) {
        self.function_mut(function).params.push(param);
    }

    /// Appends closure capture metadata.
    pub fn push_capture(&mut self, function: FunctionId, capture: IrCapture) {
        self.function_mut(function).captures.push(capture);
    }

    /// Sets the function return type metadata.
    pub fn set_return_type(&mut self, function: FunctionId, return_type: Option<IrReturnType>) {
        self.function_mut(function).return_type = return_type;
    }

    /// Sets whether the function returns by reference.
    pub fn set_returns_by_ref(&mut self, function: FunctionId, returns_by_ref: bool) {
        self.function_mut(function).returns_by_ref = returns_by_ref;
    }

    /// Sets function-like attribute metadata.
    pub fn set_function_attributes(
        &mut self,
        function: FunctionId,
        attributes: Vec<AttributeEntry>,
    ) {
        self.function_mut(function).attributes = attributes;
    }

    /// Returns whether a function was declared to return by reference.
    #[must_use]
    pub fn returns_by_ref(&self, function: FunctionId) -> bool {
        self.unit.functions[function.index()].returns_by_ref
    }

    /// Returns the shape flags for a function.
    #[must_use]
    pub fn function_flags(&self, function: FunctionId) -> FunctionFlags {
        self.unit.functions[function.index()].flags
    }

    /// Resolves a registered (unconditional) unit function by normalized name.
    ///
    /// Constant-time: call lowering consults this per static call site, so a
    /// table scan would make lowering quadratic in unit size.
    #[must_use]
    pub fn registered_function(&self, normalized_name: &str) -> Option<FunctionId> {
        self.function_name_index.get(normalized_name).copied()
    }

    /// Returns the declared parameters for a function.
    #[must_use]
    pub fn function_params(&self, function: FunctionId) -> &[IrParam] {
        &self.unit.functions[function.index()].params
    }

    /// Compatibility helper for early manual IR tests.
    pub fn push_required_param(
        &mut self,
        function: FunctionId,
        name: impl Into<String>,
        local: LocalId,
    ) {
        self.function_mut(function).params.push(IrParam {
            name: name.into(),
            local,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        });
    }

    /// Appends an empty block to a function.
    pub fn append_block(&mut self, function: FunctionId) -> BlockId {
        let function = self.function_mut(function);
        let id = BlockId::new(function.blocks.len() as u32);
        function.blocks.push(BasicBlock::new(id));
        id
    }

    /// Allocates a register in a function.
    pub fn alloc_register(&mut self, function: FunctionId) -> RegId {
        let function = self.function_mut(function);
        let id = RegId::new(function.register_count);
        function.register_count += 1;
        id
    }

    /// Interns a named PHP local slot in deterministic first-use order.
    pub fn intern_local(&mut self, function: FunctionId, name: impl AsRef<str>) -> LocalId {
        let name = name.as_ref();
        let function = self.function_mut(function);
        if let Some((index, _)) = function
            .locals
            .iter()
            .enumerate()
            .find(|(_, existing)| existing.as_str() == name)
        {
            return LocalId::new(index as u32);
        }
        let id = LocalId::new(function.local_count);
        function.locals.push(name.to_owned());
        function.local_count += 1;
        id
    }

    /// Returns the local slot for an existing named PHP local.
    #[must_use]
    pub fn local_id(&self, function: FunctionId, name: &str) -> Option<LocalId> {
        self.unit.functions[function.index()]
            .locals
            .iter()
            .enumerate()
            .find_map(|(index, existing)| {
                (existing.as_str() == name).then(|| LocalId::new(index as u32))
            })
    }

    /// Emits `LoadConst`.
    pub fn emit_load_const(
        &mut self,
        function: FunctionId,
        block: BlockId,
        dst: RegId,
        constant: ConstId,
        span: IrSpan,
    ) -> InstrId {
        self.emit(
            function,
            block,
            InstructionKind::LoadConst { dst, constant },
            span,
        )
    }

    /// Emits an instruction kind.
    pub fn emit(
        &mut self,
        function: FunctionId,
        block: BlockId,
        kind: InstructionKind,
        span: IrSpan,
    ) -> InstrId {
        let block = self.block_mut(function, block);
        let id = InstrId::new(block.instructions.len() as u32);
        block.instructions.push(Instruction { id, span, kind });
        id
    }

    /// Returns true when the block already has a terminator.
    pub fn is_terminated(&mut self, function: FunctionId, block: BlockId) -> bool {
        self.block_mut(function, block).terminator.is_some()
    }

    /// Adds a source-map entry.
    pub fn add_source_map(
        &mut self,
        target: IrSourceMapTarget,
        origin: impl Into<String>,
        span: IrSpan,
    ) {
        self.unit.source_map.push(target, origin, span);
    }

    /// Sets a return terminator.
    pub fn terminate_return(
        &mut self,
        function: FunctionId,
        block: BlockId,
        value: Option<Operand>,
        span: IrSpan,
    ) {
        self.block_mut(function, block).terminator = Some(Terminator {
            span,
            kind: TerminatorKind::Return {
                value,
                by_ref_local: None,
            },
        });
    }

    /// Sets a return terminator that returns a local slot by reference.
    pub fn terminate_return_ref(
        &mut self,
        function: FunctionId,
        block: BlockId,
        local: LocalId,
        span: IrSpan,
    ) {
        self.block_mut(function, block).terminator = Some(Terminator {
            span,
            kind: TerminatorKind::Return {
                value: Some(Operand::Local(local)),
                by_ref_local: Some(local),
            },
        });
    }

    /// Sets a script-exit terminator.
    pub fn terminate_exit(
        &mut self,
        function: FunctionId,
        block: BlockId,
        value: Option<Operand>,
        span: IrSpan,
    ) {
        self.block_mut(function, block).terminator = Some(Terminator {
            span,
            kind: TerminatorKind::Exit { value },
        });
    }

    /// Sets an unconditional jump terminator.
    pub fn terminate_jump(
        &mut self,
        function: FunctionId,
        block: BlockId,
        target: BlockId,
        span: IrSpan,
    ) {
        self.block_mut(function, block).terminator = Some(Terminator {
            span,
            kind: TerminatorKind::Jump { target },
        });
    }

    /// Sets a conditional jump-if-false terminator.
    pub fn terminate_jump_if_false(
        &mut self,
        function: FunctionId,
        block: BlockId,
        condition: Operand,
        target: BlockId,
        span: IrSpan,
    ) {
        self.block_mut(function, block).terminator = Some(Terminator {
            span,
            kind: TerminatorKind::JumpIfFalse { condition, target },
        });
    }

    /// Sets a conditional jump-if-true terminator.
    pub fn terminate_jump_if_true(
        &mut self,
        function: FunctionId,
        block: BlockId,
        condition: Operand,
        target: BlockId,
        span: IrSpan,
    ) {
        self.block_mut(function, block).terminator = Some(Terminator {
            span,
            kind: TerminatorKind::JumpIfTrue { condition, target },
        });
    }

    /// Sets a conditional jump with explicit true and false targets.
    pub fn terminate_jump_if(
        &mut self,
        function: FunctionId,
        block: BlockId,
        condition: Operand,
        if_true: BlockId,
        if_false: BlockId,
        span: IrSpan,
    ) {
        self.block_mut(function, block).terminator = Some(Terminator {
            span,
            kind: TerminatorKind::JumpIf {
                condition,
                if_true,
                if_false,
            },
        });
    }

    /// Sets the entry function.
    pub fn set_entry(&mut self, function: FunctionId) {
        self.unit.entry = function;
    }

    /// Records top-level linked-file functions in dependency execution order.
    pub fn set_linked_file_entries(&mut self, entries: Vec<FunctionId>) {
        self.unit.linked_file_entries = entries;
    }

    /// Records, index-aligned with the linked file entries, which linked files
    /// require runtime autoload activation and under which declaration name.
    pub fn set_linked_entry_autoload_declarations(&mut self, declarations: Vec<Option<String>>) {
        self.unit.linked_entry_autoload_declarations = declarations;
    }

    /// Sets file-level `declare(strict_types=1)` metadata for this IR unit.
    pub fn set_strict_types(&mut self, strict_types: bool) {
        self.unit.strict_types = strict_types;
    }

    /// Finishes the unit.
    #[must_use]
    pub fn finish(self) -> IrUnit {
        self.unit
    }

    fn function_mut(&mut self, id: FunctionId) -> &mut IrFunction {
        &mut self.unit.functions[id.index()]
    }

    fn block_mut(&mut self, function: FunctionId, block: BlockId) -> &mut BasicBlock {
        &mut self.function_mut(function).blocks[block.index()]
    }
}
