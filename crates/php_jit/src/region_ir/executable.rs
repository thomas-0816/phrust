//! Structured executable Region IR lowered from `php_ir`.

use php_ir::instruction::{
    CallableKind, IncludeKind, IrCallArg, IrCallArgValueKind, TerminatorKind,
};
use php_ir::{
    AttributeEntry, BinaryOp, BlockId, ClassMethodEntry, CompareOp, FunctionEntry, FunctionFlags,
    FunctionId, InstrId, InstructionKind, IrCapture, IrConstant, IrParam, IrReturnType, IrSpan,
    IrUnit, LocalId, Operand, RegId,
};
use std::collections::{BTreeMap, BTreeSet};

use super::{RegionClassName, RegionPropertyName, RegionSemanticContext, RegionSemanticOp};

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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
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
    pub protected_blocks: Vec<BlockId>,
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
    Div,
    Mod,
    Concat,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

/// Typed unary operations executed through the native runtime ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionUnaryOp {
    Plus,
    Minus,
    Not,
    BitNot,
}

/// Scalar comparison operations currently executable without a runtime helper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionCompareOpCode {
    Equal,
    NotEqual,
    Identical,
    NotIdentical,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Spaceship,
}

/// Typed casts executed through the native runtime ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionCastOp {
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    Void,
}

/// Region operand detached from the source unit's constant pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionOperand {
    Register(RegId),
    Local(LocalId),
    I64(i64),
    /// Constant-pool value encoded as a stable native value handle.
    Constant(u32),
}

/// Destination written by one unified native call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionCallResult {
    Register(RegId),
    ReferenceLocal(LocalId),
    Discard,
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
    /// PHP runtime semantics identified by an append-only operation ID, not a
    /// synthetic function symbol.
    Semantic {
        operation: RegionSemanticOp,
    },
}

/// One call-site contract. Argument metadata remains typed and is materialized
/// directly into native slots during lowering, never into VM call objects.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionNativeCall {
    pub result: RegionCallResult,
    pub target: RegionCallTarget,
    pub args: Vec<IrCallArg>,
    /// Number of leading operands that belong to the call target (receiver,
    /// callable, or captures) rather than to PHP-visible arguments.
    pub argument_operand_offset: usize,
    /// Compile-time scalar operands for direct-slot materialization. `None`
    /// selects the native binder/trampoline for that argument.
    pub operands: Vec<Option<RegionOperand>>,
    pub direct_arity: Option<u32>,
    pub variadic: bool,
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

/// Suspension implemented by a generated native state-machine transition.
#[derive(Clone, Debug, PartialEq)]
pub enum RegionNativeSuspend {
    GeneratorYield {
        dst: RegId,
        key: Option<RegionOperand>,
        value: Option<RegionOperand>,
    },
    GeneratorDelegate {
        dst: RegId,
        source: RegionOperand,
    },
    FiberSuspend {
        dst: RegId,
        value: Option<RegionOperand>,
    },
}

/// Dynamic compilation/publication operation emitted into generated code.
#[derive(Clone, Debug, PartialEq)]
pub enum RegionNativeDynamicCode {
    Include {
        dst: RegId,
        kind: IncludeKind,
        path: RegionOperand,
    },
    Eval {
        dst: RegId,
        code: RegionOperand,
    },
    DeclareFunction {
        name: String,
        function: FunctionId,
    },
    DeclareClass {
        name: String,
    },
    RegisterConstant {
        name: String,
        value: RegionOperand,
    },
    EmitDiagnostic,
    MakeClosure {
        dst: RegId,
        function: FunctionId,
        captures: Vec<RegionNativeClosureCapture>,
        binds_this: bool,
    },
}

/// One closure capture whose source location and binding mode are immutable
/// in Region IR. Optimizing lowering consumes the local directly; the exact
/// allocator receives only the resulting authoritative native encodings.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionNativeClosureCapture {
    pub name: String,
    pub local: LocalId,
    pub by_ref: bool,
}

impl RegionNativeCall {
    pub(crate) fn declared_argument_reference_requirement(&self, index: usize) -> Option<bool> {
        let argument = self.args.get(index)?;
        let parameters = match &self.target {
            RegionCallTarget::Function {
                name,
                function: None,
            } => {
                let normalized = name.trim_start_matches('\\');
                php_std::arginfo::function_metadata_indexed(normalized)
                    .or_else(|| {
                        normalized
                            .rsplit('\\')
                            .next()
                            .and_then(php_std::arginfo::function_metadata_indexed)
                    })
                    .map(|function| function.params)
            }
            RegionCallTarget::StaticMethod { class_name, method } => {
                php_std::generated::arginfo::method_metadata(class_name, method)
                    .map(|method| method.params)
            }
            RegionCallTarget::Constructor { class_name, .. } => {
                php_std::generated::arginfo::method_metadata(class_name, "__construct")
                    .map(|method| method.params)
            }
            _ => None,
        };
        let parameters = parameters?;
        let parameter = argument.name.as_deref().map_or_else(
            || {
                parameters
                    .get(index)
                    .or_else(|| parameters.last().filter(|parameter| parameter.variadic))
            },
            |name| {
                parameters
                    .iter()
                    .find(|parameter| parameter.name == name)
                    .or_else(|| parameters.last().filter(|parameter| parameter.variadic))
            },
        );
        Some(parameter.is_some_and(|parameter| parameter.by_ref))
    }

    /// Returns whether a known builtin parameter requires a reference cell.
    /// IR lvalue metadata alone is insufficient: PHP also records lvalue
    /// origins for ordinary by-value parameters.
    #[must_use]
    pub fn builtin_argument_requires_reference(&self, index: usize) -> bool {
        self.declared_argument_reference_requirement(index)
            .unwrap_or(false)
    }

    /// Returns whether the native trampoline must preserve this argument's
    /// lvalue so the runtime binder can apply the resolved callee signature.
    #[must_use]
    pub fn argument_requires_reference_binding(&self, index: usize) -> bool {
        let Some(argument) = self.args.get(index) else {
            return false;
        };
        let has_location = argument.by_ref_local.is_some()
            || argument.by_ref_dim.is_some()
            || argument.by_ref_property.is_some()
            || argument.by_ref_property_dim.is_some();
        if let Some(required) = self.declared_argument_reference_requirement(index) {
            return has_location && required;
        }
        if matches!(self.target, RegionCallTarget::Function { .. }) {
            // An unresolved cross-unit function signature is finalized by the
            // runtime dispatcher. Only a plain local can be speculatively
            // wrapped and restored after that decision. Eagerly binding an
            // array dimension or property permanently turns the caller's
            // element into a reference even when the resolved parameter is
            // by-value, so defer those locations until signature-aware
            // writeback exists.
            return argument.by_ref_local.is_some();
        }
        // Unknown dynamic method/callable signatures may only speculate on a
        // plain local, whose reference flag the trampoline can restore after
        // resolution. Binding an array dimension or property permanently
        // turns that caller location into a reference; a by-value call would
        // then corrupt subsequent copy-on-write assignments.
        argument.by_ref_local.is_some()
    }

    /// Returns whether this call needs the native reference-binding helper.
    #[must_use]
    pub fn needs_local_reference_binding(&self) -> bool {
        self.args
            .iter()
            .enumerate()
            .any(|(index, _)| self.argument_requires_reference_binding(index))
    }

    /// Returns a statically bound userland callee whose arguments are fully
    /// materialized for the native callee ABI. Complex runtime binding remains
    /// on the typed native trampoline.
    #[must_use]
    pub fn direct_compiled_target(&self) -> Option<FunctionId> {
        let RegionCallTarget::Function {
            function: Some(function),
            ..
        } = self.target
        else {
            return None;
        };
        let arity_matches = if self.variadic {
            self.direct_arity.is_some_and(|arity| {
                arity != 0
                    && self.operands.len()
                        >= usize::try_from(arity.saturating_sub(1)).unwrap_or(usize::MAX)
            })
        } else {
            self.direct_arity == u32::try_from(self.operands.len()).ok()
        };
        (arity_matches
            && self.operands.iter().all(Option::is_some)
            && self.args.iter().all(|arg| {
                arg.name.is_none()
                    && !arg.unpack
                    && (arg.value_kind == IrCallArgValueKind::Direct
                        || (arg.value_kind == IrCallArgValueKind::ByRefLocationPlaceholder
                            && arg.by_ref_local.is_some()))
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
    /// This instruction owns a real optimizing-to-baseline continuation.
    /// Unsupported instructions are grouped into baseline islands, so only
    /// the island entry carries this flag rather than every instruction.
    pub optimizer_transition_entry: bool,
    /// Authoritative instruction, retained even when native lowering is missing.
    pub source_kind: InstructionKind,
    /// Exact global symbol selected by a constant `$GLOBALS["name"]`
    /// operation. This is publication metadata: generated code consumes the
    /// dense numeric reference plan for this continuation and never hashes or
    /// dispatches the name.
    pub native_global_name: Option<String>,
    pub kind: RegionInstructionKind,
}

impl RegionInstruction {
    /// Returns actual register reads after executable optimizer rewrites. The
    /// retained source instruction remains authoritative for every form the
    /// optimizer does not rewrite.
    #[must_use]
    pub fn register_uses(&self) -> Vec<RegId> {
        let mut uses = Vec::new();
        let mut push = |operand: RegionOperand| {
            if let RegionOperand::Register(register) = operand {
                uses.push(register);
            }
        };
        match &self.kind {
            RegionInstructionKind::Move { src, .. }
            | RegionInstructionKind::Unary { src, .. }
            | RegionInstructionKind::Cast { src, .. }
            | RegionInstructionKind::Discard { src }
            | RegionInstructionKind::Echo { src } => push(*src),
            RegionInstructionKind::Binary { lhs, rhs, .. }
            | RegionInstructionKind::Compare { lhs, rhs, .. } => {
                push(*lhs);
                push(*rhs);
            }
            RegionInstructionKind::NativeCall(call) => {
                for operand in call.operands.iter().flatten() {
                    push(*operand);
                }
                let mut push_ir = |operand: Operand| {
                    if let Operand::Register(register) = operand {
                        uses.push(register);
                    }
                };
                for argument in &call.args {
                    push_ir(argument.value);
                    if let Some(dimension) = &argument.by_ref_dim {
                        for dimension in &dimension.dims {
                            push_ir(*dimension);
                        }
                    }
                    if let Some(property) = &argument.by_ref_property {
                        push_ir(property.object);
                    }
                    if let Some(property) = &argument.by_ref_property_dim {
                        push_ir(property.object);
                        for dimension in &property.dims {
                            push_ir(*dimension);
                        }
                    }
                }
            }
            _ => php_ir::instruction_register_uses(&self.source_kind, &mut uses),
        }
        uses.sort_unstable();
        uses.dedup();
        uses
    }

    /// Returns registers materialized or updated by this instruction. This is
    /// the baseline planner's definition set; it deliberately follows the
    /// executable operation while retaining the authoritative source defs for
    /// forms that have not been rewritten.
    #[must_use]
    pub fn register_definitions(&self) -> Vec<RegId> {
        let mut definitions = Vec::new();
        php_ir::instruction_register_defs(&self.source_kind, &mut definitions);
        match &self.kind {
            RegionInstructionKind::ArrayInsert { array, .. }
            | RegionInstructionKind::ArraySpread { array, .. } => definitions.push(*array),
            RegionInstructionKind::ForeachNext { key, value, .. } => {
                definitions.extend(*key);
                definitions.push(*value);
            }
            RegionInstructionKind::ForeachNextRef { key, .. } => definitions.extend(*key),
            _ => {}
        }
        definitions.sort_unstable();
        definitions.dedup();
        definitions
    }
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
        quiet: bool,
    },
    StoreLocal {
        local: LocalId,
        src: RegionOperand,
    },
    AssignLocalResult {
        dst: RegId,
        local: LocalId,
        value: RegionOperand,
    },
    BindReference {
        target: LocalId,
        source: LocalId,
    },
    BindReferenceDim {
        target: LocalId,
        array: LocalId,
        keys: Vec<RegionOperand>,
    },
    BindReferenceIntoDim {
        array: LocalId,
        keys: Vec<RegionOperand>,
        append: bool,
        source: LocalId,
    },
    BindReferenceProperty {
        object: RegionOperand,
        source: LocalId,
        property: String,
        prepared_class: Option<u32>,
    },
    BindReferenceFromProperty {
        target: LocalId,
        object: RegionOperand,
        property: String,
        prepared_class: Option<u32>,
    },
    BindReferenceFromPropertyDim {
        target: LocalId,
        object: RegionOperand,
        keys: Vec<RegionOperand>,
        property: String,
        prepared_class: Option<u32>,
    },
    BindReferenceIntoPropertyDim {
        object: RegionOperand,
        keys: Vec<RegionOperand>,
        append: bool,
        source: LocalId,
        property: String,
        prepared_class: Option<u32>,
    },
    BindReferenceDimFromProperty {
        array: LocalId,
        keys: Vec<RegionOperand>,
        append: bool,
        object: RegionOperand,
        property: String,
        prepared_class: Option<u32>,
    },
    InitStaticLocal {
        local: LocalId,
        default: RegionOperand,
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
    Unary {
        dst: RegId,
        op: RegionUnaryOp,
        src: RegionOperand,
    },
    Compare {
        dst: RegId,
        op: RegionCompareOpCode,
        lhs: RegionOperand,
        rhs: RegionOperand,
    },
    Cast {
        dst: RegId,
        op: RegionCastOp,
        src: RegionOperand,
    },
    Echo {
        src: RegionOperand,
    },
    NewArray {
        dst: RegId,
    },
    NewObject {
        dst: RegId,
        class: u32,
        prepared: bool,
    },
    FetchProperty {
        dst: RegId,
        object: RegionOperand,
        property: String,
        prepared_class: Option<u32>,
    },
    FetchDynamicStaticProperty {
        dst: RegId,
        class_name: RegionOperand,
    },
    FetchObjectClassName {
        dst: RegId,
        object: RegionOperand,
        prepared_class: Option<u32>,
    },
    AssignProperty {
        dst: RegId,
        object: RegionOperand,
        value: RegionOperand,
        property: String,
        prepared_class: Option<u32>,
    },
    CloneObject {
        dst: RegId,
        object: RegionOperand,
        plain: bool,
    },
    CloneWith {
        dst: RegId,
        object: RegionOperand,
        replacements: RegionOperand,
    },
    ArrayInsert {
        array: RegId,
        key: Option<RegionOperand>,
        value: RegionOperand,
        by_ref_local: Option<LocalId>,
    },
    ArraySpread {
        array: RegId,
        source: RegionOperand,
    },
    FetchDim {
        dst: RegId,
        array: RegionOperand,
        key: RegionOperand,
        quiet: bool,
        mode: php_ir::instruction::DimFetchMode,
    },
    FetchConst {
        dst: RegId,
    },
    AssignDim {
        dst: RegId,
        local: LocalId,
        keys: Vec<RegionOperand>,
        value: RegionOperand,
    },
    AppendDim {
        dst: RegId,
        local: LocalId,
        keys: Vec<RegionOperand>,
        value: RegionOperand,
    },
    IssetDim {
        dst: RegId,
        local: LocalId,
        keys: Vec<RegionOperand>,
    },
    EmptyDim {
        dst: RegId,
        local: LocalId,
        keys: Vec<RegionOperand>,
    },
    UnsetDim {
        local: LocalId,
        keys: Vec<RegionOperand>,
    },
    IssetLocal {
        dst: RegId,
        local: LocalId,
    },
    EmptyLocal {
        dst: RegId,
        local: LocalId,
    },
    UnsetLocal {
        local: LocalId,
    },
    ForeachInit {
        iterator: RegId,
        source: RegionOperand,
    },
    ForeachInitRef {
        iterator: RegId,
        local: LocalId,
    },
    ForeachNext {
        has_value: RegId,
        iterator: RegId,
        key: Option<RegId>,
        value: RegId,
    },
    ForeachCleanup {
        iterator: RegId,
    },
    ForeachNextRef {
        has_value: RegId,
        iterator: RegId,
        key: Option<RegId>,
        value_local: LocalId,
    },
    NativeCall(RegionNativeCall),
    NativeControl(RegionNativeControl),
    NativeSuspend(RegionNativeSuspend),
    NativeDynamicCode(RegionNativeDynamicCode),
    /// Explicit fatal produced by IR lowering; native code returns fatal status.
    RuntimeFatal {
        /// Optional source result made unreachable by this fatal operation.
        dst: Option<RegId>,
        diagnostic_id: String,
        message: String,
    },
    /// Explicit unsupported-feature fatal emitted by the frontend.
    CompileTimeFatal {
        diagnostic_id: String,
    },
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
}

impl RegionTerminator {
    /// Returns actual register reads after branch folding.
    #[must_use]
    pub fn register_uses(&self) -> Vec<RegId> {
        let operand = match self {
            Self::Jump { .. } | Self::ReturnReference { .. } => None,
            Self::JumpIfFalse { condition, .. }
            | Self::JumpIfTrue { condition, .. }
            | Self::JumpIf { condition, .. }
            | Self::Return {
                value: condition, ..
            } => Some(*condition),
            Self::Exit { value, .. } => *value,
        };
        match operand {
            Some(RegionOperand::Register(register)) => vec![register],
            _ => Vec::new(),
        }
    }
}

/// One basic block in an executable region.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionBlock {
    pub id: BlockId,
    /// Original PHP IR block used by callsite and diagnostic metadata. Native
    /// fragmentation may assign a different internal CFG `id`.
    pub source_block: BlockId,
    /// Stable native continuation for entry into this executable block.
    /// Unlike the first remaining instruction, this survives optimization
    /// and identifies the same baseline/optimizing island boundary.
    pub entry_continuation_id: u32,
    pub entry_live_locals: Vec<LocalId>,
    /// Locals with a materialized value on at least one incoming path.
    /// Unlike safepoint liveness this includes path-dependent values and is
    /// used only by bounded native-fragment frame transitions.
    pub entry_state_locals: Vec<LocalId>,
    pub instructions: Vec<RegionInstruction>,
    pub terminator_span: IrSpan,
    pub terminator_continuation_id: u32,
    pub terminator_live_locals: Vec<LocalId>,
    pub terminator_state_locals: Vec<LocalId>,
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
                let continuation_id = region_block.entry_continuation_id;
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
                if let RegionInstructionKind::NativeDynamicCode(
                    RegionNativeDynamicCode::DeclareFunction { function, .. }
                    | RegionNativeDynamicCode::MakeClosure { function, .. },
                ) = &instruction.kind
                {
                    callees.insert(*function);
                }
            }
        }
        callees.into_iter().collect()
    }

