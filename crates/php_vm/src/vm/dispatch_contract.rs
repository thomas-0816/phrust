use super::prelude::*;

pub(super) fn next_dense_block_index(
    function: &DenseFunction,
    current: u32,
) -> Result<u32, String> {
    let next = current + 1;
    if next as usize >= function.blocks.len() {
        return Err(format!(
            "fallthrough dense block after block:{current} is missing"
        ));
    }
    Ok(next)
}

pub(super) fn dense_opcode_family(opcode: DenseOpcode) -> &'static str {
    match opcode {
        DenseOpcode::LoadConst
        | DenseOpcode::LoadConstEcho
        | DenseOpcode::LoadConstLoadConst
        | DenseOpcode::FetchConst => "constants",
        DenseOpcode::Move
        | DenseOpcode::LoadLocal
        | DenseOpcode::LoadLocalEcho
        | DenseOpcode::LoadLocalQuiet
        | DenseOpcode::StoreLocal
        | DenseOpcode::StoreLocalDiscard
        | DenseOpcode::InitStaticLocal
        | DenseOpcode::UnsetLocal
        | DenseOpcode::IssetLocal
        | DenseOpcode::EmptyLocal
        | DenseOpcode::BindGlobal
        | DenseOpcode::LoadLocalLoadConst => "locals",
        DenseOpcode::BinaryAdd
        | DenseOpcode::BinarySub
        | DenseOpcode::BinaryMul
        | DenseOpcode::BinaryDiv
        | DenseOpcode::BinaryMod
        | DenseOpcode::BinaryConcat
        | DenseOpcode::BinaryConcatEcho
        | DenseOpcode::BinaryPow
        | DenseOpcode::BinaryBitAnd
        | DenseOpcode::BinaryBitOr
        | DenseOpcode::BinaryBitXor
        | DenseOpcode::BinaryShiftLeft
        | DenseOpcode::BinaryShiftRight
        | DenseOpcode::Cast => "scalar_ops",
        DenseOpcode::CompareEqual
        | DenseOpcode::CompareNotEqual
        | DenseOpcode::CompareIdentical
        | DenseOpcode::CompareNotIdentical
        | DenseOpcode::CompareLess
        | DenseOpcode::CompareLessEqual
        | DenseOpcode::CompareGreater
        | DenseOpcode::CompareGreaterEqual
        | DenseOpcode::CompareSpaceship => "comparisons",
        DenseOpcode::UnaryPlus
        | DenseOpcode::UnaryMinus
        | DenseOpcode::UnaryNot
        | DenseOpcode::UnaryBitNot => "unary_ops",
        DenseOpcode::NewObject | DenseOpcode::AcquireCallable | DenseOpcode::MakeClosure => {
            "objects"
        }
        DenseOpcode::CallFunction
        | DenseOpcode::CallFunctionDiscard
        | DenseOpcode::CallMethod
        | DenseOpcode::CallStaticMethod
        | DenseOpcode::CallCallable
        | DenseOpcode::ResolveCallable
        | DenseOpcode::Pipe => "function_calls",
        DenseOpcode::NewArray
        | DenseOpcode::ArrayInsert
        | DenseOpcode::LoadConstArrayInsert
        | DenseOpcode::FetchDim
        | DenseOpcode::LoadConstFetchDim
        | DenseOpcode::AssignDim
        | DenseOpcode::AppendDim
        | DenseOpcode::BindReferenceDim
        | DenseOpcode::IssetDim
        | DenseOpcode::EmptyDim
        | DenseOpcode::UnsetDim => "arrays",
        DenseOpcode::FetchProperty
        | DenseOpcode::AssignProperty
        | DenseOpcode::AssignPropertyDim
        | DenseOpcode::UnsetPropertyDim
        | DenseOpcode::AssignDynamicProperty
        | DenseOpcode::AssignStaticProperty
        | DenseOpcode::IssetStaticProperty
        | DenseOpcode::EmptyStaticProperty
        | DenseOpcode::UnsetProperty
        | DenseOpcode::IssetPropertyDim
        | DenseOpcode::EmptyPropertyDim
        | DenseOpcode::IssetProperty
        | DenseOpcode::EmptyProperty => "properties",
        DenseOpcode::InstanceOf
        | DenseOpcode::FetchClassConstant
        | DenseOpcode::FetchStaticProperty
        | DenseOpcode::CloneObject => "objects",
        DenseOpcode::ForeachInit | DenseOpcode::ForeachNext | DenseOpcode::ForeachCleanup => {
            "foreach"
        }
        DenseOpcode::Include => "includes",
        DenseOpcode::Echo => "output",
        DenseOpcode::Jump
        | DenseOpcode::JumpIfFalse
        | DenseOpcode::JumpIfTrue
        | DenseOpcode::JumpIf
        | DenseOpcode::Exit => "control_flow",
        DenseOpcode::Return => "returns",
        DenseOpcode::DeclareFunction | DenseOpcode::DeclareClass => "declarations",
        DenseOpcode::Discard | DenseOpcode::Nop => "bookkeeping",
    }
}

