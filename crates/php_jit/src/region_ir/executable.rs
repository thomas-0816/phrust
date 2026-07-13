//! Structured executable Region IR lowered from `php_ir`.

use php_ir::instruction::{IrCallArg, IrCallArgValueKind, TerminatorKind};
use php_ir::{
    AttributeEntry, BinaryOp, BlockId, ClassMethodEntry, CompareOp, FunctionEntry, FunctionFlags,
    FunctionId, InstrId, InstructionKind, IrCapture, IrConstant, IrParam, IrReturnType, IrSpan,
    IrUnit, LocalId, Operand, RegId,
};
use std::collections::BTreeSet;

/// A typed failure while constructing an executable region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCompileError {
    pub code: &'static str,
    pub detail: String,
}

impl NativeCompileError {
    pub(crate) fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// Native compiler tier represented by a Region IR graph.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NativeCompilerTier {
    /// Exhaustive, non-speculative lowering without profile feedback.
    #[default]
    Baseline,
    /// Guarded transformations layered on top of the baseline graph.
    Optimizing,
}

/// Runtime-owned identities that affect native code generation and caching.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompileMetadata {
    pub ir_fingerprint: String,
    pub tier: NativeCompilerTier,
    pub helper_abi_hash: u64,
    pub target_cpu: String,
    pub semantic_config_hash: u64,
    pub dependency_identity: String,
}

/// Class/method identity retained for method functions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionMethodIdentity {
    pub class_name: String,
    pub class_display_name: String,
    pub method: ClassMethodEntry,
}

/// Declaration-table identity retained next to a function body.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegionDeclarationMetadata {
    pub named_function: Option<FunctionEntry>,
    pub method: Option<RegionMethodIdentity>,
}

/// Exception-handler region declared by an `EnterTry` operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionExceptionRegion {
    pub block: BlockId,
    pub instruction: InstrId,
    pub span: IrSpan,
    pub catch: Option<BlockId>,
    pub catch_types: Vec<String>,
    pub finally: Option<BlockId>,
    pub after: BlockId,
    pub exception_local: Option<LocalId>,
}

impl std::fmt::Display for NativeCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for NativeCompileError {}

/// Scalar binary operations currently executable without a runtime helper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionBinaryOp {
    Add,
    Sub,
    Mul,
}

/// Scalar comparison operations currently executable without a runtime helper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionCompareOpCode {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

/// Region operand detached from the source unit's constant pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionOperand {
    Register(RegId),
    Local(LocalId),
    I64(i64),
}

/// Destination written by one unified native call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionCallResult {
    Register(RegId),
    ReferenceLocal(LocalId),
}

/// Typed target resolved by a direct indirection entry or the native trampoline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionCallTarget {
    Function {
        name: String,
        function: Option<FunctionId>,
    },
    Method {
        receiver: Operand,
        method: String,
    },
    StaticMethod {
        class_name: String,
        method: String,
    },
    Closure {
        callee: Operand,
    },
    Callable {
        callee: Operand,
    },
    Pipe {
        callable: Operand,
    },
    Constructor {
        display_class_name: String,
        class_name: String,
    },
    DynamicConstructor {
        class_name: Operand,
    },
}

/// One call-site contract. Argument metadata remains typed and is materialized
/// directly into native slots during lowering, never into VM call objects.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionNativeCall {
    pub result: RegionCallResult,
    pub target: RegionCallTarget,
    pub args: Vec<IrCallArg>,
    /// Compile-time scalar operands for direct-slot materialization. `None`
    /// selects the native binder/trampoline for that argument.
    pub operands: Vec<Option<RegionOperand>>,
    pub direct_arity: Option<u32>,
    pub returns_by_reference: bool,
    pub caller_strict_types: bool,
}

/// Explicit PHP control operation lowered into generated code. These variants
/// never request bytecode/IR interpreter exception dispatch.
#[derive(Clone, Debug, PartialEq)]
pub enum RegionNativeControl {
    EnterTry {
        handler_index: u32,
    },
    LeaveTry,
    EndFinally {
        after: BlockId,
        outer_finally: Option<BlockId>,
    },
    Throw {
        value: RegionOperand,
    },
    MakeException {
        dst: RegId,
        class_name: String,
        message: Option<RegionOperand>,
    },
}

impl RegionNativeCall {
    /// Returns the fixed-arity userland callee for the allocation-free path.
    /// Every other call shape is resolved by the typed native trampoline.
    #[must_use]
    pub fn direct_compiled_target(&self) -> Option<FunctionId> {
        let RegionCallTarget::Function {
            function: Some(function),
            ..
        } = self.target
        else {
            return None;
        };
        (!self.returns_by_reference
            && self.direct_arity == u32::try_from(self.args.len()).ok()
            && self.operands.iter().all(Option::is_some)
            && self.args.iter().all(|arg| {
                arg.name.is_none() && !arg.unpack && arg.value_kind == IrCallArgValueKind::Direct
            }))
        .then_some(function)
    }
}

/// One executable Region IR instruction.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionInstruction {
    pub id: InstrId,
    pub span: IrSpan,
    /// Stable continuation ID used by native PC/deopt metadata.
    pub continuation_id: u32,
    /// Locals definitely initialized immediately before this instruction.
    pub live_locals: Vec<LocalId>,
    /// Authoritative instruction, retained even when native lowering is missing.
    pub source_kind: InstructionKind,
    pub kind: RegionInstructionKind,
}