    #[must_use]
    pub fn has_native_trampoline_calls(&self) -> bool {
        self.blocks.iter().any(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call)
                    if call.direct_compiled_target().is_none()
                        && !matches!(call.target, RegionCallTarget::Semantic { .. }))
            })
        })
    }

    #[must_use]
    pub fn has_native_suspensions(&self) -> bool {
        self.blocks.iter().any(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_))
            })
        })
    }

    #[must_use]
    pub fn has_native_dynamic_code(&self) -> bool {
        self.blocks.iter().any(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(
                    instruction.kind,
                    RegionInstructionKind::NativeDynamicCode(_)
                )
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
    pub(crate) fn targets(&self) -> Vec<BlockId> {
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
        }
    }
}

/// Builds exhaustive baseline Region IR from authoritative PHP IR.
pub struct BaselineRegionBuilder;

#[derive(Clone)]
struct KnownClosure {
    function: FunctionId,
    captures: Vec<RegionOperand>,
    bound_object: Option<RegionOperand>,
    requires_runtime_context: bool,
}

fn closure_requires_implicit_this(unit: &IrUnit, closure_function: FunctionId) -> bool {
    unit.functions
        .get(closure_function.index())
        .and_then(php_ir::IrFunction::implicit_closure_this_local)
        .is_some()
}

fn native_method_class(unit: &IrUnit, function: FunctionId) -> Option<(u32, bool)> {
    unit.classes.iter().enumerate().find_map(|(class, entry)| {
        let is_static = entry
            .methods
            .iter()
            .find(|method| method.function == function)
            .map(|method| method.flags.is_static)
            .or_else(|| {
                entry
                    .properties
                    .iter()
                    .any(|property| {
                        property.hooks.get == Some(function) || property.hooks.set == Some(function)
                    })
                    .then_some(false)
            })?;
        u32::try_from(class).ok().map(|class| (class, is_static))
    })
}

/// Returns the exact packed native-entry locals for a PHP function.
///
/// Declared PHP parameters are only one part of the native ABI: instance
/// methods and bound closures prepend an implicit `$this`, while closures
/// prepend their captured locals. Function-on-demand metadata must use this
/// same list before the callee RegionGraph exists, otherwise the caller and
/// the eventually compiled entry disagree about the packed frame shape.
pub(crate) fn native_function_parameter_locals(
    unit: &IrUnit,
    function: FunctionId,
) -> Option<Vec<LocalId>> {
    let ir_function = unit.functions.get(function.index())?;
    let method_class = native_method_class(unit, function);
    let implicit_receiver = if ir_function.flags.is_method {
        method_class
            .is_some_and(|(_, is_static)| !is_static)
            .then_some(LocalId::new(0))
    } else {
        ir_function.implicit_closure_this_local()
    };
    Some(
        implicit_receiver
            .into_iter()
            .chain(ir_function.captures.iter().map(|capture| capture.local))
            .chain(ir_function.params.iter().map(|parameter| parameter.local))
            .collect(),
    )
}

fn omitted_defaults_require_runtime_binding(
    target: &php_ir::IrFunction,
    supplied_arguments: usize,
) -> bool {
    target
        .params
        .iter()
        .skip(supplied_arguments)
        .filter_map(|parameter| parameter.default.as_ref())
        .any(|default| matches!(default, IrConstant::Array(_)))
}

fn implicit_closure_bound_object(
    unit: &IrUnit,
    caller: &php_ir::IrFunction,
    closure_function: FunctionId,
    caller_has_bound_this: bool,
) -> Option<RegionOperand> {
    if !caller_has_bound_this || !closure_requires_implicit_this(unit, closure_function) {
        return None;
    }
    let closure = unit.functions.get(closure_function.index())?;
    debug_assert!(closure.implicit_closure_this_local().is_some());
    caller
        .locals
        .iter()
        .position(|name| name == "this")
        .and_then(|index| u32::try_from(index).ok())
        .map(LocalId::new)
        .map(RegionOperand::Local)
}