pub(super) fn dense_bytecode_unsupported_reason(message: &str) -> &'static str {
    if message.contains("NewObject") {
        "object_instantiation"
    } else if message.contains("FetchProperty") {
        "property_fetch"
    } else if message.contains("AssignProperty") {
        "property_assignment"
    } else if message.contains("CallStaticMethod") {
        "static_method_call"
    } else if message.contains("CallMethod") {
        "method_call"
    } else if message.contains("Include") {
        "include"
    } else if message.contains("unpacked arguments") {
        "unpacked_call"
    } else if message.contains("TRACE_UNSUPPORTED") {
        "trace"
    } else if message.contains("VERIFY") {
        "verifier"
    } else if message.contains("ENTRY") {
        "entry"
    } else if dense_bytecode_reference_aliasing_message(message) {
        "reference_aliasing"
    } else {
        "instruction_subset"
    }
}

fn dense_bytecode_reference_aliasing_message(message: &str) -> bool {
    message.contains("BindReference")
        || message.contains("BindGlobal")
        || message.contains("BindReferenceDim")
        || message.contains("BindReferenceFromDim")
        || message.contains("BindReferenceProperty")
        || message.contains("BindReferenceStaticProperty")
        || message.contains("BindReferenceFromCall")
        || message.contains("BindReferenceFromMethodCall")
        || message.contains("ForeachInitRef")
        || message.contains("ForeachNextRef")
        || message.contains("reference/COW")
        || message.contains("reference_cow")
        || message.contains("cow_or_reference")
        || message.contains("by-reference")
        || message.contains("by reference")
        || message.contains("COW")
}

pub(super) struct DenseExecutionRequest<'unit, 'call> {
    pub(super) compiled: &'unit CompiledUnit,
    pub(super) dense: &'unit DenseBytecodeUnit,
    pub(super) plan: Option<&'unit DenseExecutionPlan>,
    pub(super) dense_function: &'unit DenseFunction,
    pub(super) ir_function: &'unit IrFunction,
    pub(super) function_id: FunctionId,
    pub(super) call: FunctionCall<'call>,
}

pub(super) struct DenseBinaryRequest<'unit> {
    pub(super) compiled: &'unit CompiledUnit,
    pub(super) unit_id: UnitId,
    pub(super) function_id: FunctionId,
    pub(super) instruction_index: u32,
    pub(super) opcode: DenseOpcode,
    pub(super) dst: u32,
    pub(super) lhs: DenseOperand,
    pub(super) rhs: DenseOperand,
    pub(super) span: IrSpan,
}

pub(super) struct RichCompareRequest<'unit> {
    pub(super) unit: &'unit IrUnit,
    pub(super) frame_index: usize,
    pub(super) dst: RegId,
    pub(super) op: CompareOp,
    pub(super) lhs: Operand,
    pub(super) rhs: Operand,
}

pub(super) struct RichUnaryRequest<'unit> {
    pub(super) unit: &'unit IrUnit,
    pub(super) frame_index: usize,
    pub(super) dst: RegId,
    pub(super) op: UnaryOp,
    pub(super) src: Operand,
}

pub(super) struct RichBinaryRequest<'unit> {
    pub(super) compiled: &'unit CompiledUnit,
    pub(super) unit: &'unit IrUnit,
    pub(super) frame_index: usize,
    pub(super) function_id: FunctionId,
    pub(super) block_id: BlockId,
    pub(super) instruction_id: InstrId,
    pub(super) dst: RegId,
    pub(super) op: BinaryOp,
    pub(super) lhs: Operand,
    pub(super) rhs: Operand,
    pub(super) span: IrSpan,
}

pub(super) enum RichBinaryError {
    Direct(Box<VmResult>),
    Route(Box<VmResult>),
}

pub(super) enum RichDispatchOutcome {
    Continue,
    Jump(BlockId),
    Return(Box<VmResult>),
}