/// Instruction kinds in the initial general scalar region.
#[derive(Clone, Debug, PartialEq)]
pub enum RegionInstructionKind {
    Nop,
    Move {
        dst: RegId,
        src: RegionOperand,
    },
    LoadLocal {
        dst: RegId,
        local: LocalId,
    },
    StoreLocal {
        local: LocalId,
        src: RegionOperand,
    },
    Discard {
        src: RegionOperand,
    },
    Binary {
        dst: RegId,
        op: RegionBinaryOp,
        lhs: RegionOperand,
        rhs: RegionOperand,
    },
    Compare {
        dst: RegId,
        op: RegionCompareOpCode,
        lhs: RegionOperand,
        rhs: RegionOperand,
    },
    NativeCall(RegionNativeCall),
    NativeControl(RegionNativeControl),
    /// Explicit fatal produced by IR lowering; native code returns fatal status.
    RuntimeFatal {
        diagnostic_id: String,
        message: String,
    },
    /// Explicit unsupported-feature fatal emitted by the frontend.
    CompileTimeFatal {
        diagnostic_id: String,
    },
    /// The semantic graph is complete, but Cranelift has no lowering yet.
    MissingLowering,
}

/// Explicit control flow for one executable region block.
#[derive(Clone, Debug, PartialEq)]
pub enum RegionTerminator {
    Jump {
        target: BlockId,
    },
    JumpIfFalse {
        condition: RegionOperand,
        target: BlockId,
        fallthrough: BlockId,
    },
    JumpIfTrue {
        condition: RegionOperand,
        target: BlockId,
        fallthrough: BlockId,
    },
    JumpIf {
        condition: RegionOperand,
        if_true: BlockId,
        if_false: BlockId,
    },
    Return {
        value: RegionOperand,
        finally: Option<BlockId>,
    },
    ReturnReference {
        local: LocalId,
        finally: Option<BlockId>,
    },
    Exit {
        value: Option<RegionOperand>,
        finally: Option<BlockId>,
    },
    /// The semantic graph is complete, but Cranelift has no lowering yet.
    MissingLowering,
}

/// One basic block in an executable region.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionBlock {
    pub id: BlockId,
    pub entry_live_locals: Vec<LocalId>,
    pub instructions: Vec<RegionInstruction>,
    pub terminator_span: IrSpan,
    pub terminator_continuation_id: u32,
    pub terminator_live_locals: Vec<LocalId>,
    /// Authoritative terminator retained for effect and exception semantics.
    pub source_terminator: TerminatorKind,
    pub terminator: RegionTerminator,
}

/// A native OSR entry at a loop header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionOsrEntryPoint {
    pub id: u32,
    pub block: BlockId,
    pub continuation_id: u32,
    pub live_locals: Vec<LocalId>,
}

/// A verified, multi-block Region IR function ready for backend lowering.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionGraph {
    pub function: FunctionId,
    pub function_name: String,
    pub function_span: IrSpan,
    pub flags: FunctionFlags,
    pub strict_types: bool,
    pub params: Vec<IrParam>,
    pub locals: Vec<String>,
    pub captures: Vec<IrCapture>,
    pub return_type: Option<IrReturnType>,
    pub returns_by_ref: bool,
    pub attributes: Vec<AttributeEntry>,
    pub declarations: RegionDeclarationMetadata,
    pub exception_regions: Vec<RegionExceptionRegion>,
    pub compile_metadata: CompileMetadata,
    pub parameter_locals: Vec<LocalId>,
    pub local_count: u32,
    pub register_count: u32,
    pub blocks: Vec<RegionBlock>,
    pub fast_path_operations: u64,
}

impl RegionGraph {
    #[must_use]
    pub fn arity(&self) -> usize {
        self.parameter_locals.len()
    }

    #[must_use]
    pub fn has_control_flow(&self) -> bool {
        self.blocks.len() > 1
    }

    /// Returns one stable OSR entry for every loop header targeted by a backedge.
    #[must_use]
    pub fn osr_entries(&self) -> Vec<RegionOsrEntryPoint> {
        let mut headers = BTreeSet::new();
        for block in &self.blocks {
            for target in block.terminator.targets() {
                if target.raw() <= block.id.raw() {
                    headers.insert(target);
                }
            }
        }
        headers
            .into_iter()
            .enumerate()
            .filter_map(|(id, block)| {
                let region_block = self.blocks.get(block.index())?;
                let continuation_id = region_block
                    .instructions
                    .first()
                    .map(|instruction| instruction.continuation_id)
                    .unwrap_or(region_block.terminator_continuation_id);
                Some(RegionOsrEntryPoint {
                    id: id as u32,
                    block,
                    continuation_id,
                    live_locals: region_block.entry_live_locals.clone(),
                })
            })
            .collect()
    }

    /// Direct userland callees referenced by this region.
    #[must_use]
    pub fn direct_callees(&self) -> Vec<FunctionId> {
        let mut callees = BTreeSet::new();
        for block in &self.blocks {
            for instruction in &block.instructions {
                if let RegionInstructionKind::NativeCall(call) = &instruction.kind
                    && let Some(target) = call.direct_compiled_target()
                {
                    callees.insert(target);
                }
            }
        }
        callees.into_iter().collect()
    }