impl BaselineRegionBuilder {
    pub fn build(
        unit: &IrUnit,
        function: FunctionId,
        runtime_metadata: &CompileMetadata,
    ) -> Result<RegionGraph, NativeCompileError> {
        php_ir::verify_function(unit, function).map_err(|errors| {
            let first = &errors[0];
            NativeCompileError::new(
                "JIT_REGION_REJECT_INVALID_IR",
                format!(
                    "function={} span={}:{}-{} verifier={}: {}",
                    function.raw(),
                    unit.functions
                        .get(function.index())
                        .map_or(u32::MAX, |function| function.span.file.raw()),
                    unit.functions
                        .get(function.index())
                        .map_or(0, |function| function.span.start),
                    unit.functions
                        .get(function.index())
                        .map_or(0, |function| function.span.end),
                    first.diagnostic_id(),
                    first.message
                ),
            )
        })?;
        let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
            NativeCompileError::new(
                "JIT_REGION_REJECT_MISSING_FUNCTION",
                format!("function id {} is not present", function.raw()),
            )
        })?;
        let mut fast_path_operations = 0_u64;
        let mut blocks = Vec::with_capacity(ir_function.blocks.len());
        let mut next_continuation = 0_u32;
        let mut region_local_count = ir_function.local_count;
        let mut region_locals = ir_function.locals.clone();
        let mut region_register_count = ir_function.register_count;
        let exception_regions = collect_exception_regions(ir_function);
        let method_class = native_method_class(unit, function);
        let stable_callable_entries = stable_callable_local_entries(unit, ir_function);
        for (block_index, block) in ir_function.blocks.iter().enumerate() {
            let entry_continuation_id = next_continuation;
            let mut instructions = Vec::with_capacity(block.instructions.len());
            let mut known_register_strings = BTreeMap::<RegId, String>::new();
            let mut known_local_strings = stable_callable_entries
                .get(block.id.index())
                .cloned()
                .unwrap_or_default();
            let mut known_callables = BTreeMap::<RegId, String>::new();
            let mut known_callable_locals = known_local_strings.clone();
            let mut known_null_registers = BTreeSet::<RegId>::new();
            let mut known_closure_registers = BTreeMap::<RegId, KnownClosure>::new();
            let mut known_closure_locals = BTreeMap::<LocalId, KnownClosure>::new();
            let mut known_object_registers = BTreeMap::<RegId, u32>::new();
            let mut known_object_locals = BTreeMap::<LocalId, u32>::new();
            let mut exact_object_registers = BTreeSet::<RegId>::new();
            let mut exact_object_locals = BTreeSet::<LocalId>::new();
            let mut native_globals_registers = BTreeSet::<RegId>::new();
            if let Some((class, false)) = method_class
                && unit
                    .classes
                    .get(class as usize)
                    .is_some_and(|class| class.flags.is_final)
            {
                // `$this` may be an instance of a subclass. Treat its class as
                // exact only when the declaring class cannot be extended;
                // otherwise a direct call would bypass virtual overrides.
                known_object_locals.insert(LocalId::new(0), class);
                exact_object_locals.insert(LocalId::new(0));
            }
            for instruction in &block.instructions {
                let mut prepared_call_args = None::<Vec<IrCallArg>>;
                match &instruction.kind {
                    InstructionKind::LoadConst { dst, constant } => {
                        match unit.constants.get(constant.index()) {
                            Some(IrConstant::String(value)) => {
                                known_register_strings.insert(*dst, value.clone());
                            }
                            Some(IrConstant::Null) => {
                                known_null_registers.insert(*dst);
                            }
                            _ => {}
                        }
                    }
                    InstructionKind::Move { dst, src } => {
                        if let Some(value) =
                            known_string_operand(unit, *src, &known_register_strings)
                        {
                            known_register_strings.insert(*dst, value);
                        }
                        if let Operand::Register(register) = src
                            && let Some(closure) = known_closure_registers.get(register)
                        {
                            known_closure_registers.insert(*dst, closure.clone());
                        }
                        if let Operand::Register(register) = src
                            && let Some(name) = known_callables.get(register)
                        {
                            known_callables.insert(*dst, name.clone());
                        }
                        if let Operand::Register(register) = src
                            && known_null_registers.contains(register)
                        {
                            known_null_registers.insert(*dst);
                        }
                        if let Operand::Register(register) = src
                            && let Some(class) = known_object_registers.get(register)
                        {
                            known_object_registers.insert(*dst, *class);
                        }
                        if let Operand::Register(register) = src
                            && exact_object_registers.contains(register)
                        {
                            exact_object_registers.insert(*dst);
                        }
                        if let Operand::Register(register) = src
                            && native_globals_registers.contains(register)
                        {
                            native_globals_registers.insert(*dst);
                        }
                    }
                    InstructionKind::LoadLocal { dst, local }
                    | InstructionKind::LoadLocalQuiet { dst, local } => {
                        if let Some(value) = known_local_strings.get(local) {
                            known_register_strings.insert(*dst, value.clone());
                        }
                        if let Some(closure) = known_closure_locals.get(local) {
                            known_closure_registers.insert(*dst, closure.clone());
                        }
                        if let Some(name) = known_callable_locals.get(local) {
                            known_callables.insert(*dst, name.clone());
                        }
                        if let Some(class) = known_object_locals.get(local) {
                            known_object_registers.insert(*dst, *class);
                        }
                        if exact_object_locals.contains(local) {
                            exact_object_registers.insert(*dst);
                        }
                        if ir_function
                            .locals
                            .get(local.index())
                            .is_some_and(|name| name == "GLOBALS")
                        {
                            native_globals_registers.insert(*dst);
                        }
                    }
                    InstructionKind::StoreLocal { local, src } => {
                        if let Some(value) =
                            known_string_operand(unit, *src, &known_register_strings)
                        {
                            known_local_strings.insert(*local, value);
                        } else {
                            known_local_strings.remove(local);
                        }
                        if let Operand::Register(register) = src
                            && let Some(closure) = known_closure_registers.get(register)
                        {
                            known_closure_locals.insert(*local, closure.clone());
                        } else {
                            known_closure_locals.remove(local);
                        }
                        if let Operand::Register(register) = src
                            && let Some(name) = known_callables.get(register)
                        {
                            known_callable_locals.insert(*local, name.clone());
                        } else {
                            known_callable_locals.remove(local);
                        }
                        if let Operand::Register(register) = src
                            && let Some(class) = known_object_registers.get(register)
                        {
                            known_object_locals.insert(*local, *class);
                        } else {
                            known_object_locals.remove(local);
                        }
                        if let Operand::Register(register) = src
                            && exact_object_registers.contains(register)
                        {
                            exact_object_locals.insert(*local);
                        } else {
                            exact_object_locals.remove(local);
                        }
                    }
                    InstructionKind::ResolveCallable {
                        dst,
                        callable: CallableKind::FunctionName { name },
                    } => {
                        known_callables.insert(*dst, name.clone());
                    }
                    InstructionKind::CallFunction { name, args, .. } => {
                        let target = find_function(unit, name)
                            .or_else(|| {
                                unit.functions
                                    .iter()
                                    .position(|function| function.name.eq_ignore_ascii_case(name))
                                    .and_then(|index| u32::try_from(index).ok())
                                    .map(FunctionId::new)
                            })
                            .and_then(|function| unit.functions.get(function.index()));
                        let mut prepared = args.clone();
                        for (index, argument) in prepared.iter_mut().enumerate() {
                            if !target
                                .and_then(|target| {
                                    argument
                                        .name
                                        .as_deref()
                                        .and_then(|name| {
                                            target.params.iter().find(|parameter| {
                                                parameter.name.eq_ignore_ascii_case(name)
                                            })
                                        })
                                        .or_else(|| target.params.get(index))
                                })
                                .is_some_and(|parameter| parameter.by_ref)
                            {
                                continue;
                            }
                            let binding = if let Some(local) = argument.by_ref_local {
                                Some(RegionInstructionKind::BindReference {
                                    target: local,
                                    source: local,
                                })
                            } else if let Some(dimension) = &argument.by_ref_dim {
                                let temporary = LocalId::new(region_local_count);
                                region_local_count = region_local_count.saturating_add(1);
                                region_locals.push(format!(
                                    "by_ref_call_{}_{}",
                                    instruction.id.raw(),
                                    index
                                ));
                                argument.by_ref_local = Some(temporary);
                                Some(RegionInstructionKind::BindReferenceDim {
                                    target: temporary,
                                    array: dimension.local,
                                    keys: dimension
                                        .dims
                                        .iter()
                                        .map(|operand| lower_operand(unit, *operand))
                                        .collect(),
                                })
                            } else if let Some(property) = &argument.by_ref_property {
                                let temporary = LocalId::new(region_local_count);
                                region_local_count = region_local_count.saturating_add(1);
                                region_locals.push(format!(
                                    "by_ref_call_{}_{}",
                                    instruction.id.raw(),
                                    index
                                ));
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    optimizer_transition_entry: false,
                                    source_kind: instruction.kind.clone(),
                                    native_global_name: None,
                                    kind: RegionInstructionKind::StoreLocal {
                                        local: temporary,
                                        src: lower_operand(unit, argument.value),
                                    },
                                });
                                next_continuation = next_continuation.saturating_add(1);
                                argument.by_ref_local = Some(temporary);
                                Some(RegionInstructionKind::BindReferenceProperty {
                                    object: lower_operand(unit, property.object),
                                    source: temporary,
                                    property: property.property.clone(),
                                    prepared_class: prepared_exact_object_class(
                                        unit,
                                        property.object,
                                        &known_object_registers,
                                        &exact_object_registers,
                                    ),
                                })
                            } else {
                                None
                            };
                            if let Some(kind) = binding {
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    optimizer_transition_entry: false,
                                    source_kind: instruction.kind.clone(),
                                    native_global_name: None,
                                    kind,
                                });
                                next_continuation = next_continuation.saturating_add(1);
                            }
                        }
                        prepared_call_args = Some(prepared);
                        if let InstructionKind::CallFunction {
                            dst, name, args, ..
                        } = &instruction.kind
                            && let Some(closure) = returned_closure(unit, name, args)
                        {
                            let mut snapshots = Vec::with_capacity(closure.captures.len());
                            for (index, src) in closure.captures.into_iter().enumerate() {
                                let snapshot = LocalId::new(region_local_count);
                                region_local_count = region_local_count.saturating_add(1);
                                region_locals.push(format!(
                                    "returned_closure_capture_{}_{}",
                                    instruction.id.raw(),
                                    index
                                ));
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    optimizer_transition_entry: false,
                                    source_kind: instruction.kind.clone(),
                                    native_global_name: None,
                                    kind: RegionInstructionKind::StoreLocal {
                                        local: snapshot,
                                        src,
                                    },
                                });
                                next_continuation = next_continuation.saturating_add(1);
                                snapshots.push(RegionOperand::Local(snapshot));
                            }
                            known_closure_registers.insert(
                                *dst,
                                KnownClosure {
                                    function: closure.function,
                                    captures: snapshots,
                                    bound_object: None,
                                    requires_runtime_context: true,
                                },
                            );
                        }
                    }
                    InstructionKind::CallCallable { callee, args, .. } => {
                        let target = known_string_operand(unit, *callee, &known_register_strings)
                            .and_then(|name| find_function(unit, &name))
                            .and_then(|function| unit.functions.get(function.index()));
                        let mut prepared = args.clone();
                        for (index, argument) in prepared.iter_mut().enumerate() {
                            if !target
                                .and_then(|target| target.params.get(index))
                                .is_some_and(|parameter| parameter.by_ref)
                            {
                                continue;
                            }
                            if let Some(local) = argument.by_ref_local {
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    optimizer_transition_entry: false,
                                    source_kind: instruction.kind.clone(),
                                    native_global_name: None,
                                    kind: RegionInstructionKind::BindReference {
                                        target: local,
                                        source: local,
                                    },
                                });
                                next_continuation = next_continuation.saturating_add(1);
                            }
                        }
                        prepared_call_args = Some(prepared);
                    }
                    InstructionKind::MakeClosure {
                        dst,
                        function,
                        captures,
                    } => {
                        let captures = captures.iter().try_fold(
                            Vec::with_capacity(captures.len()),
                            |mut lowered, capture| {
                                let src = lower_operand(unit, capture.src);
                                let snapshot = LocalId::new(region_local_count);
                                region_local_count = region_local_count.saturating_add(1);
                                region_locals.push(format!(
                                    "closure_capture_{}_{}",
                                    instruction.id.raw(),
                                    lowered.len()
                                ));
                                let kind = if capture.by_ref {
                                    let Operand::Local(source) = capture.src else {
                                        return Err(NativeCompileError::new(
                                            "JIT_REGION_REJECT_REFERENCE_CAPTURE",
                                            "by-reference closure capture is not a local",
                                        ));
                                    };
                                    RegionInstructionKind::BindReference {
                                        target: snapshot,
                                        source,
                                    }
                                } else {
                                    RegionInstructionKind::StoreLocal {
                                        local: snapshot,
                                        src,
                                    }
                                };
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    optimizer_transition_entry: false,
                                    source_kind: instruction.kind.clone(),
                                    native_global_name: None,
                                    kind,
                                });
                                next_continuation = next_continuation.saturating_add(1);
                                lowered.push(RegionOperand::Local(snapshot));
                                Ok::<_, NativeCompileError>(lowered)
                            },
                        );
                        if let Ok(captures) = captures {
                            let caller_has_bound_this = method_class
                                .is_some_and(|(_, is_static)| !is_static)
                                || (ir_function.flags.is_closure
                                    && closure_requires_implicit_this(unit, *function));
                            let bound_object = implicit_closure_bound_object(
                                unit,
                                ir_function,
                                *function,
                                caller_has_bound_this,
                            );
                            if !closure_requires_implicit_this(unit, *function)
                                || bound_object.is_some()
                            {
                                known_closure_registers.insert(
                                    *dst,
                                    KnownClosure {
                                        function: *function,
                                        captures,
                                        bound_object,
                                        requires_runtime_context: method_class.is_some()
                                            || ir_function.flags.is_closure,
                                    },
                                );
                            }
                        }
                    }
                    InstructionKind::CallStaticMethod {
                        dst,
                        class_name,
                        method,
                        args,
                    } if class_name.eq_ignore_ascii_case("Closure")
                        && method.eq_ignore_ascii_case("bind")
                        && args.len() >= 2 =>
                    {
                        let closure = match args[0].value {
                            Operand::Register(register) => {
                                known_closure_registers.get(&register).cloned()
                            }
                            Operand::Local(local) => known_closure_locals.get(&local).cloned(),
                            _ => None,
                        };
                        let bound_object = match args[1].value {
                            Operand::Constant(constant)
                                if matches!(
                                    unit.constants.get(constant.index()),
                                    Some(IrConstant::Null)
                                ) =>
                            {
                                Some(None)
                            }
                            Operand::Register(register)
                                if known_null_registers.contains(&register) =>
                            {
                                Some(None)
                            }
                            operand => Some(Some(lower_operand(unit, operand))),
                        };
                        if let (Some(mut closure), Some(bound_object)) = (closure, bound_object) {
                            closure.bound_object = bound_object;
                            known_closure_registers.insert(*dst, closure);
                        }
                    }
                    InstructionKind::NewObject {
                        dst, class_name, ..
                    } => {
                        if let Some((class_index, class)) = find_class(unit, class_name)
                            && class.constructor.is_some()
                        {
                            instructions.push(RegionInstruction {
                                id: instruction.id,
                                span: instruction.span,
                                continuation_id: next_continuation,
                                live_locals: Vec::new(),
                                optimizer_transition_entry: false,
                                source_kind: instruction.kind.clone(),
                                native_global_name: None,
                                kind: RegionInstructionKind::NewObject {
                                    dst: *dst,
                                    class: class_index,
                                    prepared: class_has_publication_stable_layout(
                                        unit,
                                        class_index,
                                    ),
                                },
                            });
                            next_continuation = next_continuation.saturating_add(1);
                        }
                        if let Some((class_index, _)) = find_class(unit, class_name) {
                            known_object_registers.insert(*dst, class_index);
                            exact_object_registers.insert(*dst);
                        }
                    }
                    InstructionKind::CloneObject { dst, object } => {
                        if let Operand::Register(register) = object
                            && exact_object_registers.contains(register)
                            && let Some(class) = known_object_registers.get(register).copied()
                        {
                            known_object_registers.insert(*dst, class);
                            exact_object_registers.insert(*dst);
                        }
                    }
                    _ => {}
                }
                let semantic_context = RegionSemanticContext {
                    span: instruction.span,
                    continuation_id: next_continuation,
                };
                let kind = match &instruction.kind {
                    InstructionKind::Nop => RegionInstructionKind::Nop,
                    InstructionKind::LoadConst { dst, constant } => RegionInstructionKind::Move {
                        dst: *dst,
                        src: lower_constant(unit, *constant),
                    },
                    InstructionKind::Move { dst, src } => RegionInstructionKind::Move {
                        dst: *dst,
                        src: lower_operand(unit, *src),
                    },
                    InstructionKind::LoadLocal { dst, local } => RegionInstructionKind::LoadLocal {
                        dst: *dst,
                        local: *local,
                        quiet: false,
                    },
                    InstructionKind::LoadLocalQuiet { dst, local } => {
                        RegionInstructionKind::LoadLocal {
                            dst: *dst,
                            local: *local,
                            quiet: true,
                        }
                    }
                    InstructionKind::StoreLocal { local, src } => {
                        RegionInstructionKind::StoreLocal {
                            local: *local,
                            src: lower_operand(unit, *src),
                        }
                    }
                    InstructionKind::Discard { src } => RegionInstructionKind::Discard {
                        src: lower_operand(unit, *src),
                    },
                    InstructionKind::Binary { dst, op, lhs, rhs } => {
                        fast_path_operations = fast_path_operations.saturating_add(1);
                        RegionInstructionKind::Binary {
                            dst: *dst,
                            op: lower_binary(*op),
                            lhs: lower_operand(unit, *lhs),
                            rhs: lower_operand(unit, *rhs),
                        }
                    }
                    InstructionKind::Unary { dst, op, src } => RegionInstructionKind::Unary {
                        dst: *dst,
                        op: lower_unary(*op),
                        src: lower_operand(unit, *src),
                    },
                    InstructionKind::Compare { dst, op, lhs, rhs } => {
                        fast_path_operations = fast_path_operations.saturating_add(1);
                        RegionInstructionKind::Compare {
                            dst: *dst,
                            op: lower_compare(*op),
                            lhs: lower_operand(unit, *lhs),
                            rhs: lower_operand(unit, *rhs),
                        }
                    }
                    InstructionKind::Cast { dst, kind, src } => RegionInstructionKind::Cast {
                        dst: *dst,
                        op: lower_cast(*kind),
                        src: lower_operand(unit, *src),
                    },
                    InstructionKind::Echo { src } => RegionInstructionKind::Echo {
                        src: lower_operand(unit, *src),
                    },
                    InstructionKind::NewArray { dst } => {
                        RegionInstructionKind::NewArray { dst: *dst }
                    }
                    InstructionKind::ArrayInsert {
                        array,
                        key,
                        value,
                        by_ref_local,
                    } => RegionInstructionKind::ArrayInsert {
                        array: *array,
                        key: key.map(|key| lower_operand(unit, key)),
                        value: by_ref_local
                            .map(RegionOperand::Local)
                            .unwrap_or_else(|| lower_operand(unit, *value)),
                        by_ref_local: *by_ref_local,
                    },
                    InstructionKind::ArraySpread { array, source } => {
                        RegionInstructionKind::ArraySpread {
                            array: *array,
                            source: lower_operand(unit, *source),
                        }
                    }
                    InstructionKind::FetchDim {
                        dst,
                        array,
                        key,
                        quiet,
                        mode,
                    } => RegionInstructionKind::FetchDim {
                        dst: *dst,
                        array: lower_operand(unit, *array),
                        key: lower_operand(unit, *key),
                        quiet: *quiet,
                        mode: *mode,
                    },
                    InstructionKind::ArrayGet { dst, array, index } => {
                        RegionInstructionKind::FetchDim {
                            dst: *dst,
                            array: lower_operand(unit, *array),
                            key: lower_operand(unit, *index),
                            quiet: false,
                            mode: php_ir::instruction::DimFetchMode::Read,
                        }
                    }
                    InstructionKind::FetchConst { dst, .. } => {
                        RegionInstructionKind::FetchConst { dst: *dst }
                    }
                    InstructionKind::AssignDim {
                        dst,
                        local,
                        dims,
                        value,
                    } => {
                        let keys = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let value = lower_operand(unit, *value);
                        if keys.is_empty() {
                            RegionInstructionKind::AssignLocalResult {
                                dst: *dst,
                                local: *local,
                                value,
                            }
                        } else {
                            RegionInstructionKind::AssignDim {
                                dst: *dst,
                                local: *local,
                                keys,
                                value,
                            }
                        }
                    }
                    InstructionKind::AppendDim {
                        dst,
                        local,
                        dims,
                        value,
                    } => RegionInstructionKind::AppendDim {
                        dst: *dst,
                        local: *local,
                        keys: dims.iter().map(|dim| lower_operand(unit, *dim)).collect(),
                        value: lower_operand(unit, *value),
                    },
                    InstructionKind::IssetDim { dst, local, dims } => {
                        let keys = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        if keys.is_empty() {
                            RegionInstructionKind::IssetLocal {
                                dst: *dst,
                                local: *local,
                            }
                        } else {
                            RegionInstructionKind::IssetDim {
                                dst: *dst,
                                local: *local,
                                keys,
                            }
                        }
                    }
                    InstructionKind::EmptyDim { dst, local, dims } => {
                        let keys = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        if keys.is_empty() {
                            RegionInstructionKind::EmptyLocal {
                                dst: *dst,
                                local: *local,
                            }
                        } else {
                            RegionInstructionKind::EmptyDim {
                                dst: *dst,
                                local: *local,
                                keys,
                            }
                        }
                    }
                    InstructionKind::UnsetDim { local, dims } => {
                        let keys = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        if keys.is_empty() {
                            RegionInstructionKind::UnsetLocal { local: *local }
                        } else {
                            RegionInstructionKind::UnsetDim {
                                local: *local,
                                keys,
                            }
                        }
                    }
                    InstructionKind::CallFunction { dst, name, args }
                        if name
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case("call_user_func")
                            && !args.is_empty()
                            && args
                                .iter()
                                .all(|argument| argument.name.is_none() && !argument.unpack) =>
                    {
                        let callee = args[0].value;
                        let callback_args = args[1..].to_vec();
                        let closure = match callee {
                            Operand::Register(register) => {
                                known_closure_registers.get(&register).cloned()
                            }
                            _ => None,
                        }
                        .filter(|closure| {
                            !closure.requires_runtime_context
                                && unit.functions.get(closure.function.index()).is_some_and(
                                    |function| {
                                        function.params.iter().all(|parameter| !parameter.by_ref)
                                    },
                                )
                        });
                        if let Some(closure) = closure {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_closure_call(
                                unit,
                                *dst,
                                closure,
                                &callback_args,
                                semantic_context,
                            )
                        } else if let Some(name) = known_callable_operand_name(
                            unit,
                            callee,
                            &known_register_strings,
                            &known_local_strings,
                            &known_callables,
                            &known_callable_locals,
                        )
                        .filter(|name| stable_named_callable_is_by_value_only(unit, name))
                        {
                            let (call, direct) =
                                lower_stable_named_callable(unit, *dst, name, &callback_args);
                            fast_path_operations =
                                fast_path_operations.saturating_add(u64::from(direct));
                            call
                        } else {
                            RegionInstructionKind::NativeCall(RegionNativeCall {
                                result: RegionCallResult::Register(*dst),
                                target: RegionCallTarget::Function {
                                    name: name.clone(),
                                    function: None,
                                },
                                args: args.to_vec(),
                                argument_operand_offset: 0,
                                operands: lower_call_operands(unit, args),
                                direct_arity: None,
                                variadic: false,
                                returns_by_reference: false,
                                caller_strict_types: unit.strict_types,
                            })
                        }
                    }
                    InstructionKind::CallFunction { dst, name, args }
                        if name
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case("call_user_func_array")
                            && args.len() == 2
                            && args
                                .iter()
                                .all(|argument| argument.name.is_none() && !argument.unpack) =>
                    {
                        let callee = args[0].value;
                        let mut unpacked = args[1].clone();
                        unpacked.unpack = true;
                        let callback_args = vec![unpacked];
                        if let Some(name) = known_callable_operand_name(
                            unit,
                            callee,
                            &known_register_strings,
                            &known_local_strings,
                            &known_callables,
                            &known_callable_locals,
                        )
                        .filter(|name| stable_named_callable_is_by_value_only(unit, name))
                        {
                            lower_stable_named_callable(unit, *dst, name, &callback_args).0
                        } else {
                            RegionInstructionKind::NativeCall(RegionNativeCall {
                                result: RegionCallResult::Register(*dst),
                                target: RegionCallTarget::Function {
                                    name: name.clone(),
                                    function: None,
                                },
                                args: args.to_vec(),
                                argument_operand_offset: 0,
                                operands: lower_call_operands(unit, args),
                                direct_arity: None,
                                variadic: false,
                                returns_by_reference: false,
                                caller_strict_types: unit.strict_types,
                            })
                        }
                    }
                    InstructionKind::CallFunction { dst, name, args } => {
                        let args = prepared_call_args.as_deref().unwrap_or(args);
                        let function = unit
                            .function_table
                            .iter()
                            .find(|entry| entry.name == *name)
                            .map(|entry| entry.function);
                        if function.is_some() {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                        }
                        let variadic = function.is_some_and(|function| {
                            unit.functions
                                .get(function.index())
                                .and_then(|target| target.params.last())
                                .is_some_and(|parameter| parameter.variadic)
                        });
                        let mut operands = lower_call_operands(unit, args);
                        if let Some(function) = function
                            && let Some(target) = unit.functions.get(function.index())
                        {
                            for parameter in target
                                .params
                                .iter()
                                .skip(args.len())
                                .filter(|parameter| !parameter.variadic)
                            {
                                let operand = parameter.default.as_ref().and_then(|default| {
                                    unit.constants
                                        .iter()
                                        .position(|constant| constant == default)
                                        .and_then(|index| u32::try_from(index).ok())
                                        .map(RegionOperand::Constant)
                                });
                                operands.push(operand);
                            }
                        }
                        let direct_function = function.filter(|function| {
                                unit.functions.get(function.index()).is_some_and(|target| {
                                    !target.flags.is_generator
                                        && !omitted_defaults_require_runtime_binding(
                                            target,
                                            args.len(),
                                        )
                                        && !target.blocks.iter().flat_map(|block| &block.instructions).any(
                                            |instruction| matches!(
                                                &instruction.kind,
                                                InstructionKind::CallFunction { name, .. }
                                                    if matches!(
                                                        name.to_ascii_lowercase().as_str(),
                                                        "func_num_args" | "func_get_arg" | "func_get_args"
                                                    )
                                            ),
                                        )
                                        && !target
                                            .blocks
                                            .iter()
                                            .flat_map(|block| &block.instructions)
                                            .any(|instruction| {
                                                matches!(
                                                    instruction.kind,
                                                    InstructionKind::Yield { .. }
                                                        | InstructionKind::YieldFrom { .. }
                                                )
                                            })
                                        && !target.blocks.iter().flat_map(|block| &block.instructions).any(
                                            |instruction| matches!(
                                                &instruction.kind,
                                                InstructionKind::CallStaticMethod {
                                                    class_name,
                                                    method,
                                                    ..
                                                } if class_name.eq_ignore_ascii_case("Fiber")
                                                    && method.eq_ignore_ascii_case("suspend")
                                            ),
                                        )
                                })
                            });
                        let direct_arity = direct_function.and_then(|function| {
                            unit.functions
                                .get(function.index())
                                .and_then(|target| u32::try_from(target.params.len()).ok())
                        });
                        let mut native_args = args.to_vec();
                        if let Some(target) =
                            function.and_then(|function| unit.functions.get(function.index()))
                        {
                            for (argument, parameter) in native_args.iter_mut().zip(&target.params)
                            {
                                if parameter.by_ref {
                                    argument.value_kind =
                                        IrCallArgValueKind::ByRefLocationPlaceholder;
                                }
                            }
                        }
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Function {
                                name: name.clone(),
                                // Retain same-unit signature identity even
                                // when this call must use the trampoline. The
                                // direct-call eligibility remains encoded by
                                // `direct_arity`; dropping the function id
                                // here made ordinary by-value lvalue arguments
                                // look like unresolved by-reference sends.
                                function,
                            },
                            args: native_args,
                            argument_operand_offset: 0,
                            operands,
                            direct_arity,
                            variadic,
                            returns_by_reference: function.is_some_and(|function| {
                                unit.functions
                                    .get(function.index())
                                    .is_some_and(|target| target.returns_by_ref)
                            }),
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::CallMethod {
                        dst,
                        object,
                        method,
                        args,
                    } => known_object_class(*object, &known_object_registers)
                        .and_then(|class| {
                            let class = unit.classes.get(class as usize)?;
                            let class_is_final = class.flags.is_final;
                            let receiver_is_exact = matches!(
                                object,
                                Operand::Register(register)
                                    if exact_object_registers.contains(register)
                            );
                            class
                                .methods
                                .iter()
                                .find(|entry| entry.name.eq_ignore_ascii_case(method))
                                .filter(|entry| {
                                    !entry.flags.is_private
                                        && !entry.flags.is_protected
                                        && (receiver_is_exact
                                            || class_is_final
                                            || entry.flags.is_final)
                                })
                                .map(|entry| entry.function)
                        })
                        .filter(|function| {
                            unit.functions
                                .get(function.index())
                                .is_some_and(|function| {
                                    !function.flags.is_generator
                                        && !function
                                            .blocks
                                            .iter()
                                            .flat_map(|block| &block.instructions)
                                            .any(|instruction| {
                                                matches!(
                                                    instruction.kind,
                                                    InstructionKind::Yield { .. }
                                                        | InstructionKind::YieldFrom { .. }
                                                )
                                            })
                                        && !function
                                            .blocks
                                            .iter()
                                            .flat_map(|block| &block.instructions)
                                            .any(|instruction| {
                                                matches!(
                                                    &instruction.kind,
                                                    InstructionKind::FetchClassConstant {
                                                        class_name,
                                                        ..
                                                    } if class_name.eq_ignore_ascii_case("static")
                                                )
                                            })
                                })
                        })
                        .map_or_else(
                            || {
                                let mut operands = vec![Some(lower_operand(unit, *object))];
                                operands.extend(lower_call_operands(unit, args));
                                RegionInstructionKind::NativeCall(RegionNativeCall {
                                    result: RegionCallResult::Register(*dst),
                                    target: RegionCallTarget::Method {
                                        receiver: *object,
                                        method: method.clone(),
                                    },
                                    args: args.to_vec(),
                                    argument_operand_offset: 1,
                                    operands,
                                    direct_arity: None,
                                    variadic: false,
                                    returns_by_reference: false,
                                    caller_strict_types: unit.strict_types,
                                })
                            },
                            |function| {
                                fast_path_operations = fast_path_operations.saturating_add(1);
                                lower_direct_method_call(unit, *dst, function, *object, args)
                            },
                        ),
                    InstructionKind::CallStaticMethod {
                        dst,
                        class_name,
                        method,
                        args,
                    } if class_name.eq_ignore_ascii_case("fiber")
                        && method.eq_ignore_ascii_case("suspend")
                        && args.len() <= 1
                        && ir_function.flags.is_top_level =>
                    {
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::StaticMethod {
                                class_name: class_name.clone(),
                                method: method.clone(),
                            },
                            args: args.clone(),
                            argument_operand_offset: 0,
                            operands: lower_call_operands(unit, args),
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::CallStaticMethod {
                        dst,
                        class_name,
                        method,
                        args,
                    } if class_name.eq_ignore_ascii_case("fiber")
                        && method.eq_ignore_ascii_case("suspend")
                        && args.len() <= 1 =>
                    {
                        RegionInstructionKind::NativeSuspend(RegionNativeSuspend::FiberSuspend {
                            dst: *dst,
                            value: args
                                .first()
                                .map(|argument| lower_operand(unit, argument.value)),
                        })
                    }
                    InstructionKind::CallStaticMethod {
                        dst,
                        class_name,
                        method,
                        args,
                    } => find_direct_static_method(unit, class_name, method).map_or_else(
                        || {
                            RegionInstructionKind::NativeCall(RegionNativeCall {
                                result: RegionCallResult::Register(*dst),
                                target: RegionCallTarget::StaticMethod {
                                    class_name: class_name.clone(),
                                    method: method.clone(),
                                },
                                args: args.to_vec(),
                                argument_operand_offset: 0,
                                operands: lower_call_operands(unit, args),
                                direct_arity: None,
                                variadic: false,
                                returns_by_reference: false,
                                caller_strict_types: unit.strict_types,
                            })
                        },
                        |function| {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_function_call(
                                unit,
                                *dst,
                                unit.functions[function.index()].name.clone(),
                                function,
                                args,
                            )
                        },
                    ),
                    InstructionKind::CallClosure { dst, callee, args } => {
                        let closure = match callee {
                            Operand::Register(register) => {
                                known_closure_registers.get(register).cloned()
                            }
                            _ => None,
                        };
                        if let Some(closure) = closure.filter(|closure| {
                            !closure.requires_runtime_context
                                && unit.functions.get(closure.function.index()).is_some_and(
                                    |function| {
                                        !function.flags.is_generator
                                            && !function
                                                .blocks
                                                .iter()
                                                .flat_map(|block| &block.instructions)
                                                .any(|instruction| {
                                                    matches!(
                                                        instruction.kind,
                                                        InstructionKind::Yield { .. }
                                                            | InstructionKind::YieldFrom { .. }
                                                    )
                                                })
                                    },
                                )
                        }) {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_closure_call(unit, *dst, closure, args, semantic_context)
                        } else {
                            let mut operands = vec![Some(lower_operand(unit, *callee))];
                            operands.extend(lower_call_operands(unit, args));
                            RegionInstructionKind::NativeCall(RegionNativeCall {
                                result: RegionCallResult::Register(*dst),
                                target: RegionCallTarget::Closure { callee: *callee },
                                args: args.clone(),
                                argument_operand_offset: 1,
                                operands,
                                direct_arity: None,
                                variadic: false,
                                returns_by_reference: false,
                                caller_strict_types: unit.strict_types,
                            })
                        }
                    }
                    InstructionKind::CallCallable { dst, callee, args } => {
                        let args = prepared_call_args.as_deref().unwrap_or(args);
                        let closure = match callee {
                            Operand::Register(register) => {
                                known_closure_registers.get(register).cloned()
                            }
                            _ => None,
                        };
                        if let Some(closure) = closure.filter(|closure| {
                            !closure.requires_runtime_context
                                && unit.functions.get(closure.function.index()).is_some_and(
                                    |function| {
                                        !function.flags.is_generator
                                            && !function
                                                .blocks
                                                .iter()
                                                .flat_map(|block| &block.instructions)
                                                .any(|instruction| {
                                                    matches!(
                                                        instruction.kind,
                                                        InstructionKind::Yield { .. }
                                                            | InstructionKind::YieldFrom { .. }
                                                    )
                                                })
                                    },
                                )
                        }) {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_closure_call(unit, *dst, closure, args, semantic_context)
                        } else {
                            let known_name = known_callable_operand_name(
                                unit,
                                *callee,
                                &known_register_strings,
                                &known_local_strings,
                                &known_callables,
                                &known_callable_locals,
                            );
                            if let Some(name) = known_name {
                                let (call, direct) =
                                    lower_stable_named_callable(unit, *dst, name, args);
                                fast_path_operations =
                                    fast_path_operations.saturating_add(u64::from(direct));
                                call
                            } else {
                                let mut operands = vec![Some(lower_operand(unit, *callee))];
                                operands.extend(lower_call_operands(unit, args));
                                RegionInstructionKind::NativeCall(RegionNativeCall {
                                    result: RegionCallResult::Register(*dst),
                                    target: RegionCallTarget::Callable { callee: *callee },
                                    args: args.to_vec(),
                                    argument_operand_offset: 1,
                                    operands,
                                    direct_arity: None,
                                    variadic: false,
                                    returns_by_reference: false,
                                    caller_strict_types: unit.strict_types,
                                })
                            }
                        }
                    }
                    InstructionKind::Pipe {
                        dst,
                        input,
                        callable,
                    } => {
                        let argument = IrCallArg {
                            name: None,
                            value: *input,
                            unpack: false,
                            value_kind: IrCallArgValueKind::Direct,
                            by_ref_local: None,
                            by_ref_dim: None,
                            by_ref_property: None,
                            by_ref_property_dim: None,
                        };
                        let known_closure = match callable {
                            Operand::Register(register) => {
                                known_closure_registers.get(register).cloned()
                            }
                            _ => None,
                        };
                        let known_name = known_callable_operand_name(
                            unit,
                            *callable,
                            &known_register_strings,
                            &known_local_strings,
                            &known_callables,
                            &known_callable_locals,
                        );
                        if let Some(closure) =
                            known_closure.filter(|closure| !closure.requires_runtime_context)
                        {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_closure_call(
                                unit,
                                *dst,
                                closure,
                                &[argument],
                                semantic_context,
                            )
                        } else if let Some(name) = known_name {
                            let (call, direct) =
                                lower_stable_named_callable(unit, *dst, name, &[argument]);
                            fast_path_operations =
                                fast_path_operations.saturating_add(u64::from(direct));
                            call
                        } else {
                            let mut operands = vec![Some(lower_operand(unit, *callable))];
                            operands.push(Some(lower_operand(unit, *input)));
                            RegionInstructionKind::NativeCall(RegionNativeCall {
                                result: RegionCallResult::Register(*dst),
                                target: RegionCallTarget::Pipe {
                                    callable: *callable,
                                },
                                args: vec![argument],
                                argument_operand_offset: 1,
                                operands,
                                direct_arity: None,
                                variadic: false,
                                returns_by_reference: false,
                                caller_strict_types: unit.strict_types,
                            })
                        }
                    }
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
                        let mut native_args = args.clone();
                        if let Some(target_function) =
                            function.and_then(|function| unit.functions.get(function.index()))
                        {
                            for (argument, parameter) in
                                native_args.iter_mut().zip(&target_function.params)
                            {
                                if parameter.by_ref {
                                    argument.value_kind =
                                        IrCallArgValueKind::ByRefLocationPlaceholder;
                                }
                            }
                        }
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::ReferenceLocal(*target),
                            target: RegionCallTarget::Function {
                                name: name.clone(),
                                function,
                            },
                            args: native_args,
                            argument_operand_offset: 0,
                            operands: lower_call_operands(unit, args),
                            direct_arity,
                            variadic: false,
                            returns_by_reference: true,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::BindReferenceFromMethodCall {
                        target,
                        object,
                        method,
                        args,
                    } => {
                        let mut operands = vec![Some(lower_operand(unit, *object))];
                        operands.extend(lower_call_operands(unit, args));
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::ReferenceLocal(*target),
                            target: RegionCallTarget::Method {
                                receiver: *object,
                                method: method.clone(),
                            },
                            args: args.clone(),
                            argument_operand_offset: 1,
                            operands,
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: true,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::NewObject {
                        dst,
                        display_class_name,
                        class_name,
                        args,
                    } => match find_class(unit, class_name) {
                        Some((class_index, class)) => match class.constructor {
                            Some(constructor) => {
                                let ignored = RegId::new(region_register_count);
                                region_register_count = region_register_count.saturating_add(1);
                                lower_direct_method_call(
                                    unit,
                                    ignored,
                                    constructor,
                                    Operand::Register(*dst),
                                    args,
                                )
                            }
                            None if args.is_empty() => RegionInstructionKind::NewObject {
                                dst: *dst,
                                class: class_index,
                                prepared: class_has_publication_stable_layout(unit, class_index),
                            },
                            None => RegionInstructionKind::NativeCall(RegionNativeCall {
                                result: RegionCallResult::Register(*dst),
                                target: RegionCallTarget::Constructor {
                                    display_class_name: display_class_name.clone(),
                                    class_name: class_name.clone(),
                                },
                                args: args.clone(),
                                argument_operand_offset: 0,
                                operands: lower_call_operands(unit, args),
                                direct_arity: None,
                                variadic: false,
                                returns_by_reference: false,
                                caller_strict_types: unit.strict_types,
                            }),
                        },
                        None => RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Constructor {
                                display_class_name: display_class_name.clone(),
                                class_name: class_name.clone(),
                            },
                            args: args.clone(),
                            argument_operand_offset: 0,
                            operands: lower_call_operands(unit, args),
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        }),
                    },
                    InstructionKind::DynamicNewObject {
                        dst,
                        class_name,
                        args,
                    } => {
                        let mut operands = vec![Some(lower_operand(unit, *class_name))];
                        operands.extend(lower_call_operands(unit, args));
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::DynamicConstructor {
                                class_name: *class_name,
                            },
                            args: args.clone(),
                            argument_operand_offset: 1,
                            operands,
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
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
                    InstructionKind::Throw { value } => {
                        RegionInstructionKind::NativeControl(RegionNativeControl::Throw {
                            value: lower_operand(unit, *value),
                        })
                    }
                    InstructionKind::MakeException {
                        dst,
                        class_name,
                        message,
                    } => RegionInstructionKind::NativeControl(RegionNativeControl::MakeException {
                        dst: *dst,
                        class_name: class_name.clone(),
                        message: Some(lower_operand(unit, *message)),
                    }),
                    InstructionKind::Include { dst, kind, path } => {
                        RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::Include {
                            dst: *dst,
                            kind: *kind,
                            path: lower_operand(unit, *path),
                        })
                    }
                    InstructionKind::Eval { dst, code } => {
                        RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::Eval {
                            dst: *dst,
                            code: lower_operand(unit, *code),
                        })
                    }
                    InstructionKind::DeclareFunction { name, function } => {
                        RegionInstructionKind::NativeDynamicCode(
                            RegionNativeDynamicCode::DeclareFunction {
                                name: name.clone(),
                                function: *function,
                            },
                        )
                    }
                    InstructionKind::IssetLocal { dst, local } => {
                        RegionInstructionKind::IssetLocal {
                            dst: *dst,
                            local: *local,
                        }
                    }
                    InstructionKind::EmptyLocal { dst, local } => {
                        RegionInstructionKind::EmptyLocal {
                            dst: *dst,
                            local: *local,
                        }
                    }
                    InstructionKind::UnsetLocal { local } => {
                        RegionInstructionKind::UnsetLocal { local: *local }
                    }
                    InstructionKind::ForeachInit { iterator, source } => {
                        RegionInstructionKind::ForeachInit {
                            iterator: *iterator,
                            source: lower_operand(unit, *source),
                        }
                    }
                    InstructionKind::ForeachInitRef { iterator, local } => {
                        RegionInstructionKind::ForeachInitRef {
                            iterator: *iterator,
                            local: *local,
                        }
                    }
                    InstructionKind::ForeachNext {
                        has_value,
                        iterator,
                        key,
                        value,
                    } => RegionInstructionKind::ForeachNext {
                        has_value: *has_value,
                        iterator: *iterator,
                        key: *key,
                        value: *value,
                    },
                    InstructionKind::ForeachCleanup { iterator } => {
                        RegionInstructionKind::ForeachCleanup {
                            iterator: *iterator,
                        }
                    }
                    InstructionKind::ForeachNextRef {
                        has_value,
                        iterator,
                        key,
                        value_local,
                    } => RegionInstructionKind::ForeachNextRef {
                        has_value: *has_value,
                        iterator: *iterator,
                        key: *key,
                        value_local: *value_local,
                    },
                    InstructionKind::DeclareClass { name } => {
                        RegionInstructionKind::NativeDynamicCode(
                            RegionNativeDynamicCode::DeclareClass { name: name.clone() },
                        )
                    }
                    InstructionKind::MakeClosure { dst, .. }
                        if known_closure_registers.contains_key(dst) =>
                    {
                        let InstructionKind::MakeClosure {
                            function, captures, ..
                        } = &instruction.kind
                        else {
                            unreachable!()
                        };
                        RegionInstructionKind::NativeDynamicCode(
                            RegionNativeDynamicCode::MakeClosure {
                                dst: *dst,
                                function: *function,
                                captures: captures
                                    .iter()
                                    .map(|capture| {
                                        let Operand::Local(local) = capture.src else {
                                            unreachable!(
                                                "verified closure captures always name locals"
                                            );
                                        };
                                        RegionNativeClosureCapture {
                                            name: capture.name.clone(),
                                            local,
                                            by_ref: capture.by_ref,
                                        }
                                    })
                                    .collect(),
                                binds_this: unit
                                    .functions
                                    .get(function.index())
                                    .is_some_and(|function| !function.flags.is_static)
                                    && ir_function.flags.is_method,
                            },
                        )
                    }
                    InstructionKind::MakeClosure {
                        dst,
                        function,
                        captures,
                    } => RegionInstructionKind::NativeDynamicCode(
                        RegionNativeDynamicCode::MakeClosure {
                            dst: *dst,
                            function: *function,
                            captures: captures
                                .iter()
                                .map(|capture| {
                                    let Operand::Local(local) = capture.src else {
                                        unreachable!(
                                            "verified closure captures always name locals"
                                        );
                                    };
                                    RegionNativeClosureCapture {
                                        name: capture.name.clone(),
                                        local,
                                        by_ref: capture.by_ref,
                                    }
                                })
                                .collect(),
                            binds_this: unit
                                .functions
                                .get(function.index())
                                .is_some_and(|function| !function.flags.is_static)
                                && ir_function.flags.is_method,
                        },
                    ),
                    InstructionKind::Yield { dst, key, value } => {
                        RegionInstructionKind::NativeSuspend(RegionNativeSuspend::GeneratorYield {
                            dst: *dst,
                            key: key.map(|key| lower_operand(unit, key)),
                            value: value.map(|value| lower_operand(unit, value)),
                        })
                    }
                    InstructionKind::YieldFrom { dst, source } => {
                        RegionInstructionKind::NativeSuspend(
                            RegionNativeSuspend::GeneratorDelegate {
                                dst: *dst,
                                source: lower_operand(unit, *source),
                            },
                        )
                    }
                    InstructionKind::RuntimeError {
                        diagnostic_id,
                        message,
                    } => RegionInstructionKind::RuntimeFatal {
                        dst: None,
                        diagnostic_id: diagnostic_id.clone(),
                        message: message.clone(),
                    },
                    InstructionKind::FetchStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => RegionInstructionKind::NativeCall(RegionNativeCall {
                        result: RegionCallResult::Register(*dst),
                        target: RegionCallTarget::Semantic {
                            operation: RegionSemanticOp::StaticPropertyFetch {
                                context: RegionSemanticContext {
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                },
                                class_name: RegionClassName::Static(class_name.clone()),
                                property: property.clone(),
                            },
                        },
                        args: Vec::new(),
                        argument_operand_offset: 0,
                        operands: Vec::new(),
                        direct_arity: None,
                        variadic: false,
                        returns_by_reference: false,
                        caller_strict_types: unit.strict_types,
                    }),
                    InstructionKind::FetchClassConstant {
                        dst,
                        class_name,
                        constant,
                    } => RegionInstructionKind::NativeCall(RegionNativeCall {
                        result: RegionCallResult::Register(*dst),
                        target: RegionCallTarget::Semantic {
                            operation: RegionSemanticOp::ClassConstantFetch {
                                context: RegionSemanticContext {
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                },
                                class_name: class_name.clone(),
                                constant: constant.clone(),
                            },
                        },
                        args: Vec::new(),
                        argument_operand_offset: 0,
                        operands: Vec::new(),
                        direct_arity: None,
                        variadic: false,
                        returns_by_reference: false,
                        caller_strict_types: unit.strict_types,
                    }),
                    InstructionKind::ResolveCallable { dst, callable } => {
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::ResolveCallable {
                                    context: RegionSemanticContext {
                                        span: instruction.span,
                                        continuation_id: next_continuation,
                                    },
                                    callable: callable.clone(),
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: Vec::new(),
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::InstanceOf {
                        dst,
                        object,
                        class_name,
                    } => {
                        let object = lower_operand(unit, *object);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::InstanceOf {
                                    context: RegionSemanticContext {
                                        span: instruction.span,
                                        continuation_id: next_continuation,
                                    },
                                    object,
                                    class_name: class_name.clone(),
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(object)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::DynamicInstanceOf {
                        dst,
                        object,
                        target,
                    } => {
                        let object = lower_operand(unit, *object);
                        let target = lower_operand(unit, *target);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::DynamicInstanceOf {
                                    context: RegionSemanticContext {
                                        span: instruction.span,
                                        continuation_id: next_continuation,
                                    },
                                    object,
                                    target,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(object), Some(target)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::AcquireCallable { dst, value } => {
                        let value = lower_operand(unit, *value);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::AcquireCallable {
                                    context: RegionSemanticContext {
                                        span: instruction.span,
                                        continuation_id: next_continuation,
                                    },
                                    value,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(value)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::FetchProperty {
                        dst,
                        object,
                        property,
                    } => RegionInstructionKind::FetchProperty {
                        dst: *dst,
                        object: lower_operand(unit, *object),
                        property: property.clone(),
                        prepared_class: match object {
                            Operand::Register(register)
                                if exact_object_registers.contains(register) =>
                            {
                                known_object_registers
                                    .get(register)
                                    .copied()
                                    .filter(|class| {
                                        class_has_publication_stable_layout(unit, *class)
                                    })
                            }
                            Operand::Register(_) | Operand::Local(_) | Operand::Constant(_) => None,
                        },
                    },
                    InstructionKind::AssignProperty {
                        dst,
                        object,
                        value,
                        property,
                    } => RegionInstructionKind::AssignProperty {
                        dst: *dst,
                        object: lower_operand(unit, *object),
                        value: lower_operand(unit, *value),
                        property: property.clone(),
                        prepared_class: match object {
                            Operand::Register(register)
                                if exact_object_registers.contains(register) =>
                            {
                                known_object_registers
                                    .get(register)
                                    .copied()
                                    .filter(|class| {
                                        class_has_publication_stable_layout(unit, *class)
                                    })
                            }
                            Operand::Register(_) | Operand::Local(_) | Operand::Constant(_) => None,
                        },
                    },
                    InstructionKind::FetchDynamicProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = lower_operand(unit, *object);
                        let property = lower_operand(unit, *property);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::PropertyFetch {
                                    context: semantic_context,
                                    object,
                                    property: RegionPropertyName::Dynamic(property),
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(object), Some(property)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::IssetDynamicProperty {
                        dst,
                        object,
                        property,
                    }
                    | InstructionKind::EmptyDynamicProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = lower_operand(unit, *object);
                        let property_operand = lower_operand(unit, *property);
                        let property = RegionPropertyName::Dynamic(property_operand);
                        let operation = if matches!(
                            instruction.kind,
                            InstructionKind::IssetDynamicProperty { .. }
                        ) {
                            RegionSemanticOp::PropertyIsset {
                                context: semantic_context,
                                object,
                                property,
                            }
                        } else {
                            RegionSemanticOp::PropertyEmpty {
                                context: semantic_context,
                                object,
                                property,
                            }
                        };
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic { operation },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(object), Some(property_operand)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::IssetProperty {
                        dst,
                        object,
                        property,
                    }
                    | InstructionKind::EmptyProperty {
                        dst,
                        object,
                        property,
                    } => {
                        let object = lower_operand(unit, *object);
                        let property = RegionPropertyName::Static(property.clone());
                        let operation =
                            if matches!(instruction.kind, InstructionKind::IssetProperty { .. }) {
                                RegionSemanticOp::PropertyIsset {
                                    context: semantic_context,
                                    object,
                                    property,
                                }
                            } else {
                                RegionSemanticOp::PropertyEmpty {
                                    context: semantic_context,
                                    object,
                                    property,
                                }
                            };
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic { operation },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(object)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::IssetPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    }
                    | InstructionKind::EmptyPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = lower_operand(unit, *object);
                        let dimensions = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let semantic_property = RegionPropertyName::Static(property.clone());
                        let operation =
                            if matches!(instruction.kind, InstructionKind::IssetPropertyDim { .. })
                            {
                                RegionSemanticOp::PropertyDimIsset {
                                    context: semantic_context,
                                    object,
                                    property: semantic_property,
                                    dimensions: dimensions.clone(),
                                }
                            } else {
                                RegionSemanticOp::PropertyDimEmpty {
                                    context: semantic_context,
                                    object,
                                    property: semantic_property,
                                    dimensions: dimensions.clone(),
                                }
                            };
                        let mut operands = Vec::with_capacity(dimensions.len() + 1);
                        operands.push(Some(object));
                        operands.extend(dimensions.into_iter().map(Some));
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic { operation },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands,
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::IssetDynamicPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    }
                    | InstructionKind::EmptyDynamicPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                    } => {
                        let object = lower_operand(unit, *object);
                        let property_operand = lower_operand(unit, *property);
                        let dimensions = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let semantic_property = RegionPropertyName::Dynamic(property_operand);
                        let operation = if matches!(
                            instruction.kind,
                            InstructionKind::IssetDynamicPropertyDim { .. }
                        ) {
                            RegionSemanticOp::PropertyDimIsset {
                                context: semantic_context,
                                object,
                                property: semantic_property,
                                dimensions: dimensions.clone(),
                            }
                        } else {
                            RegionSemanticOp::PropertyDimEmpty {
                                context: semantic_context,
                                object,
                                property: semantic_property,
                                dimensions: dimensions.clone(),
                            }
                        };
                        let mut operands = Vec::with_capacity(dimensions.len() + 2);
                        operands.push(Some(object));
                        operands.push(Some(property_operand));
                        operands.extend(dimensions.into_iter().map(Some));
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic { operation },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands,
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::AssignDynamicProperty {
                        dst,
                        object,
                        property,
                        value,
                    } => {
                        let object = lower_operand(unit, *object);
                        let property = lower_operand(unit, *property);
                        let value = lower_operand(unit, *value);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::PropertyAssign {
                                    context: semantic_context,
                                    object,
                                    property: RegionPropertyName::Dynamic(property),
                                    value,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(object), Some(property), Some(value)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::UnsetProperty { object, property } => {
                        let dst = RegId::new(region_register_count);
                        region_register_count = region_register_count.saturating_add(1);
                        let object = lower_operand(unit, *object);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::PropertyUnset {
                                    context: semantic_context,
                                    object,
                                    property: RegionPropertyName::Static(property.clone()),
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(object)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::UnsetDynamicProperty { object, property } => {
                        let dst = RegId::new(region_register_count);
                        region_register_count = region_register_count.saturating_add(1);
                        let object = lower_operand(unit, *object);
                        let property = lower_operand(unit, *property);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::PropertyUnset {
                                    context: semantic_context,
                                    object,
                                    property: RegionPropertyName::Dynamic(property),
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(object), Some(property)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::UnsetPropertyDim {
                        object,
                        property,
                        dims,
                    } => {
                        let dst = RegId::new(region_register_count);
                        region_register_count = region_register_count.saturating_add(1);
                        let object = lower_operand(unit, *object);
                        let dimensions = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let mut operands = vec![Some(object)];
                        operands.extend(dimensions.iter().copied().map(Some));
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::PropertyDimUnset {
                                    context: semantic_context,
                                    object,
                                    property: RegionPropertyName::Static(property.clone()),
                                    dimensions,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands,
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::AssignPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                        value,
                        append,
                    } => {
                        let object = lower_operand(unit, *object);
                        let dimensions = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let value = lower_operand(unit, *value);
                        let mut operands = vec![Some(object)];
                        operands.extend(dimensions.iter().copied().map(Some));
                        operands.push(Some(value));
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::PropertyDimAssign {
                                    context: semantic_context,
                                    object,
                                    property: RegionPropertyName::Static(property.clone()),
                                    dimensions,
                                    value,
                                    append: *append,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands,
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::AssignStaticProperty {
                        dst,
                        class_name,
                        property,
                        value,
                    } => {
                        let value = lower_operand(unit, *value);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::StaticPropertyAssign {
                                    context: semantic_context,
                                    class_name: RegionClassName::Static(class_name.clone()),
                                    property: property.clone(),
                                    value,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(value)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::AssignDynamicStaticProperty {
                        dst,
                        class_name,
                        property,
                        value,
                    } => {
                        let class_name = lower_operand(unit, *class_name);
                        let value = lower_operand(unit, *value);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::StaticPropertyAssign {
                                    context: semantic_context,
                                    class_name: RegionClassName::Dynamic(class_name),
                                    property: property.clone(),
                                    value,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(class_name), Some(value)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::IssetStaticProperty {
                        dst,
                        class_name,
                        property,
                    }
                    | InstructionKind::EmptyStaticProperty {
                        dst,
                        class_name,
                        property,
                    } => {
                        let operation = if matches!(
                            instruction.kind,
                            InstructionKind::IssetStaticProperty { .. }
                        ) {
                            RegionSemanticOp::StaticPropertyIsset {
                                context: semantic_context,
                                class_name: RegionClassName::Static(class_name.clone()),
                                property: property.clone(),
                            }
                        } else {
                            RegionSemanticOp::StaticPropertyEmpty {
                                context: semantic_context,
                                class_name: RegionClassName::Static(class_name.clone()),
                                property: property.clone(),
                            }
                        };
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic { operation },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: Vec::new(),
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::IssetStaticPropertyDim {
                        dst,
                        class_name,
                        property,
                        dims,
                    }
                    | InstructionKind::EmptyStaticPropertyDim {
                        dst,
                        class_name,
                        property,
                        dims,
                    } => {
                        let dimensions = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let operation = if matches!(
                            instruction.kind,
                            InstructionKind::IssetStaticPropertyDim { .. }
                        ) {
                            RegionSemanticOp::StaticPropertyDimIsset {
                                context: semantic_context,
                                class_name: RegionClassName::Static(class_name.clone()),
                                property: property.clone(),
                                dimensions: dimensions.clone(),
                            }
                        } else {
                            RegionSemanticOp::StaticPropertyDimEmpty {
                                context: semantic_context,
                                class_name: RegionClassName::Static(class_name.clone()),
                                property: property.clone(),
                                dimensions: dimensions.clone(),
                            }
                        };
                        let operands = dimensions.into_iter().map(Some).collect();
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic { operation },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands,
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::UnsetStaticPropertyDim {
                        class_name,
                        property,
                        dims,
                    } => {
                        let dimensions = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let operands = dimensions.iter().copied().map(Some).collect();
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Discard,
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::StaticPropertyDimUnset {
                                    context: semantic_context,
                                    class_name: RegionClassName::Static(class_name.clone()),
                                    property: property.clone(),
                                    dimensions,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands,
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: false,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::CloneObject { dst, object } => {
                        let plain = match object {
                            Operand::Register(register)
                                if exact_object_registers.contains(register) =>
                            {
                                known_object_registers
                                    .get(register)
                                    .copied()
                                    .is_some_and(|class| !class_has_clone_method(unit, class))
                            }
                            Operand::Register(_) | Operand::Local(_) | Operand::Constant(_) => {
                                false
                            }
                        };
                        RegionInstructionKind::CloneObject {
                            dst: *dst,
                            object: lower_operand(unit, *object),
                            plain,
                        }
                    }
                    InstructionKind::CloneWith {
                        dst,
                        object,
                        replacements,
                    } => RegionInstructionKind::CloneWith {
                        dst: *dst,
                        object: lower_operand(unit, *object),
                        replacements: lower_operand(unit, *replacements),
                    },
                    InstructionKind::BindGlobal { local, name } => {
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::ReferenceLocal(*local),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::BindGlobal {
                                    context: semantic_context,
                                    local: *local,
                                    name: name.clone(),
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: Vec::new(),
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: true,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::BindReferenceDim {
                        local,
                        dims,
                        append,
                        source,
                    } => RegionInstructionKind::BindReferenceIntoDim {
                        array: *local,
                        keys: dims.iter().map(|dim| lower_operand(unit, *dim)).collect(),
                        append: *append,
                        source: *source,
                    },
                    InstructionKind::BindReferenceProperty {
                        object,
                        property,
                        source,
                    } => RegionInstructionKind::BindReferenceProperty {
                        object: lower_operand(unit, *object),
                        source: *source,
                        property: property.clone(),
                        prepared_class: prepared_exact_object_class(
                            unit,
                            *object,
                            &known_object_registers,
                            &exact_object_registers,
                        ),
                    },
                    InstructionKind::BindReferencePropertyDim {
                        object,
                        dims,
                        append,
                        source,
                        property,
                    } => RegionInstructionKind::BindReferenceIntoPropertyDim {
                        object: lower_operand(unit, *object),
                        keys: dims.iter().map(|dim| lower_operand(unit, *dim)).collect(),
                        append: *append,
                        source: *source,
                        property: property.clone(),
                        prepared_class: prepared_exact_object_class(
                            unit,
                            *object,
                            &known_object_registers,
                            &exact_object_registers,
                        ),
                    },
                    InstructionKind::BindReferenceDimFromProperty {
                        local,
                        dims,
                        append,
                        object,
                        property,
                    } => RegionInstructionKind::BindReferenceDimFromProperty {
                        array: *local,
                        keys: dims.iter().map(|dim| lower_operand(unit, *dim)).collect(),
                        append: *append,
                        object: lower_operand(unit, *object),
                        property: property.clone(),
                        prepared_class: prepared_exact_object_class(
                            unit,
                            *object,
                            &known_object_registers,
                            &exact_object_registers,
                        ),
                    },
                    InstructionKind::BindReferenceFromProperty {
                        target,
                        object,
                        property,
                    } => RegionInstructionKind::BindReferenceFromProperty {
                        target: *target,
                        object: lower_operand(unit, *object),
                        property: property.clone(),
                        prepared_class: prepared_exact_object_class(
                            unit,
                            *object,
                            &known_object_registers,
                            &exact_object_registers,
                        ),
                    },
                    InstructionKind::BindReferenceFromPropertyDim {
                        target,
                        object,
                        dims,
                        property,
                    } => RegionInstructionKind::BindReferenceFromPropertyDim {
                        target: *target,
                        object: lower_operand(unit, *object),
                        keys: dims.iter().map(|dim| lower_operand(unit, *dim)).collect(),
                        property: property.clone(),
                        prepared_class: prepared_exact_object_class(
                            unit,
                            *object,
                            &known_object_registers,
                            &exact_object_registers,
                        ),
                    },
                    InstructionKind::BindReferenceStaticProperty {
                        class_name,
                        property,
                        source,
                    } => {
                        let source_local = *source;
                        let source = RegionOperand::Local(source_local);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::ReferenceLocal(source_local),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::StaticPropertyReference {
                                    context: semantic_context,
                                    target: source_local,
                                    class_name: RegionClassName::Static(class_name.clone()),
                                    property: property.clone(),
                                    dimensions: vec![source],
                                    bind_source_into_property: true,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands: vec![Some(source)],
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: true,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::FetchDynamicStaticProperty {
                        dst, class_name, ..
                    } => RegionInstructionKind::FetchDynamicStaticProperty {
                        dst: *dst,
                        class_name: lower_operand(unit, *class_name),
                    },
                    InstructionKind::FetchObjectClassName { dst, object } => {
                        let prepared_class = match object {
                            Operand::Register(register)
                                if exact_object_registers.contains(register) =>
                            {
                                known_object_registers
                                    .get(register)
                                    .copied()
                                    .filter(|class| {
                                        class_has_publication_stable_layout(unit, *class)
                                    })
                            }
                            Operand::Register(_) | Operand::Local(_) | Operand::Constant(_) => None,
                        };
                        RegionInstructionKind::FetchObjectClassName {
                            dst: *dst,
                            object: lower_operand(unit, *object),
                            prepared_class,
                        }
                    }
                    InstructionKind::RegisterConstant { name, value } => {
                        RegionInstructionKind::NativeDynamicCode(
                            RegionNativeDynamicCode::RegisterConstant {
                                name: name.clone(),
                                value: lower_operand(unit, *value),
                            },
                        )
                    }
                    InstructionKind::EmitDiagnostic { .. } => {
                        RegionInstructionKind::NativeDynamicCode(
                            RegionNativeDynamicCode::EmitDiagnostic,
                        )
                    }
                    InstructionKind::BindReference { target, source } => {
                        RegionInstructionKind::BindReference {
                            target: *target,
                            source: *source,
                        }
                    }
                    InstructionKind::BindReferenceFromDim {
                        target,
                        local,
                        dims,
                    } => RegionInstructionKind::BindReferenceDim {
                        target: *target,
                        array: *local,
                        keys: dims.iter().map(|dim| lower_operand(unit, *dim)).collect(),
                    },
                    InstructionKind::BindReferenceFromStaticPropertyDim {
                        target,
                        class_name,
                        property,
                        dims,
                    } => {
                        let dimensions = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let operands = dimensions.iter().copied().map(Some).collect();
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::ReferenceLocal(*target),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::StaticPropertyReference {
                                    context: semantic_context,
                                    target: *target,
                                    class_name: RegionClassName::Static(class_name.clone()),
                                    property: property.clone(),
                                    dimensions,
                                    bind_source_into_property: false,
                                },
                            },
                            args: Vec::new(),
                            argument_operand_offset: 0,
                            operands,
                            direct_arity: None,
                            variadic: false,
                            returns_by_reference: true,
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::InitStaticLocal { local, default, .. } => {
                        RegionInstructionKind::InitStaticLocal {
                            local: *local,
                            default: lower_operand(unit, *default),
                        }
                    }
                };
                if let RegionInstructionKind::NativeCall(call) = &kind {
                    super::semantic_lowering::validate_semantic_call(call, semantic_context)?;
                }
                let native_global_name = native_global_site_name(
                    unit,
                    ir_function,
                    &instruction.kind,
                    &known_register_strings,
                    &native_globals_registers,
                );
                instructions.push(RegionInstruction {
                    id: instruction.id,
                    span: instruction.span,
                    continuation_id: next_continuation,
                    live_locals: Vec::new(),
                    optimizer_transition_entry: false,
                    source_kind: instruction.kind.clone(),
                    native_global_name,
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
                .map_err(|error| {
                NativeCompileError::new(
                    error.code,
                    format!(
                        "function={} block={} span={}:{}-{}: {}",
                        ir_function.name,
                        block.id.raw(),
                        source_terminator.span.file.raw(),
                        source_terminator.span.start,
                        source_terminator.span.end,
                        error.detail
                    ),
                )
            })?;
            let terminator_span = source_terminator.span;
            blocks.push(RegionBlock {
                id: block.id,
                source_block: block.id,
                entry_continuation_id,
                entry_live_locals: Vec::new(),
                entry_state_locals: Vec::new(),
                instructions,
                terminator_span,
                terminator_continuation_id: next_continuation,
                terminator_live_locals: Vec::new(),
                terminator_state_locals: Vec::new(),
                source_terminator: source_terminator.kind.clone(),
                terminator,
            });
            next_continuation = next_continuation.saturating_add(1);
        }
        let parameter_locals = native_function_parameter_locals(unit, function)
            .expect("RegionGraph function must belong to its source unit");
        // Native entry state includes more than declared PHP parameters:
        // instance methods prepend `$this`, and closures can prepend a bound
        // receiver and captures. These locals are initialized at entry and
        // must remain part of semantic state across safepoints and fragment
        // boundaries just like explicit parameters.
        populate_live_locals(&mut blocks, &parameter_locals);
        annotate_native_finally_control(&mut blocks, &exception_regions);
        quiet_known_reference_argument_loads(&mut blocks);
        let region = RegionGraph {
            function,
            function_name: ir_function.name.clone(),
            function_span: ir_function.span,
            flags: ir_function.flags,
            strict_types: unit.strict_types_for_function(function),
            params: ir_function.params.clone(),
            locals: region_locals,
            captures: ir_function.captures.clone(),
            return_type: ir_function.return_type.clone(),
            returns_by_ref: ir_function.returns_by_ref,
            attributes: ir_function.attributes.clone(),
            declarations: declaration_metadata(unit, function),
            exception_regions,
            compile_metadata: runtime_metadata.clone(),
            parameter_locals,
            local_count: region_local_count,
            register_count: region_register_count,
            blocks,
            fast_path_operations,
        };
        region.verify()?;
        Ok(region)
    }
}

fn quiet_known_reference_argument_loads(blocks: &mut [RegionBlock]) {
    let quiet_registers = blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
        .flat_map(|call| {
            call.args.iter().enumerate().filter_map(|(index, _)| {
                call.argument_requires_reference_binding(index)
                    .then(|| {
                        call.operands
                            .get(call.argument_operand_offset + index)
                            .copied()
                            .flatten()
                    })
                    .flatten()
                    .and_then(|operand| match operand {
                        RegionOperand::Register(register) => Some(register),
                        _ => None,
                    })
            })
        })
        .collect::<BTreeSet<_>>();
    if quiet_registers.is_empty() {
        return;
    }
    for instruction in blocks.iter_mut().flat_map(|block| &mut block.instructions) {
        if let RegionInstructionKind::LoadLocal { dst, quiet, .. } = &mut instruction.kind
            && quiet_registers.contains(dst)
        {
            *quiet = true;
        }
    }
}

fn collect_exception_regions(ir_function: &php_ir::IrFunction) -> Vec<RegionExceptionRegion> {
    let mut regions = ir_function
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
                    protected_blocks: Vec::new(),
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
        .collect::<Vec<_>>();
    for region in &mut regions {
        let descriptor = region.clone();
        region.protected_blocks = ir_function
            .blocks
            .iter()
            .filter(|block| block_in_exception_body(ir_function, &descriptor, block.id))
            .map(|block| block.id)
            .collect();
    }
    regions
}

fn block_in_exception_body(
    function: &php_ir::IrFunction,
    region: &RegionExceptionRegion,
    candidate: BlockId,
) -> bool {
    if candidate == region.block {
        return true;
    }
    let mut pending = ir_block_successors(function, region.block);
    let mut visited = BTreeSet::new();
    while let Some(block) = pending.pop() {
        if Some(block) == region.catch || Some(block) == region.finally || block == region.after {
            continue;
        }
        if block == candidate {
            return true;
        }
        if visited.insert(block) {
            pending.extend(ir_block_successors(function, block));
        }
    }
    false
}

fn ir_block_successors(function: &php_ir::IrFunction, block: BlockId) -> Vec<BlockId> {
    let Some((index, block)) = function
        .blocks
        .iter()
        .enumerate()
        .find(|(_, candidate)| candidate.id == block)
    else {
        return Vec::new();
    };
    let Some(terminator) = &block.terminator else {
        return Vec::new();
    };
    let fallthrough = || function.blocks.get(index + 1).map(|block| block.id);
    match terminator.kind {
        TerminatorKind::Jump { target } => vec![target],
        TerminatorKind::JumpIfFalse { target, .. } | TerminatorKind::JumpIfTrue { target, .. } => {
            [Some(target), fallthrough()]
                .into_iter()
                .flatten()
                .collect()
        }
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => vec![if_true, if_false],
        TerminatorKind::Return { .. } | TerminatorKind::Exit { .. } => Vec::new(),
    }
}

fn stable_callable_local_entries(
    unit: &IrUnit,
    function: &php_ir::IrFunction,
) -> Vec<BTreeMap<LocalId, String>> {
    let mut predecessors = vec![Vec::<usize>::new(); function.blocks.len()];
    for block in &function.blocks {
        for successor in ir_block_successors(function, block.id) {
            if let Some(incoming) = predecessors.get_mut(successor.index()) {
                incoming.push(block.id.index());
            }
        }
    }
    for incoming in &mut predecessors {
        incoming.sort_unstable();
        incoming.dedup();
    }

    let mut entries = vec![None::<BTreeMap<LocalId, String>>; function.blocks.len()];
    let mut exits = vec![None::<BTreeMap<LocalId, String>>; function.blocks.len()];
    if !entries.is_empty() {
        entries[0] = Some(BTreeMap::new());
    }
    loop {
        let mut changed = false;
        for (block_index, block) in function.blocks.iter().enumerate() {
            let incoming = if block_index == 0 {
                BTreeMap::new()
            } else {
                let mut reachable = predecessors[block_index]
                    .iter()
                    .filter_map(|predecessor| exits[*predecessor].as_ref());
                let Some(first) = reachable.next() else {
                    continue;
                };
                let mut incoming = first.clone();
                for predecessor in reachable {
                    incoming.retain(|local, name| predecessor.get(local) == Some(name));
                }
                incoming
            };
            if entries[block_index].as_ref() != Some(&incoming) {
                entries[block_index] = Some(incoming.clone());
                changed = true;
            }
            let outgoing = transfer_stable_callable_locals(unit, block, incoming);
            if exits[block_index].as_ref() != Some(&outgoing) {
                exits[block_index] = Some(outgoing);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    entries.into_iter().map(Option::unwrap_or_default).collect()
}

fn transfer_stable_callable_locals(
    unit: &IrUnit,
    block: &php_ir::BasicBlock,
    mut locals: BTreeMap<LocalId, String>,
) -> BTreeMap<LocalId, String> {
    let mut registers = BTreeMap::<RegId, String>::new();
    let operand_name = |operand: Operand,
                        registers: &BTreeMap<RegId, String>,
                        locals: &BTreeMap<LocalId, String>| {
        match operand {
            Operand::Register(register) => registers.get(&register).cloned(),
            Operand::Local(local) => locals.get(&local).cloned(),
            Operand::Constant(constant) => match unit.constants.get(constant.index()) {
                Some(IrConstant::String(value)) => Some(value.clone()),
                _ => None,
            },
        }
    };
    for instruction in &block.instructions {
        match &instruction.kind {
            InstructionKind::LoadConst { dst, constant } => {
                if let Some(IrConstant::String(value)) = unit.constants.get(constant.index()) {
                    registers.insert(*dst, value.clone());
                }
            }
            InstructionKind::ResolveCallable {
                dst,
                callable: CallableKind::FunctionName { name },
            } => {
                registers.insert(*dst, name.clone());
            }
            InstructionKind::Move { dst, src } => {
                if let Some(name) = operand_name(*src, &registers, &locals) {
                    registers.insert(*dst, name);
                }
            }
            InstructionKind::LoadLocal { dst, local }
            | InstructionKind::LoadLocalQuiet { dst, local } => {
                if let Some(name) = locals.get(local) {
                    registers.insert(*dst, name.clone());
                }
            }
            InstructionKind::StoreLocal { local, src } => {
                if let Some(name) = operand_name(*src, &registers, &locals) {
                    locals.insert(*local, name);
                } else {
                    locals.remove(local);
                }
            }
            InstructionKind::BindReference { target, source } => {
                locals.remove(target);
                locals.remove(source);
            }
            InstructionKind::BindGlobal { local, .. }
            | InstructionKind::InitStaticLocal { local, .. }
            | InstructionKind::AssignDim { local, .. }
            | InstructionKind::AppendDim { local, .. }
            | InstructionKind::UnsetLocal { local }
            | InstructionKind::UnsetDim { local, .. }
            | InstructionKind::BindReferenceDim { local, .. }
            | InstructionKind::BindReferenceDimFromProperty { local, .. }
            | InstructionKind::ForeachInitRef { local, .. } => {
                locals.remove(local);
            }
            InstructionKind::BindReferenceFromProperty { target, .. }
            | InstructionKind::BindReferenceFromPropertyDim { target, .. }
            | InstructionKind::BindReferenceFromDim { target, .. }
            | InstructionKind::BindReferenceFromStaticPropertyDim { target, .. }
            | InstructionKind::BindReferenceFromCall { target, .. }
            | InstructionKind::BindReferenceFromMethodCall { target, .. } => {
                locals.remove(target);
            }
            InstructionKind::BindReferenceProperty { source, .. }
            | InstructionKind::BindReferencePropertyDim { source, .. }
            | InstructionKind::BindReferenceStaticProperty { source, .. } => {
                locals.remove(source);
            }
            InstructionKind::ForeachNextRef { value_local, .. } => {
                locals.remove(value_local);
            }
            _ => {}
        }
    }
    locals
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
                    let stack_outer = stack
                        .iter()
                        .rev()
                        .filter_map(|index| handlers.get(*index as usize))
                        .find_map(|handler| handler.finally);
                    let static_outer = handlers
                        .iter()
                        .position(|handler| handler.finally == Some(block.id))
                        .and_then(|current_index| {
                            let current_block = handlers[current_index].block;
                            handlers[..current_index]
                                .iter()
                                .rev()
                                .find(|handler| handler.protected_blocks.contains(&current_block))
                                .and_then(|handler| handler.finally)
                        });
                    *outer_finally = static_outer.or(stack_outer);
                }
                _ => {}
            }
        }
        let stack_finally = stack
            .iter()
            .rev()
            .filter_map(|index| handlers.get(*index as usize))
            .find_map(|handler| handler.finally);
        // Data-flow joins deliberately retain only a common handler-stack
        // prefix. A return in a nested protected body can therefore lose its
        // inner handler when another path reaches the same block. The static
        // exception regions retain the precise nesting for protected blocks;
        // prefer their innermost handler and use the stack for returns from a
        // finally body itself.
        let pending_finally = handlers
            .iter()
            .rev()
            .find(|handler| handler.protected_blocks.contains(&block.id))
            .and_then(|handler| handler.finally)
            .or(stack_finally);
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct LocalBitSet {
    words: Vec<u64>,
}

impl LocalBitSet {
    fn empty(word_count: usize) -> Self {
        Self {
            words: vec![0; word_count],
        }
    }

    fn insert(&mut self, local: LocalId) {
        let index = local.index();
        self.words[index / u64::BITS as usize] |= 1_u64 << (index % u64::BITS as usize);
    }

    fn union_with(&mut self, other: &Self) {
        for (word, other) in self.words.iter_mut().zip(&other.words) {
            *word |= *other;
        }
    }

    fn intersect_with_out(&mut self, incoming: &Self, definitions: &Self) {
        for ((word, incoming), definitions) in self
            .words
            .iter_mut()
            .zip(&incoming.words)
            .zip(&definitions.words)
        {
            *word &= *incoming | *definitions;
        }
    }

    fn replace_with_out(&mut self, incoming: &Self, definitions: &Self) {
        for ((word, incoming), definitions) in self
            .words
            .iter_mut()
            .zip(&incoming.words)
            .zip(&definitions.words)
        {
            *word = *incoming | *definitions;
        }
    }

    fn to_locals(&self) -> Vec<LocalId> {
        let mut locals = Vec::new();
        for (word_index, word) in self.words.iter().copied().enumerate() {
            let mut remaining = word;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                let index = u32::try_from(word_index * u64::BITS as usize + bit)
                    .expect("local bitset index derives from LocalId");
                locals.push(LocalId::new(index));
                remaining &= remaining - 1;
            }
        }
        locals
    }
}

fn populate_live_locals(blocks: &mut [RegionBlock], params: &[LocalId]) {
    let mut definition_ids = Vec::with_capacity(blocks.len());
    let mut predecessors = vec![Vec::<usize>::new(); blocks.len()];
    let mut local_count = params
        .iter()
        .map(|local| local.index().saturating_add(1))
        .max()
        .unwrap_or(0);
    for block in blocks.iter() {
        let mut defs = Vec::new();
        for instruction in &block.instructions {
            if let Some(local) = native_local_state_definition(&instruction.kind) {
                defs.push(local);
                local_count = local_count.max(local.index().saturating_add(1));
            }
        }
        definition_ids.push(defs);
        for target in block.terminator.targets() {
            if let Some(target_predecessors) = predecessors.get_mut(target.index()) {
                target_predecessors.push(block.id.index());
            }
        }
    }

    let word_count = local_count.div_ceil(u64::BITS as usize);
    let mut candidates = LocalBitSet::empty(word_count);
    let mut definitions = Vec::with_capacity(blocks.len());
    for defs in definition_ids {
        let mut definition = LocalBitSet::empty(word_count);
        for local in defs {
            definition.insert(local);
            candidates.insert(local);
        }
        definitions.push(definition);
    }
    let mut entry = LocalBitSet::empty(word_count);
    for local in params {
        entry.insert(*local);
        candidates.insert(*local);
    }
    let mut initialized_in = vec![candidates.clone(); blocks.len()];
    if let Some(first) = initialized_in.first_mut() {
        *first = entry.clone();
    }
    let mut incoming = LocalBitSet::empty(word_count);
    loop {
        let mut changed = false;
        for block_index in 1..blocks.len() {
            let Some((first, rest)) = predecessors[block_index].split_first() else {
                continue;
            };
            incoming.replace_with_out(&initialized_in[*first], &definitions[*first]);
            for predecessor in rest {
                incoming
                    .intersect_with_out(&initialized_in[*predecessor], &definitions[*predecessor]);
            }
            if initialized_in[block_index] != incoming {
                initialized_in[block_index].clone_from(&incoming);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut materialized_in = vec![LocalBitSet::empty(word_count); blocks.len()];
    if let Some(first) = materialized_in.first_mut() {
        *first = entry.clone();
    }
    loop {
        let mut changed = false;
        for block_index in 1..blocks.len() {
            incoming.words.fill(0);
            for predecessor in &predecessors[block_index] {
                incoming.union_with(&materialized_in[*predecessor]);
                incoming.union_with(&definitions[*predecessor]);
            }
            if materialized_in[block_index] != incoming {
                materialized_in[block_index].clone_from(&incoming);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for ((block, incoming), state_incoming) in
        blocks.iter_mut().zip(initialized_in).zip(materialized_in)
    {
        let mut initialized = incoming;
        let mut materialized = state_incoming;
        block.entry_live_locals = initialized.to_locals();
        block.entry_state_locals = materialized.to_locals();
        for instruction in &mut block.instructions {
            instruction.live_locals = initialized.to_locals();
            if let Some(local) = native_local_state_definition(&instruction.kind) {
                initialized.insert(local);
                materialized.insert(local);
            }
        }
        block.terminator_live_locals = initialized.to_locals();
        block.terminator_state_locals = materialized.to_locals();
    }
}

const fn native_local_state_definition(kind: &RegionInstructionKind) -> Option<LocalId> {
    match kind {
        RegionInstructionKind::StoreLocal { local, .. }
        | RegionInstructionKind::AssignLocalResult { local, .. }
        | RegionInstructionKind::UnsetLocal { local }
        | RegionInstructionKind::AssignDim { local, .. }
        | RegionInstructionKind::AppendDim { local, .. }
        | RegionInstructionKind::UnsetDim { local, .. }
        | RegionInstructionKind::InitStaticLocal { local, .. } => Some(*local),
        RegionInstructionKind::BindReference { target, .. }
        | RegionInstructionKind::BindReferenceDim { target, .. }
        | RegionInstructionKind::BindReferenceFromProperty { target, .. }
        | RegionInstructionKind::BindReferenceFromPropertyDim { target, .. } => Some(*target),
        RegionInstructionKind::BindReferenceIntoDim { array, .. }
        | RegionInstructionKind::BindReferenceDimFromProperty { array, .. } => Some(*array),
        RegionInstructionKind::ForeachNextRef { value_local, .. } => Some(*value_local),
        RegionInstructionKind::NativeCall(RegionNativeCall {
            result: RegionCallResult::ReferenceLocal(local),
            ..
        }) => Some(*local),
        _ => None,
    }
}

const fn lower_binary(op: BinaryOp) -> RegionBinaryOp {
    match op {
        BinaryOp::Add => RegionBinaryOp::Add,
        BinaryOp::Sub => RegionBinaryOp::Sub,
        BinaryOp::Mul => RegionBinaryOp::Mul,
        BinaryOp::Div => RegionBinaryOp::Div,
        BinaryOp::Mod => RegionBinaryOp::Mod,
        BinaryOp::Concat => RegionBinaryOp::Concat,
        BinaryOp::Pow => RegionBinaryOp::Pow,
        BinaryOp::BitAnd => RegionBinaryOp::BitAnd,
        BinaryOp::BitOr => RegionBinaryOp::BitOr,
        BinaryOp::BitXor => RegionBinaryOp::BitXor,
        BinaryOp::ShiftLeft => RegionBinaryOp::ShiftLeft,
        BinaryOp::ShiftRight => RegionBinaryOp::ShiftRight,
    }
}

const fn lower_unary(op: php_ir::UnaryOp) -> RegionUnaryOp {
    match op {
        php_ir::UnaryOp::Plus => RegionUnaryOp::Plus,
        php_ir::UnaryOp::Minus => RegionUnaryOp::Minus,
        php_ir::UnaryOp::Not => RegionUnaryOp::Not,
        php_ir::UnaryOp::BitNot => RegionUnaryOp::BitNot,
    }
}

const fn lower_compare(op: CompareOp) -> RegionCompareOpCode {
    match op {
        CompareOp::Equal => RegionCompareOpCode::Equal,
        CompareOp::NotEqual => RegionCompareOpCode::NotEqual,
        CompareOp::Identical => RegionCompareOpCode::Identical,
        CompareOp::NotIdentical => RegionCompareOpCode::NotIdentical,
        CompareOp::Less => RegionCompareOpCode::Less,
        CompareOp::LessEqual => RegionCompareOpCode::LessEqual,
        CompareOp::Greater => RegionCompareOpCode::Greater,
        CompareOp::GreaterEqual => RegionCompareOpCode::GreaterEqual,
        CompareOp::Spaceship => RegionCompareOpCode::Spaceship,
    }
}

const fn lower_cast(op: php_ir::CastKind) -> RegionCastOp {
    match op {
        php_ir::CastKind::Bool => RegionCastOp::Bool,
        php_ir::CastKind::Int => RegionCastOp::Int,
        php_ir::CastKind::Float => RegionCastOp::Float,
        php_ir::CastKind::String => RegionCastOp::String,
        php_ir::CastKind::Array => RegionCastOp::Array,
        php_ir::CastKind::Object => RegionCastOp::Object,
        php_ir::CastKind::Void => RegionCastOp::Void,
    }
}

fn lower_operand(unit: &IrUnit, operand: Operand) -> RegionOperand {
    match operand {
        Operand::Register(register) => RegionOperand::Register(register),
        Operand::Local(local) => RegionOperand::Local(local),
        Operand::Constant(constant) => lower_constant(unit, constant),
    }
}

fn lower_call_operands(unit: &IrUnit, args: &[IrCallArg]) -> Vec<Option<RegionOperand>> {
    args.iter()
        .map(|argument| Some(lower_operand(unit, argument.value)))
        .collect()
}

fn known_string_operand(
    unit: &IrUnit,
    operand: Operand,
    registers: &BTreeMap<RegId, String>,
) -> Option<String> {
    match operand {
        Operand::Register(register) => registers.get(&register).cloned(),
        Operand::Constant(constant) => match unit.constants.get(constant.index()) {
            Some(IrConstant::String(value)) => Some(value.clone()),
            _ => None,
        },
        Operand::Local(_) => None,
    }
}

fn known_callable_operand_name(
    unit: &IrUnit,
    operand: Operand,
    register_strings: &BTreeMap<RegId, String>,
    local_strings: &BTreeMap<LocalId, String>,
    register_callables: &BTreeMap<RegId, String>,
    local_callables: &BTreeMap<LocalId, String>,
) -> Option<String> {
    match operand {
        Operand::Register(register) => register_callables
            .get(&register)
            .or_else(|| register_strings.get(&register))
            .cloned(),
        Operand::Local(local) => local_callables
            .get(&local)
            .or_else(|| local_strings.get(&local))
            .cloned(),
        Operand::Constant(_) => known_string_operand(unit, operand, register_strings),
    }
}

fn native_global_site_name(
    unit: &IrUnit,
    function: &php_ir::IrFunction,
    instruction: &InstructionKind,
    strings: &BTreeMap<RegId, String>,
    globals: &BTreeSet<RegId>,
) -> Option<String> {
    let local_is_globals = |local: LocalId| {
        function
            .locals
            .get(local.index())
            .is_some_and(|name| name == "GLOBALS")
    };
    let first_dimension = |dimensions: &[Operand]| {
        dimensions
            .first()
            .and_then(|operand| known_string_operand(unit, *operand, strings))
    };
    let name = match instruction {
        InstructionKind::FetchDim { array, key, .. } if matches!(array, Operand::Register(register) if globals.contains(register)) => {
            known_string_operand(unit, *key, strings)
        }
        InstructionKind::ArrayGet { array, index, .. } if matches!(array, Operand::Register(register) if globals.contains(register)) => {
            known_string_operand(unit, *index, strings)
        }
        InstructionKind::AssignDim { local, dims, .. }
        | InstructionKind::AppendDim { local, dims, .. }
        | InstructionKind::IssetDim { local, dims, .. }
        | InstructionKind::EmptyDim { local, dims, .. }
        | InstructionKind::UnsetDim { local, dims }
        | InstructionKind::BindReferenceDim { local, dims, .. }
        | InstructionKind::BindReferenceFromDim { local, dims, .. }
            if local_is_globals(*local) =>
        {
            first_dimension(dims)
        }
        _ => None,
    }?;
    (name != "GLOBALS").then_some(name)
}

fn find_function(unit: &IrUnit, name: &str) -> Option<FunctionId> {
    let normalized = name.trim_start_matches('\\');
    unit.function_table
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(normalized))
        .map(|entry| entry.function)
}

fn find_class<'a>(unit: &'a IrUnit, name: &str) -> Option<(u32, &'a php_ir::module::ClassEntry)> {
    let normalized = php_ir::module::normalize_class_name(name);
    unit.classes
        .iter()
        .enumerate()
        .find(|(_, class)| php_ir::module::normalize_class_name(&class.name) == normalized)
        .and_then(|(index, class)| u32::try_from(index).ok().map(|index| (index, class)))
}

fn find_direct_static_method(unit: &IrUnit, class_name: &str, method: &str) -> Option<FunctionId> {
    find_class(unit, class_name)
        .and_then(|(_, class)| {
            class
                .methods
                .iter()
                .find(|entry| {
                    entry.name.eq_ignore_ascii_case(method)
                        && entry.flags.is_static
                        && !entry.flags.is_private
                        && !entry.flags.is_protected
                })
                .map(|entry| entry.function)
        })
        .filter(|_| !class_name.eq_ignore_ascii_case("static"))
        .filter(|function| {
            unit.functions
                .get(function.index())
                .is_some_and(|function| {
                    !function
                        .blocks
                        .iter()
                        .flat_map(|block| &block.instructions)
                        .any(|instruction| {
                            matches!(
                                &instruction.kind,
                                InstructionKind::FetchClassConstant {
                                    class_name,
                                    ..
                                }
                                    | InstructionKind::CallStaticMethod {
                                        class_name,
                                        ..
                                    } if class_name.eq_ignore_ascii_case("static")
                            )
                        })
                })
        })
}

fn publication_constant_is_stable(constant: &IrConstant) -> bool {
    match constant {
        IrConstant::Null
        | IrConstant::Bool(_)
        | IrConstant::Int(_)
        | IrConstant::Float(_)
        | IrConstant::String(_)
        | IrConstant::StringBytes(_) => true,
        IrConstant::Array(entries) => entries.iter().all(|entry| {
            entry
                .key
                .as_ref()
                .is_none_or(publication_constant_is_stable)
                && publication_constant_is_stable(&entry.value)
        }),
        IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => false,
    }
}

fn class_has_publication_stable_layout(unit: &IrUnit, class_index: u32) -> bool {
    let Some(mut class) = unit.classes.get(class_index as usize) else {
        return false;
    };
    let mut visited = std::collections::BTreeSet::new();
    loop {
        if class.flags.is_abstract
            || class.flags.is_interface
            || class.flags.is_trait
            || class.flags.is_enum
            || !visited.insert(class.name.as_str())
            || class.properties.iter().any(|property| {
                property
                    .default
                    .and_then(|constant| unit.constants.get(constant.index()))
                    .is_some_and(|constant| !publication_constant_is_stable(constant))
            })
        {
            return false;
        }
        let Some(parent) = class.parent.as_deref() else {
            return true;
        };
        let Some((_, parent)) = find_class(unit, parent) else {
            // Runtime/internal and dynamically supplied parents need their
            // exact baseline declaration boundary before publication.
            return false;
        };
        class = parent;
    }
}

fn class_has_clone_method(unit: &IrUnit, class_index: u32) -> bool {
    let Some(mut class) = unit.classes.get(class_index as usize) else {
        return true;
    };
    let mut visited = std::collections::BTreeSet::new();
    loop {
        if !visited.insert(class.name.as_str())
            || class
                .methods
                .iter()
                .any(|method| method.name.eq_ignore_ascii_case("__clone"))
        {
            return true;
        }
        let Some(parent) = class.parent.as_deref() else {
            return false;
        };
        let Some((_, parent)) = find_class(unit, parent) else {
            // An external parent may supply magic clone semantics.
            return true;
        };
        class = parent;
    }
}

fn known_object_class(operand: Operand, registers: &BTreeMap<RegId, u32>) -> Option<u32> {
    match operand {
        Operand::Register(register) => registers.get(&register).copied(),
        Operand::Local(_) | Operand::Constant(_) => None,
    }
}

fn prepared_exact_object_class(
    unit: &IrUnit,
    operand: Operand,
    known_registers: &BTreeMap<RegId, u32>,
    exact_registers: &BTreeSet<RegId>,
) -> Option<u32> {
    let Operand::Register(register) = operand else {
        return None;
    };
    exact_registers
        .contains(&register)
        .then(|| known_registers.get(&register).copied())
        .flatten()
        .filter(|class| class_has_publication_stable_layout(unit, *class))
}

fn returned_closure(unit: &IrUnit, name: &str, args: &[IrCallArg]) -> Option<KnownClosure> {
    let target_id = find_function(unit, name)?;
    let target = unit.functions.get(target_id.index())?;
    let (closure_register, closure_function, captures) = target
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match &instruction.kind {
            InstructionKind::MakeClosure {
                dst,
                function,
                captures,
            } => Some((*dst, *function, captures)),
            _ => None,
        })?;
    let returned = target.blocks.iter().any(|block| {
        block.terminator.as_ref().is_some_and(|terminator| {
            matches!(
                &terminator.kind,
                TerminatorKind::Return {
                    value: Some(Operand::Register(register)),
                    ..
                } if *register == closure_register
            )
        })
    });
    if !returned {
        return None;
    }
    let captures = captures
        .iter()
        .map(|capture| {
            let Operand::Local(local) = capture.src else {
                return None;
            };
            let parameter = target
                .params
                .iter()
                .position(|parameter| parameter.local == local)?;
            let argument = args.get(parameter)?;
            argument
                .by_ref_local
                .map(RegionOperand::Local)
                .or_else(|| Some(lower_operand(unit, argument.value)))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(KnownClosure {
        function: closure_function,
        captures,
        bound_object: None,
        requires_runtime_context: true,
    })
}

fn lower_direct_function_call(
    unit: &IrUnit,
    dst: RegId,
    name: String,
    function: FunctionId,
    args: &[IrCallArg],
) -> RegionInstructionKind {
    let target = &unit.functions[function.index()];
    let direct_arity = (!omitted_defaults_require_runtime_binding(target, args.len()))
        .then(|| u32::try_from(target.params.len()).ok())
        .flatten();
    let is_generator = target.flags.is_generator
        || target
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| {
                matches!(
                    instruction.kind,
                    InstructionKind::Yield { .. } | InstructionKind::YieldFrom { .. }
                )
            });
    let variadic = target
        .params
        .last()
        .is_some_and(|parameter| parameter.variadic);
    let mut operands = lower_call_operands(unit, args);
    for parameter in target
        .params
        .iter()
        .skip(args.len())
        .filter(|parameter| !parameter.variadic)
    {
        let operand = parameter.default.as_ref().and_then(|default| {
            unit.constants
                .iter()
                .position(|constant| constant == default)
                .and_then(|index| u32::try_from(index).ok())
                .map(RegionOperand::Constant)
        });
        operands.push(operand);
    }
    RegionInstructionKind::NativeCall(RegionNativeCall {
        result: RegionCallResult::Register(dst),
        target: RegionCallTarget::Function {
            name,
            function: (!is_generator).then_some(function),
        },
        args: args.to_vec(),
        argument_operand_offset: 0,
        operands,
        direct_arity,
        variadic,
        returns_by_reference: target.returns_by_ref,
        caller_strict_types: unit.strict_types,
    })
}

fn lower_stable_named_callable(
    unit: &IrUnit,
    dst: RegId,
    name: String,
    args: &[IrCallArg],
) -> (RegionInstructionKind, bool) {
    let direct_shape = args
        .iter()
        .all(|argument| argument.name.is_none() && !argument.unpack);
    if let Some((class_name, method)) = name.split_once("::") {
        if direct_shape && let Some(function) = find_direct_static_method(unit, class_name, method)
        {
            return (
                lower_direct_function_call(unit, dst, name, function, args),
                true,
            );
        }
        return (
            RegionInstructionKind::NativeCall(RegionNativeCall {
                result: RegionCallResult::Register(dst),
                target: RegionCallTarget::StaticMethod {
                    class_name: class_name.to_owned(),
                    method: method.to_owned(),
                },
                args: args.to_vec(),
                argument_operand_offset: 0,
                operands: lower_call_operands(unit, args),
                direct_arity: None,
                variadic: false,
                returns_by_reference: false,
                caller_strict_types: unit.strict_types,
            }),
            false,
        );
    }
    if let Some(function) = find_function(unit, &name) {
        return (
            lower_direct_function_call(unit, dst, name, function, args),
            direct_shape,
        );
    }
    (
        RegionInstructionKind::NativeCall(RegionNativeCall {
            result: RegionCallResult::Register(dst),
            target: RegionCallTarget::Function {
                name,
                function: None,
            },
            args: args.to_vec(),
            argument_operand_offset: 0,
            operands: lower_call_operands(unit, args),
            direct_arity: None,
            variadic: false,
            returns_by_reference: false,
            caller_strict_types: unit.strict_types,
        }),
        false,
    )
}

fn stable_named_callable_is_by_value_only(unit: &IrUnit, name: &str) -> bool {
    let local_target = name
        .split_once("::")
        .and_then(|(class_name, method)| find_direct_static_method(unit, class_name, method))
        .or_else(|| find_function(unit, name));
    if let Some(function) = local_target {
        return unit
            .functions
            .get(function.index())
            .is_some_and(|function| function.params.iter().all(|parameter| !parameter.by_ref));
    }
    let normalized = name.trim_start_matches('\\');
    !normalized.contains("::")
        && php_std::arginfo::function_metadata_indexed(normalized)
            .is_some_and(|function| function.params.iter().all(|parameter| !parameter.by_ref))
}

fn lower_direct_method_call(
    unit: &IrUnit,
    dst: RegId,
    function: FunctionId,
    receiver: Operand,
    args: &[IrCallArg],
) -> RegionInstructionKind {
    let target = &unit.functions[function.index()];
    let is_static = unit.classes.iter().any(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function == function && method.flags.is_static)
    });
    let variadic = target
        .params
        .last()
        .is_some_and(|parameter| parameter.variadic);
    let receiver_count = usize::from(!is_static);
    let direct_arity = (!omitted_defaults_require_runtime_binding(target, args.len()))
        .then(|| u32::try_from(receiver_count + target.params.len()).ok())
        .flatten();
    let mut operands = if is_static {
        Vec::new()
    } else {
        vec![Some(lower_operand(unit, receiver))]
    };
    operands.extend(lower_call_operands(unit, args));
    for parameter in target
        .params
        .iter()
        .skip(args.len())
        .filter(|parameter| !parameter.variadic)
    {
        let operand = parameter.default.as_ref().and_then(|default| {
            unit.constants
                .iter()
                .position(|constant| constant == default)
                .and_then(|index| u32::try_from(index).ok())
                .map(RegionOperand::Constant)
        });
        operands.push(operand);
    }
    RegionInstructionKind::NativeCall(RegionNativeCall {
        result: RegionCallResult::Register(dst),
        target: RegionCallTarget::Function {
            name: target.name.clone(),
            function: Some(function),
        },
        args: args.to_vec(),
        argument_operand_offset: receiver_count,
        operands,
        direct_arity,
        variadic,
        returns_by_reference: target.returns_by_ref,
        caller_strict_types: unit.strict_types,
    })
}

fn lower_direct_closure_call(
    unit: &IrUnit,
    dst: RegId,
    closure: KnownClosure,
    args: &[IrCallArg],
    semantic_context: RegionSemanticContext,
) -> RegionInstructionKind {
    let target = &unit.functions[closure.function.index()];
    let variadic = target
        .params
        .last()
        .is_some_and(|parameter| parameter.variadic);
    if let Some(bound_object) = closure.bound_object
        && target.blocks.iter().any(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(
                    &instruction.kind,
                    InstructionKind::FetchClassConstant {
                        class_name,
                        constant,
                        ..
                    } if class_name.eq_ignore_ascii_case("static")
                        && constant.eq_ignore_ascii_case("class")
                )
            })
        })
    {
        return RegionInstructionKind::NativeCall(RegionNativeCall {
            result: RegionCallResult::Register(dst),
            target: RegionCallTarget::Semantic {
                operation: RegionSemanticOp::BoundClosureClass {
                    context: semantic_context,
                    bound_object,
                },
            },
            args: Vec::new(),
            argument_operand_offset: 0,
            operands: vec![Some(bound_object)],
            direct_arity: None,
            variadic: false,
            returns_by_reference: false,
            caller_strict_types: unit.strict_types,
        });
    }
    let bound_object_count = usize::from(closure.bound_object.is_some());
    let direct_arity = (!omitted_defaults_require_runtime_binding(target, args.len()))
        .then(|| {
            u32::try_from(bound_object_count + target.captures.len() + target.params.len()).ok()
        })
        .flatten();
    let argument_operand_offset = bound_object_count + closure.captures.len();
    let mut operands = closure
        .bound_object
        .into_iter()
        .map(Some)
        .collect::<Vec<_>>();
    operands.extend(closure.captures.into_iter().map(Some));
    operands.extend(lower_call_operands(unit, args));
    for parameter in target
        .params
        .iter()
        .skip(args.len())
        .filter(|parameter| !parameter.variadic)
    {
        let operand = parameter.default.as_ref().and_then(|default| {
            unit.constants
                .iter()
                .position(|constant| constant == default)
                .and_then(|index| u32::try_from(index).ok())
                .map(RegionOperand::Constant)
        });
        operands.push(operand);
    }
    RegionInstructionKind::NativeCall(RegionNativeCall {
        result: RegionCallResult::Register(dst),
        target: RegionCallTarget::Function {
            name: target.name.clone(),
            function: Some(closure.function),
        },
        args: args.to_vec(),
        argument_operand_offset,
        operands,
        direct_arity,
        variadic,
        returns_by_reference: target.returns_by_ref,
        caller_strict_types: unit.strict_types,
    })
}

fn lower_constant(unit: &IrUnit, constant: php_ir::ConstId) -> RegionOperand {
    match unit.constants.get(constant.index()) {
        Some(IrConstant::Int(value)) => RegionOperand::I64(*value),
        Some(IrConstant::Null) => RegionOperand::Constant(u32::MAX),
        Some(IrConstant::Bool(false)) => RegionOperand::Constant(crate::JIT_VALUE_FALSE),
        Some(IrConstant::Bool(true)) => RegionOperand::Constant(crate::JIT_VALUE_TRUE),
        Some(_) | None => RegionOperand::Constant(constant.raw()),
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
            condition: lower_operand(unit, *condition),
            target: *target,
            fallthrough: fallthrough()?,
        }),
        TerminatorKind::JumpIfTrue { condition, target } => Ok(RegionTerminator::JumpIfTrue {
            condition: lower_operand(unit, *condition),
            target: *target,
            fallthrough: fallthrough()?,
        }),
        TerminatorKind::JumpIf {
            condition,
            if_true,
            if_false,
        } => Ok(RegionTerminator::JumpIf {
            condition: lower_operand(unit, *condition),
            if_true: *if_true,
            if_false: *if_false,
        }),
        TerminatorKind::Return {
            value: Some(value),
            by_ref_local: None,
        } => Ok(RegionTerminator::Return {
            value: lower_operand(unit, *value),
            finally: None,
        }),
        TerminatorKind::Return { value: None, .. } => Ok(RegionTerminator::Return {
            value: RegionOperand::Constant(u32::MAX),
            finally: None,
        }),
        TerminatorKind::Return {
            value: Some(_),
            by_ref_local: Some(local),
        } => Ok(RegionTerminator::ReturnReference {
            local: *local,
            finally: None,
        }),
        TerminatorKind::Exit { value } => Ok(RegionTerminator::Exit {
            value: value.map(|value| lower_operand(unit, value)),
            finally: None,
        }),
    }
}

#[cfg(test)]
#[path = "executable_tests.rs"]
mod tests;