    #[must_use]
    pub fn has_native_trampoline_calls(&self) -> bool {
        self.blocks.iter().any(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call) if call.direct_compiled_target().is_none())
            })
        })
    }

    /// Verifies dense IDs and all explicit CFG targets.
    pub fn verify(&self) -> Result<(), NativeCompileError> {
        if self.blocks.is_empty() {
            return Err(NativeCompileError::new(
                "JIT_REGION_REJECT_EMPTY",
                "executable region has no blocks",
            ));
        }
        for (index, block) in self.blocks.iter().enumerate() {
            if block.id.index() != index {
                return Err(NativeCompileError::new(
                    "JIT_REGION_REJECT_BLOCK_IDS",
                    format!("block {} appears at position {index}", block.id.raw()),
                ));
            }
            for target in block.terminator.targets() {
                if target.index() >= self.blocks.len() {
                    return Err(NativeCompileError::new(
                        "JIT_REGION_REJECT_TARGET",
                        format!(
                            "block {} targets missing block {}",
                            block.id.raw(),
                            target.raw()
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl RegionTerminator {
    fn targets(&self) -> Vec<BlockId> {
        match self {
            Self::Jump { target } => vec![*target],
            Self::JumpIfFalse {
                target,
                fallthrough,
                ..
            }
            | Self::JumpIfTrue {
                target,
                fallthrough,
                ..
            } => vec![*target, *fallthrough],
            Self::JumpIf {
                if_true, if_false, ..
            } => vec![*if_true, *if_false],
            Self::Return { .. } | Self::ReturnReference { .. } | Self::Exit { .. } => Vec::new(),
            Self::MissingLowering => Vec::new(),
        }
    }
}

/// Builds exhaustive baseline Region IR from authoritative PHP IR.
pub struct BaselineRegionBuilder;

impl BaselineRegionBuilder {
    pub fn build(
        unit: &IrUnit,
        function: FunctionId,
        runtime_metadata: &CompileMetadata,
    ) -> Result<RegionGraph, NativeCompileError> {
        let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
            NativeCompileError::new(
                "JIT_REGION_REJECT_MISSING_FUNCTION",
                format!("function id {} is not present", function.raw()),
            )
        })?;
        let mut fast_path_operations = 0_u64;
        let mut blocks = Vec::with_capacity(ir_function.blocks.len());
        let mut next_continuation = 0_u32;
        for (block_index, block) in ir_function.blocks.iter().enumerate() {
            let mut instructions = Vec::with_capacity(block.instructions.len());
            for instruction in &block.instructions {
                let kind = match &instruction.kind {
                    InstructionKind::Nop => RegionInstructionKind::Nop,
                    InstructionKind::LoadConst { dst, constant } => lower_constant(unit, *constant)
                        .map_or(RegionInstructionKind::MissingLowering, |src| {
                            RegionInstructionKind::Move { dst: *dst, src }
                        }),
                    InstructionKind::Move { dst, src } => lower_operand(unit, *src)
                        .map_or(RegionInstructionKind::MissingLowering, |src| {
                            RegionInstructionKind::Move { dst: *dst, src }
                        }),
                    InstructionKind::LoadLocal { dst, local }
                    | InstructionKind::LoadLocalQuiet { dst, local } => {
                        RegionInstructionKind::LoadLocal {
                            dst: *dst,
                            local: *local,
                        }
                    }
                    InstructionKind::StoreLocal { local, src } => lower_operand(unit, *src)
                        .map_or(RegionInstructionKind::MissingLowering, |src| {
                            RegionInstructionKind::StoreLocal { local: *local, src }
                        }),
                    InstructionKind::Discard { src } => lower_operand(unit, *src)
                        .map_or(RegionInstructionKind::MissingLowering, |src| {
                            RegionInstructionKind::Discard { src }
                        }),
                    InstructionKind::Binary { dst, op, lhs, rhs } => {
                        if let (Ok(op), Ok(lhs), Ok(rhs)) = (
                            lower_binary(*op),
                            lower_operand(unit, *lhs),
                            lower_operand(unit, *rhs),
                        ) {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            RegionInstructionKind::Binary {
                                dst: *dst,
                                op,
                                lhs,
                                rhs,
                            }
                        } else {
                            RegionInstructionKind::MissingLowering
                        }
                    }
                    InstructionKind::Compare { dst, op, lhs, rhs } => {
                        if let (Ok(op), Ok(lhs), Ok(rhs)) = (
                            lower_compare(*op),
                            lower_operand(unit, *lhs),
                            lower_operand(unit, *rhs),
                        ) {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            RegionInstructionKind::Compare {
                                dst: *dst,
                                op,
                                lhs,
                                rhs,
                            }
                        } else {
                            RegionInstructionKind::MissingLowering
                        }
                    }
                    InstructionKind::CallFunction { dst, name, args } => {
                        let function = unit
                            .function_table
                            .iter()
                            .find(|entry| entry.name == *name)
                            .map(|entry| entry.function);
                        if function.is_some() {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                        }
                        let direct_arity = function.and_then(|function| {
                            unit.functions
                                .get(function.index())
                                .and_then(|target| u32::try_from(target.params.len()).ok())
                        });
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Function {
                                name: name.clone(),
                                function,
                            },
                            args: args.clone(),
                            operands: lower_call_operands(unit, args),
                            direct_arity,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::CallMethod {
                        dst,
                        object,
                        method,
                        args,
                    } => RegionInstructionKind::NativeCall(RegionNativeCall {
                        result: RegionCallResult::Register(*dst),
                        target: RegionCallTarget::Method {
                            receiver: *object,
                            method: method.clone(),
                        },
                        args: args.clone(),
                        operands: lower_call_operands(unit, args),
                        direct_arity: None,
                        returns_by_reference: false,
                        caller_strict_types: unit.strict_types,
                    }),
                    InstructionKind::CallStaticMethod {
                        dst,
                        class_name,
                        method,
                        args,
                    } => RegionInstructionKind::NativeCall(RegionNativeCall {
                        result: RegionCallResult::Register(*dst),
                        target: RegionCallTarget::StaticMethod {
                            class_name: class_name.clone(),
                            method: method.clone(),
                        },
                        args: args.clone(),
                        operands: lower_call_operands(unit, args),
                        direct_arity: None,
                        returns_by_reference: false,
                        caller_strict_types: unit.strict_types,
                    }),
                    InstructionKind::CallClosure { dst, callee, args } => {
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Closure { callee: *callee },
                            args: args.clone(),
                            operands: lower_call_operands(unit, args),
                            direct_arity: None,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::CallCallable { dst, callee, args } => {
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Callable { callee: *callee },
                            args: args.clone(),
                            operands: lower_call_operands(unit, args),
                            direct_arity: None,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::Pipe {
                        dst,
                        input,
                        callable,
                    } => RegionInstructionKind::NativeCall(RegionNativeCall {
                        result: RegionCallResult::Register(*dst),
                        target: RegionCallTarget::Pipe {
                            callable: *callable,
                        },
                        args: vec![IrCallArg {
                            name: None,
                            value: *input,
                            unpack: false,
                            value_kind: IrCallArgValueKind::Direct,
                            by_ref_local: None,
                            by_ref_dim: None,
                            by_ref_property: None,
                            by_ref_property_dim: None,
                        }],
                        operands: vec![lower_operand(unit, *input).ok()],
                        direct_arity: None,
                        returns_by_reference: false,
                        caller_strict_types: unit.strict_types,
                    }),
                    InstructionKind::BindReferenceFromCall { target, name, args } => {
                        let function = unit
                            .function_table
                            .iter()
                            .find(|entry| entry.name == *name)
                            .map(|entry| entry.function);
                        let direct_arity = function.and_then(|function| {
                            unit.functions
                                .get(function.index())
                                .and_then(|target| u32::try_from(target.params.len()).ok())
                        });
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::ReferenceLocal(*target),
                            target: RegionCallTarget::Function {
                                name: name.clone(),
                                function,
                            },
                            args: args.clone(),
                            operands: lower_call_operands(unit, args),
                            direct_arity,
                            returns_by_reference: true,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::BindReferenceFromMethodCall {
                        target,
                        object,
                        method,
                        args,
                    } => RegionInstructionKind::NativeCall(RegionNativeCall {
                        result: RegionCallResult::ReferenceLocal(*target),
                        target: RegionCallTarget::Method {
                            receiver: *object,
                            method: method.clone(),
                        },
                        args: args.clone(),
                        operands: lower_call_operands(unit, args),
                        direct_arity: None,
                        returns_by_reference: true,
                        caller_strict_types: unit.strict_types,
                    }),
                    InstructionKind::NewObject {
                        dst,
                        display_class_name,
                        class_name,
                        args,
                    } => RegionInstructionKind::NativeCall(RegionNativeCall {
                        result: RegionCallResult::Register(*dst),
                        target: RegionCallTarget::Constructor {
                            display_class_name: display_class_name.clone(),
                            class_name: class_name.clone(),
                        },
                        args: args.clone(),
                        operands: lower_call_operands(unit, args),
                        direct_arity: None,
                        returns_by_reference: false,
                        caller_strict_types: unit.strict_types,
                    }),
                    InstructionKind::DynamicNewObject {
                        dst,
                        class_name,
                        args,
                    } => RegionInstructionKind::NativeCall(RegionNativeCall {
                        result: RegionCallResult::Register(*dst),
                        target: RegionCallTarget::DynamicConstructor {
                            class_name: *class_name,
                        },
                        args: args.clone(),
                        operands: lower_call_operands(unit, args),
                        direct_arity: None,
                        returns_by_reference: false,
                        caller_strict_types: unit.strict_types,
                    }),
                    InstructionKind::EnterTry { .. } => {
                        let handler_index = collect_exception_regions(ir_function)
                            .iter()
                            .position(|region| region.instruction == instruction.id)
                            .and_then(|index| u32::try_from(index).ok())
                            .unwrap_or(u32::MAX);
                        RegionInstructionKind::NativeControl(RegionNativeControl::EnterTry {
                            handler_index,
                        })
                    }
                    InstructionKind::LeaveTry => {
                        RegionInstructionKind::NativeControl(RegionNativeControl::LeaveTry)
                    }
                    InstructionKind::EndFinally { after } => {
                        RegionInstructionKind::NativeControl(RegionNativeControl::EndFinally {
                            after: *after,
                            outer_finally: None,
                        })
                    }
                    InstructionKind::Throw { value } => lower_operand(unit, *value).map_or(
                        RegionInstructionKind::MissingLowering,
                        |value| {
                            RegionInstructionKind::NativeControl(RegionNativeControl::Throw {
                                value,
                            })
                        },
                    ),
                    InstructionKind::MakeException {
                        dst,
                        class_name,
                        message,
                    } => RegionInstructionKind::NativeControl(RegionNativeControl::MakeException {
                        dst: *dst,
                        class_name: class_name.clone(),
                        message: lower_operand(unit, *message).ok(),
                    }),
                    InstructionKind::RuntimeError {
                        diagnostic_id,
                        message,
                    } => RegionInstructionKind::RuntimeFatal {
                        diagnostic_id: diagnostic_id.clone(),
                        message: message.clone(),
                    },
                    InstructionKind::Unsupported { diagnostic_id } => {
                        RegionInstructionKind::CompileTimeFatal {
                            diagnostic_id: diagnostic_id.clone(),
                        }
                    }
                    InstructionKind::FetchConst { .. }
                    | InstructionKind::RegisterConstant { .. }
                    | InstructionKind::DeclareFunction { .. }
                    | InstructionKind::DeclareClass { .. }
                    | InstructionKind::BindReference { .. }
                    | InstructionKind::BindGlobal { .. }
                    | InstructionKind::BindReferenceDim { .. }
                    | InstructionKind::BindReferenceProperty { .. }
                    | InstructionKind::BindReferencePropertyDim { .. }
                    | InstructionKind::BindReferenceDimFromProperty { .. }
                    | InstructionKind::BindReferenceFromProperty { .. }
                    | InstructionKind::BindReferenceFromPropertyDim { .. }
                    | InstructionKind::BindReferenceFromDim { .. }
                    | InstructionKind::BindReferenceFromStaticPropertyDim { .. }
                    | InstructionKind::BindReferenceStaticProperty { .. }
                    | InstructionKind::InitStaticLocal { .. }
                    | InstructionKind::InstanceOf { .. }
                    | InstructionKind::DynamicInstanceOf { .. }
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Cast { .. }
                    | InstructionKind::Echo { .. }
                    | InstructionKind::EmitDiagnostic { .. }
                    | InstructionKind::Yield { .. }
                    | InstructionKind::YieldFrom { .. }
                    | InstructionKind::CloneObject { .. }
                    | InstructionKind::CloneWith { .. }
                    | InstructionKind::MakeClosure { .. }
                    | InstructionKind::ResolveCallable { .. }
                    | InstructionKind::AcquireCallable { .. }
                    | InstructionKind::Include { .. }
                    | InstructionKind::Eval { .. }
                    | InstructionKind::FetchProperty { .. }
                    | InstructionKind::FetchDynamicProperty { .. }
                    | InstructionKind::IssetProperty { .. }
                    | InstructionKind::IssetDynamicProperty { .. }
                    | InstructionKind::EmptyProperty { .. }
                    | InstructionKind::EmptyDynamicProperty { .. }
                    | InstructionKind::IssetDynamicPropertyDim { .. }
                    | InstructionKind::EmptyDynamicPropertyDim { .. }
                    | InstructionKind::IssetPropertyDim { .. }
                    | InstructionKind::EmptyPropertyDim { .. }
                    | InstructionKind::UnsetProperty { .. }
                    | InstructionKind::UnsetPropertyDim { .. }
                    | InstructionKind::UnsetDynamicProperty { .. }
                    | InstructionKind::FetchStaticProperty { .. }
                    | InstructionKind::FetchDynamicStaticProperty { .. }
                    | InstructionKind::IssetStaticProperty { .. }
                    | InstructionKind::EmptyStaticProperty { .. }
                    | InstructionKind::IssetStaticPropertyDim { .. }
                    | InstructionKind::EmptyStaticPropertyDim { .. }
                    | InstructionKind::UnsetStaticPropertyDim { .. }
                    | InstructionKind::FetchClassConstant { .. }
                    | InstructionKind::FetchObjectClassName { .. }
                    | InstructionKind::AssignProperty { .. }
                    | InstructionKind::AssignPropertyDim { .. }
                    | InstructionKind::AssignDynamicProperty { .. }
                    | InstructionKind::AssignStaticProperty { .. }
                    | InstructionKind::AssignDynamicStaticProperty { .. }
                    | InstructionKind::NewArray { .. }
                    | InstructionKind::ArrayInsert { .. }
                    | InstructionKind::ArraySpread { .. }
                    | InstructionKind::FetchDim { .. }
                    | InstructionKind::AssignDim { .. }
                    | InstructionKind::AppendDim { .. }
                    | InstructionKind::IssetLocal { .. }
                    | InstructionKind::EmptyLocal { .. }
                    | InstructionKind::UnsetLocal { .. }
                    | InstructionKind::IssetDim { .. }
                    | InstructionKind::EmptyDim { .. }
                    | InstructionKind::UnsetDim { .. }
                    | InstructionKind::ForeachInit { .. }
                    | InstructionKind::ForeachNext { .. }
                    | InstructionKind::ForeachCleanup { .. }
                    | InstructionKind::ForeachInitRef { .. }
                    | InstructionKind::ForeachNextRef { .. }
                    | InstructionKind::ArrayGet { .. } => RegionInstructionKind::MissingLowering,
                };
                instructions.push(RegionInstruction {
                    id: instruction.id,
                    span: instruction.span,
                    continuation_id: next_continuation,
                    live_locals: Vec::new(),
                    source_kind: instruction.kind.clone(),
                    kind,
                });
                next_continuation = next_continuation.saturating_add(1);
            }
            let source_terminator = block.terminator.as_ref().ok_or_else(|| {
                NativeCompileError::new(
                    "JIT_REGION_REJECT_TERMINATOR",
                    format!("block {} has no terminator", block.id.raw()),
                )
            })?;
            let terminator = lower_terminator(unit, ir_function.blocks.len(), block_index, block)
                .unwrap_or(RegionTerminator::MissingLowering);
            let terminator_span = source_terminator.span;
            blocks.push(RegionBlock {
                id: block.id,
                entry_live_locals: Vec::new(),
                instructions,
                terminator_span,
                terminator_continuation_id: next_continuation,
                terminator_live_locals: Vec::new(),
                source_terminator: source_terminator.kind.clone(),
                terminator,
            });
            next_continuation = next_continuation.saturating_add(1);
        }
        populate_live_locals(
            &mut blocks,
            &ir_function
                .params
                .iter()
                .map(|param| param.local)
                .collect::<Vec<_>>(),
        );
        let exception_regions = collect_exception_regions(ir_function);
        annotate_native_finally_control(&mut blocks, &exception_regions);
        let region = RegionGraph {
            function,
            function_name: ir_function.name.clone(),
            function_span: ir_function.span,
            flags: ir_function.flags,
            strict_types: unit.strict_types_for_function(function),
            params: ir_function.params.clone(),
            locals: ir_function.locals.clone(),
            captures: ir_function.captures.clone(),
            return_type: ir_function.return_type.clone(),
            returns_by_ref: ir_function.returns_by_ref,
            attributes: ir_function.attributes.clone(),
            declarations: declaration_metadata(unit, function),
            exception_regions,
            compile_metadata: runtime_metadata.clone(),
            parameter_locals: ir_function.params.iter().map(|param| param.local).collect(),
            local_count: ir_function.local_count,
            register_count: ir_function.register_count,
            blocks,
            fast_path_operations,
        };
        region.verify()?;
        Ok(region)
    }
}

fn collect_exception_regions(ir_function: &php_ir::IrFunction) -> Vec<RegionExceptionRegion> {
    ir_function
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
        .collect()
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
                    *outer_finally = stack
                        .iter()
                        .rev()
                        .filter_map(|index| handlers.get(*index as usize))
                        .find_map(|handler| handler.finally);
                }
                _ => {}
            }
        }
        let pending_finally = stack
            .iter()
            .rev()
            .filter_map(|index| handlers.get(*index as usize))
            .find_map(|handler| handler.finally);
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

/// Compatibility wrapper for callers that do not yet own runtime metadata.
pub fn build_baseline_region(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<RegionGraph, NativeCompileError> {
    BaselineRegionBuilder::build(unit, function, &CompileMetadata::default())
}

fn declaration_metadata(unit: &IrUnit, function: FunctionId) -> RegionDeclarationMetadata {
    let named_function = unit
        .function_table
        .iter()
        .find(|entry| entry.function == function)
        .cloned();
    let method = unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == function)
            .cloned()
            .map(|method| RegionMethodIdentity {
                class_name: class.name.clone(),
                class_display_name: class.display_name.clone(),
                method,
            })
    });
    RegionDeclarationMetadata {
        named_function,
        method,
    }
}

fn populate_live_locals(blocks: &mut [RegionBlock], params: &[LocalId]) {
    let mut candidates = params.iter().copied().collect::<BTreeSet<_>>();
    let mut definitions = Vec::with_capacity(blocks.len());
    let mut predecessors = vec![Vec::<usize>::new(); blocks.len()];
    for block in blocks.iter() {
        let mut defs = BTreeSet::new();
        for instruction in &block.instructions {
            if let RegionInstructionKind::StoreLocal { local, .. } = instruction.kind {
                defs.insert(local);
                candidates.insert(local);
            }
        }
        definitions.push(defs);
        for target in block.terminator.targets() {
            if let Some(target_predecessors) = predecessors.get_mut(target.index()) {
                target_predecessors.push(block.id.index());
            }
        }
    }

    let entry = params.iter().copied().collect::<BTreeSet<_>>();
    let mut initialized_in = vec![candidates.clone(); blocks.len()];
    if let Some(first) = initialized_in.first_mut() {
        *first = entry;
    }
    loop {
        let initialized_out = initialized_in
            .iter()
            .zip(&definitions)
            .map(|(incoming, defs)| incoming.union(defs).copied().collect::<BTreeSet<_>>())
            .collect::<Vec<_>>();
        let mut changed = false;
        for block_index in 1..blocks.len() {
            let Some((first, rest)) = predecessors[block_index].split_first() else {
                continue;
            };
            let mut incoming = initialized_out[*first].clone();
            for predecessor in rest {
                incoming = incoming
                    .intersection(&initialized_out[*predecessor])
                    .copied()
                    .collect();
            }
            if initialized_in[block_index] != incoming {
                initialized_in[block_index] = incoming;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for (block, incoming) in blocks.iter_mut().zip(initialized_in) {
        let mut initialized = incoming;
        block.entry_live_locals = initialized.iter().copied().collect();
        for instruction in &mut block.instructions {
            instruction.live_locals = initialized.iter().copied().collect();
            if let RegionInstructionKind::StoreLocal { local, .. } = instruction.kind {
                initialized.insert(local);
            }
        }
        block.terminator_live_locals = initialized.into_iter().collect();
    }
}

fn lower_binary(op: BinaryOp) -> Result<RegionBinaryOp, NativeCompileError> {
    match op {
        BinaryOp::Add => Ok(RegionBinaryOp::Add),
        BinaryOp::Sub => Ok(RegionBinaryOp::Sub),
        BinaryOp::Mul => Ok(RegionBinaryOp::Mul),
        BinaryOp::Div => Err(NativeCompileError::new(
            "JIT_REGION_REJECT_BINARY",
            "binary operation Div has no scalar Region IR lowering",
        )),
        BinaryOp::Mod => unsupported_binary("Mod"),
        BinaryOp::Concat => unsupported_binary("Concat"),
        BinaryOp::Pow => unsupported_binary("Pow"),
        BinaryOp::BitAnd => unsupported_binary("BitAnd"),
        BinaryOp::BitOr => unsupported_binary("BitOr"),
        BinaryOp::BitXor => unsupported_binary("BitXor"),
        BinaryOp::ShiftLeft => unsupported_binary("ShiftLeft"),
        BinaryOp::ShiftRight => unsupported_binary("ShiftRight"),
    }
}

fn unsupported_binary(name: &'static str) -> Result<RegionBinaryOp, NativeCompileError> {
    Err(NativeCompileError::new(
        "JIT_REGION_REJECT_BINARY",
        format!("binary operation {name} has no scalar Region IR lowering"),
    ))
}

fn lower_compare(op: CompareOp) -> Result<RegionCompareOpCode, NativeCompileError> {
    match op {
        CompareOp::Equal | CompareOp::Identical => Ok(RegionCompareOpCode::Equal),
        CompareOp::NotEqual | CompareOp::NotIdentical => Ok(RegionCompareOpCode::NotEqual),
        CompareOp::Less => Ok(RegionCompareOpCode::Less),
        CompareOp::LessEqual => Ok(RegionCompareOpCode::LessEqual),
        CompareOp::Greater => Ok(RegionCompareOpCode::Greater),
        CompareOp::GreaterEqual => Ok(RegionCompareOpCode::GreaterEqual),
        CompareOp::Spaceship => Err(NativeCompileError::new(
            "JIT_REGION_REJECT_COMPARE",
            "comparison Spaceship has no scalar Region IR lowering",
        )),
    }
}

fn lower_operand(unit: &IrUnit, operand: Operand) -> Result<RegionOperand, NativeCompileError> {
    match operand {
        Operand::Register(register) => Ok(RegionOperand::Register(register)),
        Operand::Local(local) => Ok(RegionOperand::Local(local)),
        Operand::Constant(constant) => lower_constant(unit, constant),
    }
}

fn lower_call_operands(unit: &IrUnit, args: &[IrCallArg]) -> Vec<Option<RegionOperand>> {
    args.iter()
        .map(|argument| lower_operand(unit, argument.value).ok())
        .collect()
}

fn lower_constant(
    unit: &IrUnit,
    constant: php_ir::ConstId,
) -> Result<RegionOperand, NativeCompileError> {
    match unit.constants.get(constant.index()) {
        Some(IrConstant::Int(value)) => Ok(RegionOperand::I64(*value)),
        Some(other) => Err(NativeCompileError::new(
            "JIT_REGION_REJECT_CONSTANT",
            format!("constant {other:?} is outside the scalar Region IR"),
        )),
        None => Err(NativeCompileError::new(
            "JIT_REGION_REJECT_CONSTANT",
            format!("constant {} is missing", constant.raw()),
        )),
    }
}

fn lower_terminator(
    unit: &IrUnit,
    block_count: usize,
    block_index: usize,
    block: &php_ir::BasicBlock,
) -> Result<RegionTerminator, NativeCompileError> {
    let terminator = block.terminator.as_ref().ok_or_else(|| {
        NativeCompileError::new(
            "JIT_REGION_REJECT_TERMINATOR",
            format!("block {} has no terminator", block.id.raw()),
        )
    })?;
    let fallthrough = || {
        (block_index + 1 < block_count)
            .then(|| BlockId::new((block_index + 1) as u32))
            .ok_or_else(|| {
                NativeCompileError::new(
                    "JIT_REGION_REJECT_FALLTHROUGH",
                    format!("block {} has no fallthrough block", block.id.raw()),
                )
            })
    };
    match &terminator.kind {
        TerminatorKind::Jump { target } => Ok(RegionTerminator::Jump { target: *target }),
        TerminatorKind::JumpIfFalse { condition, target } => Ok(RegionTerminator::JumpIfFalse {
            condition: lower_operand(unit, *condition)?,
            target: *target,
            fallthrough: fallthrough()?,
        }),
        TerminatorKind::JumpIfTrue { condition, target } => Ok(RegionTerminator::JumpIfTrue {
            condition: lower_operand(unit, *condition)?,
            target: *target,
            fallthrough: fallthrough()?,
        }),
        TerminatorKind::JumpIf {
            condition,
            if_true,
            if_false,
        } => Ok(RegionTerminator::JumpIf {
            condition: lower_operand(unit, *condition)?,
            if_true: *if_true,
            if_false: *if_false,
        }),
        TerminatorKind::Return {
            value: Some(value),
            by_ref_local: None,
        } => Ok(RegionTerminator::Return {
            value: lower_operand(unit, *value)?,
            finally: None,
        }),
        TerminatorKind::Return { value: None, .. } => Err(NativeCompileError::new(
            "JIT_REGION_REJECT_TERMINATOR",
            "void return has no scalar Region IR lowering",
        )),
        TerminatorKind::Return {
            value: Some(_),
            by_ref_local: Some(local),
        } => Ok(RegionTerminator::ReturnReference {
            local: *local,
            finally: None,
        }),
        TerminatorKind::Exit { value } => Ok(RegionTerminator::Exit {
            value: value.map(|value| lower_operand(unit, value)).transpose()?,
            finally: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_ir::{
        ClassEntry, ClassFlags, ClassId, ClassMethodEntry, ClassMethodFlags, FunctionFlags,
        IrBuilder, IrCapture, IrParam, IrSpan, UnitId,
    };

    #[test]
    fn builds_verified_multiblock_region_from_php_ir() {
        let mut builder = IrBuilder::new(UnitId::new(91));
        let file = builder.add_file("region.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("region", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        builder.push_param(
            function,
            IrParam {
                name: "value".to_owned(),
                local,
                required: true,
                type_: Some(IrReturnType::Int),
                by_ref: false,
                variadic: false,
                default: None,
                attributes: Vec::new(),
            },
        );
        builder.set_return_type(function, Some(IrReturnType::Int));
        let entry = builder.append_block(function);
        let body = builder.append_block(function);
        builder.terminate_jump(function, entry, body, span);
        let loaded = builder.alloc_register(function);
        builder.emit(
            function,
            body,
            InstructionKind::LoadLocal { dst: loaded, local },
            span,
        );
        builder.terminate_return(function, body, Some(Operand::Register(loaded)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("region");
        assert_eq!(region.arity(), 1);
        assert_eq!(region.blocks.len(), 2);
        region.verify().expect("verified region");
    }

    #[test]
    fn preserves_method_declaration_and_strict_types_metadata() {
        let mut builder = IrBuilder::new(UnitId::new(92));
        let file = builder.add_file("method.php");
        builder.set_strict_types(true);
        builder.set_file_strict_types(file, true);
        let span = IrSpan::new(file, 4, 40);
        let function = builder.start_function(
            "Widget::value",
            FunctionFlags {
                is_method: true,
                ..FunctionFlags::default()
            },
            span,
        );
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        let constant = builder.intern_constant(IrConstant::Int(7));
        let value = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadConst {
                dst: value,
                constant,
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(value)), span);
        builder.push_class(ClassEntry {
            id: ClassId::new(0),
            name: "widget".to_owned(),
            display_name: "Widget".to_owned(),
            parent: None,
            parent_display_name: None,
            interfaces: Vec::new(),
            methods: vec![ClassMethodEntry {
                name: "value".to_owned(),
                origin_class: "widget".to_owned(),
                function,
                flags: ClassMethodFlags {
                    has_body: true,
                    ..ClassMethodFlags::default()
                },
                attributes: Vec::new(),
            }],
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor: None,
            flags: ClassFlags::default(),
            span,
        });
        let unit = builder.finish();
        let region = BaselineRegionBuilder::build(&unit, function, &CompileMetadata::default())
            .expect("method graph");

        assert!(region.flags.is_method);
        assert!(region.strict_types);
        let method = region.declarations.method.expect("method identity");
        assert_eq!(method.class_display_name, "Widget");
        assert_eq!(method.method.function, function);
    }

    #[test]
    fn every_ir_call_form_enters_the_unified_native_call_model() {
        let mut builder = IrBuilder::new(UnitId::new(95));
        let file = builder.add_file("calls.php");
        let span = IrSpan::new(file, 0, 20);
        let function = builder.start_function("calls", FunctionFlags::default(), span);
        let block = builder.append_block(function);
        let constant = builder.intern_constant(IrConstant::Int(1));
        let value = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadConst {
                dst: value,
                constant,
            },
            span,
        );
        let argument = IrCallArg {
            name: None,
            value: Operand::Register(value),
            unpack: false,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: None,
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        };
        let local = builder.intern_local(function, "reference");
        let calls = [
            InstructionKind::CallFunction {
                dst: builder.alloc_register(function),
                name: "f".to_owned(),
                args: vec![argument.clone()],
            },
            InstructionKind::CallMethod {
                dst: builder.alloc_register(function),
                object: Operand::Register(value),
                method: "m".to_owned(),
                args: vec![argument.clone()],
            },
            InstructionKind::CallStaticMethod {
                dst: builder.alloc_register(function),
                class_name: "c".to_owned(),
                method: "m".to_owned(),
                args: vec![argument.clone()],
            },
            InstructionKind::CallClosure {
                dst: builder.alloc_register(function),
                callee: Operand::Register(value),
                args: vec![argument.clone()],
            },
            InstructionKind::CallCallable {
                dst: builder.alloc_register(function),
                callee: Operand::Register(value),
                args: vec![argument.clone()],
            },
            InstructionKind::Pipe {
                dst: builder.alloc_register(function),
                input: Operand::Register(value),
                callable: Operand::Register(value),
            },
            InstructionKind::BindReferenceFromCall {
                target: local,
                name: "by_ref".to_owned(),
                args: vec![argument.clone()],
            },
            InstructionKind::BindReferenceFromMethodCall {
                target: local,
                object: Operand::Register(value),
                method: "byRef".to_owned(),
                args: vec![argument.clone()],
            },
            InstructionKind::NewObject {
                dst: builder.alloc_register(function),
                display_class_name: "C".to_owned(),
                class_name: "c".to_owned(),
                args: vec![argument.clone()],
            },
            InstructionKind::DynamicNewObject {
                dst: builder.alloc_register(function),
                class_name: Operand::Register(value),
                args: vec![argument],
            },
        ];
        for call in calls {
            builder.emit(function, block, call, span);
        }
        builder.terminate_return(function, block, Some(Operand::Register(value)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("call graph");
        let native_calls = region.blocks[0]
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.kind, RegionInstructionKind::NativeCall(_)))
            .collect::<Vec<_>>();
        assert_eq!(native_calls.len(), 10);
        assert!(native_calls.iter().all(|instruction| !matches!(
            instruction.kind,
            RegionInstructionKind::MissingLowering
        )));
    }

    #[test]
    fn exception_instructions_enter_the_native_control_model() {
        let mut builder = IrBuilder::new(UnitId::new(96));
        let file = builder.add_file("exceptions.php");
        let span = IrSpan::new(file, 0, 30);
        let function = builder.start_function("exceptions", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let entry = builder.append_block(function);
        let finally = builder.append_block(function);
        let after = builder.append_block(function);
        builder.emit(
            function,
            entry,
            InstructionKind::EnterTry {
                catch: None,
                catch_types: Vec::new(),
                finally: Some(finally),
                after,
                exception_local: None,
            },
            span,
        );
        let message = builder.intern_constant(IrConstant::Int(17));
        let exception = builder.alloc_register(function);
        builder.emit(
            function,
            entry,
            InstructionKind::MakeException {
                dst: exception,
                class_name: "runtimeexception".to_owned(),
                message: Operand::Constant(message),
            },
            span,
        );
        builder.emit(function, entry, InstructionKind::LeaveTry, span);
        builder.emit(
            function,
            entry,
            InstructionKind::Throw {
                value: Operand::Register(exception),
            },
            span,
        );
        builder.terminate_jump(function, entry, after, span);
        builder.emit(
            function,
            finally,
            InstructionKind::EndFinally { after },
            span,
        );
        builder.terminate_jump(function, finally, after, span);
        let zero = builder.intern_constant(IrConstant::Int(0));
        builder.terminate_return(function, after, Some(Operand::Constant(zero)), span);
        let unit = builder.finish();
        let region = build_baseline_region(&unit, function).expect("exception region");
        let controls = region
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(instruction.kind, RegionInstructionKind::NativeControl(_))
            })
            .count();
        assert_eq!(controls, 5);
        assert_eq!(region.exception_regions.len(), 1);
        assert!(
            !region
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|instruction| matches!(
                    instruction.kind,
                    RegionInstructionKind::MissingLowering
                ))
        );
    }

    #[test]
    fn closure_and_missing_lowering_remain_in_the_semantic_graph() {
        let mut builder = IrBuilder::new(UnitId::new(93));
        let file = builder.add_file("closure.php");
        let span = IrSpan::new(file, 10, 20);
        let function = builder.start_function(
            "{closure}",
            FunctionFlags {
                is_closure: true,
                ..FunctionFlags::default()
            },
            span,
        );
        let captured = builder.intern_local(function, "captured");
        builder.push_capture(
            function,
            IrCapture {
                name: "captured".to_owned(),
                local: captured,
                by_ref: true,
            },
        );
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        let dst = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::FetchConst {
                dst,
                name: "DYNAMIC".to_owned(),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(dst)), span);
        let unit = builder.finish();
        let region = BaselineRegionBuilder::build(&unit, function, &CompileMetadata::default())
            .expect("closure graph");

        assert!(region.flags.is_closure);
        assert_eq!(region.captures[0].name, "captured");
        let instruction = &region.blocks[0].instructions[0];
        assert!(matches!(
            instruction.kind,
            RegionInstructionKind::MissingLowering
        ));
        assert!(matches!(
            instruction.source_kind,
            InstructionKind::FetchConst { .. }
        ));
        assert_eq!(instruction.span, span);
    }
}
