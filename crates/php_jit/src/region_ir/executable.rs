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
    /// Immutable default loaded from a linked callee's literal table.
    LinkedConstant {
        link_index: u32,
        constant: u32,
        class: super::SsaValueClass,
    },
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
        function: Option<FunctionId>,
        linked_function: Option<u32>,
        receiver_layout_id: Option<u64>,
    },
    StaticMethod {
        class_name: String,
        method: String,
    },
    Closure {
        callee: Operand,
        /// Exact closure body admitted at Region construction. `None` keeps
        /// the ordinary runtime-resolved callable boundary.
        function: Option<FunctionId>,
        /// Packed leading receiver and capture slots loaded from the
        /// authoritative prepared closure record by optimizing code.
        bound_object_count: usize,
        capture_count: usize,
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

/// Compile-time identity of a callback whose native userland entry and
/// declaration contract are already published.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionStableCallback {
    pub name: String,
    pub function: Option<FunctionId>,
    /// Exact object receiver carried directly into an instance-method entry.
    /// Callable-array construction is dead after this plan is selected.
    pub receiver: Option<RegionOperand>,
    /// Exact prepared closure whose immutable native record supplies the
    /// hidden receiver and captures. Named callbacks leave this empty.
    pub closure: Option<RegionOperand>,
    pub bound_object_count: usize,
    pub capture_count: usize,
    /// Publication-time proof that every successful callback return is an
    /// integer. Callback sorts use this to avoid a post-callback coercion
    /// side exit that would otherwise repeat PHP-visible callback effects.
    pub returns_int: bool,
    /// Publication-time proof that every successful callback return is an
    /// authoritative native string. PCRE replacement uses this to avoid a
    /// post-callback coercion exit.
    pub returns_string: bool,
    /// Publication-time proof that every successful callback return is a
    /// non-reference native scalar. PCRE uses the shared terminal coercion
    /// boundary for these results without admitting arrays/objects.
    pub returns_releasable_scalar: bool,
}

/// One callback boundary selected before an array loop has observable effects.
///
/// Stable callbacks carry their already-linked declaration. Runtime callbacks
/// carry the authoritative operand and are acquired once into the same fixed
/// packed native-entry contract; no per-element callable dispatch remains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionArrayCallbackTarget {
    Stable(RegionStableCallback),
    Runtime(RegionOperand),
}

impl RegionArrayCallbackTarget {
    #[must_use]
    pub fn stable(&self) -> Option<&RegionStableCallback> {
        match self {
            Self::Stable(callback) => Some(callback),
            Self::Runtime(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionArrayCallbackOperation {
    Map,
    FilterValue,
    FilterKey,
    FilterValueAndKey,
    Reduce,
    All,
    Any,
    Find,
    FindKey,
    Usort,
    Uasort,
    Uksort,
    Walk,
    WalkRecursive,
    PregReplace,
}

/// One native array loop with a fixed compiled callback entry. The arrays
/// remain ordinary authoritative native operands; no callback or argument
/// `Value` is constructed at runtime.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionArrayCallbackCall {
    pub result: RegId,
    pub operation: RegionArrayCallbackOperation,
    pub callback: RegionArrayCallbackTarget,
    pub arrays: Vec<RegionOperand>,
    pub initial: Option<RegionOperand>,
    /// Direct caller-local lvalue mutated by callback sort operations.
    pub mutable_local: Option<LocalId>,
    pub caller_strict_types: bool,
}

/// One pattern/callback pair from a locally constructed
/// `preg_replace_callback_array()` map.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionPregCallbackArrayEntry {
    pub pattern: RegionOperand,
    pub callback: RegionArrayCallbackTarget,
}

/// One sequential native PCRE callback-map boundary.
///
/// The entries remain ordered because PHP exposes callback effects from an
/// earlier pattern even when a later pattern or callback fails.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionPregCallbackArrayCall {
    pub result: RegId,
    pub entries: Vec<RegionPregCallbackArrayEntry>,
    pub subject: RegionOperand,
    pub limit: RegionOperand,
    pub count_local: Option<LocalId>,
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
        /// Exact caller-local carrying the object implicitly bound to this
        /// Closure. Static methods have no receiver, and a Closure that
        /// explicitly captures `$this` has no separate implicit operand.
        bound_this_local: Option<LocalId>,
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
    /// Resolves PHP-visible source arguments to the callee's packed native
    /// parameter order. Fixed parameters occupy one entry each; omitted
    /// defaults are `None`, while a variadic parameter contributes one entry
    /// per supplied positional tail value. Named variadic entries remain on
    /// the exact baseline binder because their string keys are PHP-visible.
    pub(crate) fn prepared_argument_sources(
        &self,
        parameters: &[php_ir::IrParam],
    ) -> Option<Vec<Option<usize>>> {
        prepared_call_argument_plan(&self.args, parameters).map(|plan| plan.parameter_sources)
    }

    pub(crate) fn prepared_argument_plan(
        &self,
        parameters: &[php_ir::IrParam],
    ) -> Option<PreparedCallArgumentPlan> {
        prepared_call_argument_plan(&self.args, parameters)
    }

    /// Returns the source index of the one direct-array segment that can be
    /// expanded entirely in generated code. Multiple/interleaved unpack
    /// segments and named unpack keys retain their exact baseline binder.
    pub(crate) fn trailing_unpack_argument(&self) -> Option<usize> {
        let (last, prefix) = self.args.split_last()?;
        (last.unpack
            && last.name.is_none()
            && prefix
                .iter()
                .all(|argument| argument.name.is_none() && !argument.unpack)
            && self.operands.len() == self.argument_operand_offset.saturating_add(self.args.len())
            && self.operands.iter().all(Option::is_some))
        .then_some(prefix.len())
    }

    /// Statically bound callee for a trailing direct-array argument segment.
    /// Parameter-count, type and by-reference admission is performed by the
    /// optimizing compiler with the published signature.
    pub(crate) fn direct_compiled_unpack_target(&self) -> Option<FunctionId> {
        let function = match self.target {
            RegionCallTarget::Function {
                function: Some(function),
                ..
            }
            | RegionCallTarget::Closure {
                function: Some(function),
                ..
            }
            | RegionCallTarget::Method {
                function: Some(function),
                linked_function: None,
                receiver_layout_id: Some(_),
                ..
            } => function,
            _ => return None,
        };
        self.trailing_unpack_argument().map(|_| function)
    }

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
        if self.direct_compiled_target().is_some() {
            return has_location
                && argument.value_kind == IrCallArgValueKind::ByRefLocationPlaceholder;
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
        let function = match self.target {
            RegionCallTarget::Function {
                function: Some(function),
                ..
            }
            | RegionCallTarget::Closure {
                function: Some(function),
                ..
            }
            | RegionCallTarget::Method {
                function: Some(function),
                linked_function: None,
                receiver_layout_id: Some(_),
                ..
            } => function,
            _ => return None,
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
                !arg.unpack
                    && (arg.value_kind == IrCallArgValueKind::Direct
                        || (arg.value_kind == IrCallArgValueKind::ByRefLocationPlaceholder
                            && arg.by_ref_local.is_some()))
            }))
        .then_some(function)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreparedCallArgumentPlan {
    /// Fixed parameter sources followed by positional variadic sources.
    pub(crate) parameter_sources: Vec<Option<usize>>,
    /// Leading fixed parameter values visible through func_get_arg(s).
    pub(crate) visible_fixed_count: usize,
    /// Positional variadic sources visible after the fixed prefix.
    pub(crate) visible_variadic_sources: Vec<usize>,
    /// Surplus positional sources accepted by PHP but absent from the native
    /// callee parameter ABI.
    pub(crate) extra_sources: Vec<usize>,
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
    /// Baseline register identities required to resume this exact
    /// continuation. Optimizing rewrites may add current operands, but they
    /// must retain and publish these source-tier snapshot roots unchanged.
    pub transition_live_registers: Option<Vec<RegId>>,
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
                if let RegionCallTarget::Closure {
                    callee,
                    function: Some(_),
                    ..
                } = call.target
                    && let Operand::Register(register) = callee
                {
                    // The exact prepared closure is the source of bound
                    // receiver/capture values, but it is deliberately not a
                    // packed callee-frame operand. Keep that independent
                    // source value live across native fragment boundaries.
                    push(RegionOperand::Register(register));
                }
                for operand in call.operands.iter().flatten() {
                    push(*operand);
                }
                let mut push_ir = |operand: Operand| {
                    if let Operand::Register(register) = operand {
                        uses.push(register);
                    }
                };
                for argument in &call.args {
                    if argument.value_kind != IrCallArgValueKind::ByRefLocationPlaceholder
                        || argument.by_ref_local.is_none()
                    {
                        push_ir(argument.value);
                    }
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
            RegionInstructionKind::ArrayCallback(call) => {
                match &call.callback {
                    RegionArrayCallbackTarget::Stable(callback) => {
                        if let Some(receiver) = callback.receiver {
                            push(receiver);
                        }
                        if let Some(closure) = callback.closure {
                            push(closure);
                        }
                    }
                    RegionArrayCallbackTarget::Runtime(callback) => push(*callback),
                }
                for array in &call.arrays {
                    push(*array);
                }
                if let Some(initial) = call.initial {
                    push(initial);
                }
            }
            RegionInstructionKind::PregCallbackArray(call) => {
                for entry in &call.entries {
                    push(entry.pattern);
                    match &entry.callback {
                        RegionArrayCallbackTarget::Stable(callback) => {
                            if let Some(receiver) = callback.receiver {
                                push(receiver);
                            }
                            if let Some(closure) = callback.closure {
                                push(closure);
                            }
                        }
                        RegionArrayCallbackTarget::Runtime(callback) => push(*callback),
                    }
                }
                push(call.subject);
                push(call.limit);
            }
            RegionInstructionKind::BindReference { .. } => {}
            RegionInstructionKind::BindReferenceDim { keys, .. }
            | RegionInstructionKind::BindReferenceIntoDim { keys, .. } => {
                for key in keys {
                    push(*key);
                }
            }
            RegionInstructionKind::BindReferenceProperty { object, .. }
            | RegionInstructionKind::BindReferenceFromProperty { object, .. } => push(*object),
            RegionInstructionKind::BindReferenceFromPropertyDim { object, keys, .. }
            | RegionInstructionKind::BindReferenceIntoPropertyDim { object, keys, .. }
            | RegionInstructionKind::BindReferenceDimFromProperty { object, keys, .. } => {
                push(*object);
                for key in keys {
                    push(*key);
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
        // Synthesized reference preparation deliberately retains the source
        // call for diagnostics, but it does not execute or define that call's
        // result. Likewise, an elided lvalue fetch is a real no-op. Keeping
        // source-tier definitions for either shape makes fragment liveness
        // believe values exist that generated code never produced.
        if matches!(
            self.kind,
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
        ) {
            return Vec::new();
        }
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
        /// External constructor link whose target-unit class plan owns the
        /// allocation layout. Local classes leave this empty.
        linked_class: Option<u32>,
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
    ArrayCallback(RegionArrayCallbackCall),
    PregCallbackArray(RegionPregCallbackArrayCall),
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
    /// Baseline register identities required before the terminator.
    pub terminator_live_registers: Option<Vec<RegId>>,
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
                    && let Some(target) = call
                        .direct_compiled_target()
                        .or_else(|| call.direct_compiled_unpack_target())
                {
                    callees.insert(target);
                }
                if let RegionInstructionKind::ArrayCallback(call) = &instruction.kind
                    && let RegionArrayCallbackTarget::Stable(callback) = &call.callback
                    && let Some(target) = callback.function
                {
                    callees.insert(target);
                }
                if let RegionInstructionKind::PregCallbackArray(call) = &instruction.kind {
                    for entry in &call.entries {
                        if let RegionArrayCallbackTarget::Stable(callback) = &entry.callback
                            && let Some(target) = callback.function
                        {
                            callees.insert(target);
                        }
                    }
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
            if self.compile_metadata.tier == NativeCompilerTier::Optimizing {
                for instruction in &block.instructions {
                    let RegionInstructionKind::NativeCall(call) = &instruction.kind else {
                        continue;
                    };
                    for argument in &call.args {
                        if argument.value_kind == IrCallArgValueKind::ByRefLocationPlaceholder
                            && (argument.by_ref_dim.is_some()
                                || argument.by_ref_property.is_some()
                                || argument.by_ref_property_dim.is_some())
                        {
                            return Err(NativeCompileError::new(
                                "JIT_REGION_REJECT_NONCANONICAL_REFERENCE_ARGUMENT",
                                format!(
                                    "block {} instruction {} retains a superseded reference lvalue shape",
                                    block.id.raw(),
                                    instruction.id.raw()
                                ),
                            ));
                        }
                    }
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
    capture_count: usize,
    bound_object: Option<RegionOperand>,
    requires_runtime_context: bool,
}

#[derive(Clone)]
enum KnownMethodCallableTarget {
    Instance {
        receiver: RegionOperand,
        class_name: String,
    },
    Static {
        class_name: String,
    },
}

#[derive(Clone, Default)]
struct KnownMethodCallableArray {
    target: Option<KnownMethodCallableTarget>,
    method: Option<String>,
    length: u8,
    root_register: Option<RegId>,
    receiver_owner_captured: bool,
    construction_ids: BTreeSet<InstrId>,
    last_inserted: Option<Operand>,
}

struct ConsumedMethodCallableArray {
    receiver_owner: Option<RegionOperand>,
    construction_ids: BTreeSet<InstrId>,
}

#[derive(Clone, Default)]
struct KnownPregCallbackArray {
    entries: Vec<(Operand, Operand)>,
    root_register: Option<RegId>,
    construction_ids: BTreeSet<InstrId>,
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

/// Resolves the exact caller local that PHP implicitly binds to a nested
/// Closure.
///
/// Method-ness alone is insufficient: static methods also carry the method
/// flag but have no object, while a Closure's `$this` local can follow its
/// captures instead of occupying local zero. This is publication-time shape
/// resolution; generated code receives the numeric local directly.
#[must_use]
pub fn native_closure_bound_this_local(
    unit: &IrUnit,
    caller_function: FunctionId,
    closure_function: FunctionId,
) -> Option<LocalId> {
    if !closure_requires_implicit_this(unit, closure_function) {
        return None;
    }
    let caller = unit.functions.get(caller_function.index())?;
    let caller_has_bound_this = native_method_class(unit, caller_function)
        .is_some_and(|(_, is_static)| !is_static)
        || (caller.flags.is_closure && !caller.flags.is_static);
    if !caller_has_bound_this {
        return None;
    }
    caller
        .locals
        .iter()
        .position(|name| name == "this")
        .and_then(|index| u32::try_from(index).ok())
        .map(LocalId::new)
}

/// Returns a publication-time upper bound for every continuation ID a
/// function can assign.
///
/// The executable builder emits one continuation for every source
/// instruction and block terminator. Reference-aware call preparation may
/// precede a call with at most one direct binding per argument, and
/// `NewObject` may additionally reserve one allocation continuation. Keeping
/// this bound next to the builder makes demand-zero runtime tables stable
/// without constructing RegionGraphs for dormant declarations.
#[must_use]
pub fn native_continuation_capacity_upper_bound(
    unit: &IrUnit,
    function: FunctionId,
) -> Option<usize> {
    let function = unit.functions.get(function.index())?;
    let mut capacity = function.blocks.len();
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        let argument_count = match &instruction.kind {
            InstructionKind::BindReferenceFromCall { args, .. }
            | InstructionKind::BindReferenceFromMethodCall { args, .. }
            | InstructionKind::CallFunction { args, .. }
            | InstructionKind::CallMethod { args, .. }
            | InstructionKind::CallStaticMethod { args, .. }
            | InstructionKind::CallClosure { args, .. }
            | InstructionKind::CallCallable { args, .. }
            | InstructionKind::NewObject { args, .. }
            | InstructionKind::DynamicNewObject { args, .. } => args.len(),
            _ => 0,
        };
        capacity = capacity
            .saturating_add(1)
            .saturating_add(argument_count)
            .saturating_add(usize::from(matches!(
                instruction.kind,
                InstructionKind::NewObject { .. }
            )));
    }
    Some(capacity)
}

impl BaselineRegionBuilder {
    pub fn build(
        unit: &IrUnit,
        function: FunctionId,
        runtime_metadata: &CompileMetadata,
    ) -> Result<RegionGraph, NativeCompileError> {
        Self::build_with_external_function_signatures(unit, function, runtime_metadata, &[])
    }

    pub fn build_with_external_function_signatures(
        unit: &IrUnit,
        function: FunctionId,
        runtime_metadata: &CompileMetadata,
        external_function_signatures: &[crate::JitExternalFunctionSignature],
    ) -> Result<RegionGraph, NativeCompileError> {
        Self::build_with_runtime_specializations(
            unit,
            function,
            runtime_metadata,
            external_function_signatures,
            &[],
        )
    }

    pub fn build_with_runtime_specializations(
        unit: &IrUnit,
        function: FunctionId,
        runtime_metadata: &CompileMetadata,
        external_function_signatures: &[crate::JitExternalFunctionSignature],
        method_specializations: &[crate::JitMethodSpecialization],
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
        let published_external_signatures =
            if runtime_metadata.tier == NativeCompilerTier::Optimizing {
                external_function_signatures
            } else {
                &[]
            };
        let mut fast_path_operations = 0_u64;
        let mut blocks = Vec::with_capacity(ir_function.blocks.len());
        let mut next_continuation = 0_u32;
        let mut region_local_count = ir_function.local_count;
        let mut region_locals = ir_function.locals.clone();
        let mut region_register_count = ir_function.register_count;
        let exception_regions = collect_exception_regions(ir_function);
        let method_class = native_method_class(unit, function);
        let stable_callable_entries = stable_callable_local_entries(unit, ir_function);
        let mut source_register_use_counts = vec![0_usize; ir_function.register_count as usize];
        for block in &ir_function.blocks {
            for instruction in &block.instructions {
                let mut uses = Vec::new();
                php_ir::instruction_register_uses(&instruction.kind, &mut uses);
                for register in uses {
                    if let Some(count) = source_register_use_counts.get_mut(register.index()) {
                        *count = count.saturating_add(1);
                    }
                }
            }
            if let Some(terminator) = &block.terminator {
                let mut uses = Vec::new();
                php_ir::terminator_register_uses(&terminator.kind, &mut uses);
                for register in uses {
                    if let Some(count) = source_register_use_counts.get_mut(register.index()) {
                        *count = count.saturating_add(1);
                    }
                }
            }
        }
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
            let mut known_external_object_registers = BTreeMap::<RegId, String>::new();
            let mut known_external_object_locals = BTreeMap::<LocalId, String>::new();
            let mut known_method_callable_registers =
                BTreeMap::<RegId, KnownMethodCallableArray>::new();
            let mut known_method_callable_locals =
                BTreeMap::<LocalId, KnownMethodCallableArray>::new();
            let mut known_preg_callback_arrays = BTreeMap::<RegId, KnownPregCallbackArray>::new();
            let mut consumed_method_callable_receivers =
                BTreeMap::<RegId, ConsumedMethodCallableArray>::new();
            let mut exact_object_registers = BTreeSet::<RegId>::new();
            let mut exact_object_locals = BTreeSet::<LocalId>::new();
            let mut native_globals_registers = BTreeSet::<RegId>::new();
            if let Some((class, false)) = method_class {
                // Every instance entry receives a receiver from its declaring
                // class family. Property storage is prefix-stable across that
                // family, while virtual method resolution still requires the
                // separate exactness fact below.
                known_object_locals.insert(LocalId::new(0), class);
                if unit
                    .classes
                    .get(class as usize)
                    .is_some_and(|class| class.flags.is_final)
                {
                    exact_object_locals.insert(LocalId::new(0));
                }
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
                            && let Some(class) = known_external_object_registers.get(register)
                        {
                            known_external_object_registers.insert(*dst, class.clone());
                        }
                        if let Operand::Register(register) = src
                            && let Some(callable) = known_method_callable_registers.get(register)
                        {
                            known_method_callable_registers.insert(*dst, callable.clone());
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
                        if let Some(class) = known_external_object_locals.get(local) {
                            known_external_object_registers.insert(*dst, class.clone());
                        }
                        if let Some(callable) = known_method_callable_locals.get(local) {
                            known_method_callable_registers.insert(*dst, callable.clone());
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
                            && let Some(class) = known_external_object_registers.get(register)
                        {
                            known_external_object_locals.insert(*local, class.clone());
                        } else {
                            known_external_object_locals.remove(local);
                        }
                        if let Operand::Register(register) = src
                            && let Some(callable) = known_method_callable_registers.get(register)
                        {
                            known_method_callable_locals.insert(*local, callable.clone());
                        } else {
                            known_method_callable_locals.remove(local);
                        }
                        if let Operand::Register(register) = src
                            && exact_object_registers.contains(register)
                        {
                            exact_object_locals.insert(*local);
                        } else {
                            exact_object_locals.remove(local);
                        }
                    }
                    InstructionKind::NewArray { dst } => {
                        let mut callable = KnownMethodCallableArray {
                            root_register: Some(*dst),
                            ..KnownMethodCallableArray::default()
                        };
                        callable.construction_ids.insert(instruction.id);
                        known_method_callable_registers.insert(*dst, callable);
                        let mut callbacks = KnownPregCallbackArray {
                            root_register: Some(*dst),
                            ..KnownPregCallbackArray::default()
                        };
                        callbacks.construction_ids.insert(instruction.id);
                        known_preg_callback_arrays.insert(*dst, callbacks);
                    }
                    InstructionKind::ArrayInsert {
                        array,
                        key,
                        value,
                        by_ref_local,
                    } => {
                        let candidate = known_method_callable_registers.get(array).cloned();
                        let next = candidate.and_then(|mut candidate| {
                            if key.is_some() || by_ref_local.is_some() {
                                return None;
                            }
                            match candidate.length {
                                0 => {
                                    let exact_instance = match value {
                                        Operand::Register(register)
                                            if exact_object_registers.contains(register) =>
                                        {
                                            known_external_object_registers
                                                .get(register)
                                                .cloned()
                                                .or_else(|| {
                                                    known_object_registers
                                                        .get(register)
                                                        .and_then(|class| {
                                                            unit.classes.get(*class as usize)
                                                        })
                                                        .map(|class| class.name.clone())
                                                })
                                                .map(|class_name| {
                                                    KnownMethodCallableTarget::Instance {
                                                        receiver: lower_operand(unit, *value),
                                                        class_name,
                                                    }
                                                })
                                        }
                                        Operand::Local(local)
                                            if exact_object_locals.contains(local) =>
                                        {
                                            known_external_object_locals
                                                .get(local)
                                                .cloned()
                                                .or_else(|| {
                                                    known_object_locals
                                                        .get(local)
                                                        .and_then(|class| {
                                                            unit.classes.get(*class as usize)
                                                        })
                                                        .map(|class| class.name.clone())
                                                })
                                                .map(|class_name| {
                                                    KnownMethodCallableTarget::Instance {
                                                        receiver: lower_operand(unit, *value),
                                                        class_name,
                                                    }
                                                })
                                        }
                                        Operand::Register(_)
                                        | Operand::Local(_)
                                        | Operand::Constant(_) => None,
                                    };
                                    let static_class = match value {
                                        Operand::Register(register) => {
                                            known_register_strings.get(register).cloned()
                                        }
                                        Operand::Local(local) => {
                                            known_local_strings.get(local).cloned()
                                        }
                                        Operand::Constant(_) => known_string_operand(
                                            unit,
                                            *value,
                                            &known_register_strings,
                                        ),
                                    }
                                    .map(|class_name| KnownMethodCallableTarget::Static {
                                        class_name,
                                    });
                                    candidate.target = exact_instance.or(static_class);
                                    candidate.target.as_ref()?;
                                }
                                1 => {
                                    candidate.method = match value {
                                        Operand::Register(register) => {
                                            known_register_strings.get(register).cloned()
                                        }
                                        Operand::Local(local) => {
                                            known_local_strings.get(local).cloned()
                                        }
                                        Operand::Constant(_) => known_string_operand(
                                            unit,
                                            *value,
                                            &known_register_strings,
                                        ),
                                    };
                                    candidate.method.as_ref()?;
                                }
                                _ => return None,
                            }
                            candidate.construction_ids.insert(instruction.id);
                            candidate.last_inserted = Some(*value);
                            candidate.length = candidate.length.saturating_add(1);
                            Some(candidate)
                        });
                        if let Some(candidate) = next {
                            known_method_callable_registers.insert(*array, candidate);
                        } else {
                            known_method_callable_registers.remove(array);
                        }
                        let callback_map = known_preg_callback_arrays.get(array).cloned().and_then(
                            |mut callback_map| {
                                let key = key.as_ref()?;
                                if by_ref_local.is_some()
                                    || known_string_operand(unit, *key, &known_register_strings)
                                        .is_none()
                                {
                                    return None;
                                }
                                callback_map.entries.push((*key, *value));
                                callback_map.construction_ids.insert(instruction.id);
                                Some(callback_map)
                            },
                        );
                        if let Some(callback_map) = callback_map {
                            known_preg_callback_arrays.insert(*array, callback_map);
                        } else {
                            known_preg_callback_arrays.remove(array);
                        }
                    }
                    InstructionKind::ArraySpread { array, .. } => {
                        known_method_callable_registers.remove(array);
                        known_preg_callback_arrays.remove(array);
                    }
                    InstructionKind::Discard { src } => {
                        for callable in known_method_callable_registers.values_mut() {
                            if callable.last_inserted == Some(*src) {
                                if callable.length == 1
                                    && matches!(
                                        callable.target,
                                        Some(KnownMethodCallableTarget::Instance { .. })
                                    )
                                {
                                    callable.construction_ids.insert(instruction.id);
                                    callable.receiver_owner_captured = true;
                                }
                                callable.last_inserted = None;
                            }
                        }
                    }
                    InstructionKind::ResolveCallable {
                        dst,
                        callable: CallableKind::FunctionName { name },
                    } => {
                        known_callables.insert(*dst, name.clone());
                    }
                    InstructionKind::CallFunction { name, args, .. }
                    | InstructionKind::BindReferenceFromCall { name, args, .. } => {
                        let local_target = find_function(unit, name)
                            .or_else(|| {
                                unit.functions
                                    .iter()
                                    .position(|function| function.name.eq_ignore_ascii_case(name))
                                    .and_then(|index| u32::try_from(index).ok())
                                    .map(FunctionId::new)
                            })
                            .and_then(|function| unit.functions.get(function.index()));
                        let external_target = local_target.is_none().then(|| {
                            published_external_function_signature(
                                published_external_signatures,
                                name,
                            )
                        });
                        let builtin_parameters = (local_target.is_none()
                            && external_target.flatten().is_none())
                        .then(|| internal_builtin_binding_parameters(name))
                        .flatten();
                        let target_params = local_target
                            .map(|target| target.params.as_slice())
                            .or_else(|| {
                                external_target
                                    .flatten()
                                    .map(|target| target.native_params.as_slice())
                            })
                            .or(builtin_parameters.as_deref());
                        let (prepared, bindings) = target_params.map_or_else(
                            || (args.clone(), Vec::new()),
                            |parameters| {
                                prepare_reference_call_arguments(
                                    unit,
                                    ir_function,
                                    instruction,
                                    args,
                                    parameters,
                                    runtime_metadata.tier == NativeCompilerTier::Optimizing,
                                    &mut region_local_count,
                                    &mut region_locals,
                                    &known_object_registers,
                                    published_external_signatures,
                                    &source_register_use_counts,
                                    &mut instructions,
                                )
                            },
                        );
                        for kind in bindings {
                            instructions.push(RegionInstruction {
                                id: instruction.id,
                                span: instruction.span,
                                continuation_id: next_continuation,
                                live_locals: Vec::new(),
                                transition_live_registers: None,
                                optimizer_transition_entry: false,
                                source_kind: instruction.kind.clone(),
                                native_global_name: None,
                                kind,
                            });
                            next_continuation = next_continuation.saturating_add(1);
                        }
                        prepared_call_args = Some(prepared);
                        if let InstructionKind::CallFunction {
                            dst, name, args, ..
                        } = &instruction.kind
                            && let Some(closure) = returned_closure(unit, name, args)
                        {
                            known_closure_registers.insert(*dst, closure);
                        }
                    }
                    InstructionKind::CallCallable { callee, args, .. } => {
                        let target = known_string_operand(unit, *callee, &known_register_strings)
                            .and_then(|name| find_function(unit, &name))
                            .and_then(|function| unit.functions.get(function.index()));
                        let mut prepared = args.clone();
                        for (index, argument) in prepared.iter_mut().enumerate() {
                            if !target
                                .and_then(|target| {
                                    prepared_parameter_for_source(
                                        target.params.as_slice(),
                                        args,
                                        index,
                                    )
                                })
                                .is_some_and(|parameter| parameter.by_ref)
                            {
                                continue;
                            }
                            if let Some(local) = argument.by_ref_local
                                && !local_is_entry_reference(ir_function, local)
                            {
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    transition_live_registers: None,
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
                    InstructionKind::CallMethod {
                        object,
                        method,
                        args,
                        ..
                    }
                    | InstructionKind::BindReferenceFromMethodCall {
                        object,
                        method,
                        args,
                        ..
                    } => {
                        let specialization = method_specializations.iter().find(|specialization| {
                            specialization.instruction_id == instruction.id.raw()
                        });
                        let specialized_local_target = specialization.and_then(|specialization| {
                            match &specialization.target {
                                crate::JitMethodSpecializationTarget::Local(function) => {
                                    Some(*function)
                                }
                                crate::JitMethodSpecializationTarget::Linked(_) => None,
                            }
                        });
                        let local_target = stable_local_method_function(
                            unit,
                            *object,
                            method,
                            &known_object_registers,
                            &exact_object_registers,
                        )
                        .or(specialized_local_target);
                        let target_params = local_target
                            .and_then(|function| unit.functions.get(function.index()))
                            .map(|function| function.params.as_slice())
                            .or_else(|| {
                                specialization.and_then(|specialization| {
                                    match &specialization.target {
                                        crate::JitMethodSpecializationTarget::Linked(signature) => {
                                            Some(signature.native_params.as_slice())
                                        }
                                        crate::JitMethodSpecializationTarget::Local(_) => None,
                                    }
                                })
                            })
                            .or_else(|| {
                                matches!(
                                    object,
                                    Operand::Register(register)
                                        if exact_object_registers.contains(register)
                                )
                                .then(|| {
                                    known_external_object_class(
                                        *object,
                                        &known_external_object_registers,
                                        &known_external_object_locals,
                                    )
                                    .and_then(|class| {
                                        published_external_method_signature(
                                            published_external_signatures,
                                            class,
                                            method,
                                        )
                                    })
                                    .filter(|signature| {
                                        usize::try_from(signature.native_arity).ok()
                                            == Some(signature.native_params.len().saturating_add(1))
                                    })
                                    .map(|signature| signature.native_params.as_slice())
                                })
                                .flatten()
                            });
                        if let Some(parameters) = target_params {
                            let (prepared, bindings) = prepare_reference_call_arguments(
                                unit,
                                ir_function,
                                instruction,
                                args,
                                parameters,
                                runtime_metadata.tier == NativeCompilerTier::Optimizing,
                                &mut region_local_count,
                                &mut region_locals,
                                &known_object_registers,
                                published_external_signatures,
                                &source_register_use_counts,
                                &mut instructions,
                            );
                            for kind in bindings {
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    transition_live_registers: None,
                                    optimizer_transition_entry: false,
                                    source_kind: instruction.kind.clone(),
                                    native_global_name: None,
                                    kind,
                                });
                                next_continuation = next_continuation.saturating_add(1);
                            }
                            prepared_call_args = Some(prepared);
                        }
                    }
                    InstructionKind::CallStaticMethod {
                        class_name,
                        method,
                        args,
                        ..
                    } if !(class_name.eq_ignore_ascii_case("Closure")
                        && method.eq_ignore_ascii_case("bind")) =>
                    {
                        let local_target = find_direct_static_method(unit, class_name, method)
                            .and_then(|function| unit.functions.get(function.index()));
                        let external_target = local_target.is_none().then(|| {
                            published_external_method_signature(
                                published_external_signatures,
                                class_name,
                                method,
                            )
                            .filter(|signature| {
                                usize::try_from(signature.native_arity).ok()
                                    == Some(signature.native_params.len())
                            })
                        });
                        let target_params = local_target
                            .map(|target| target.params.as_slice())
                            .or_else(|| {
                                external_target
                                    .flatten()
                                    .map(|target| target.native_params.as_slice())
                            });
                        if let Some(parameters) = target_params {
                            let (prepared, bindings) = prepare_reference_call_arguments(
                                unit,
                                ir_function,
                                instruction,
                                args,
                                parameters,
                                runtime_metadata.tier == NativeCompilerTier::Optimizing,
                                &mut region_local_count,
                                &mut region_locals,
                                &known_object_registers,
                                published_external_signatures,
                                &source_register_use_counts,
                                &mut instructions,
                            );
                            for kind in bindings {
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    transition_live_registers: None,
                                    optimizer_transition_entry: false,
                                    source_kind: instruction.kind.clone(),
                                    native_global_name: None,
                                    kind,
                                });
                                next_continuation = next_continuation.saturating_add(1);
                            }
                            prepared_call_args = Some(prepared);
                        }
                    }
                    InstructionKind::MakeClosure {
                        dst,
                        function: closure_function,
                        captures,
                    } => {
                        let bound_this_local =
                            native_closure_bound_this_local(unit, function, *closure_function);
                        let bound_object = bound_this_local.map(RegionOperand::Local);
                        if !closure_requires_implicit_this(unit, *closure_function)
                            || bound_object.is_some()
                        {
                            known_closure_registers.insert(
                                *dst,
                                KnownClosure {
                                    function: *closure_function,
                                    capture_count: captures.len(),
                                    bound_object,
                                    requires_runtime_context: method_class.is_some()
                                        || ir_function.flags.is_closure,
                                },
                            );
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
                        dst,
                        class_name,
                        args,
                        ..
                    } => {
                        if let Some((class_index, class)) = find_class(unit, class_name) {
                            if class.constructor.is_some() {
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    transition_live_registers: None,
                                    optimizer_transition_entry: false,
                                    source_kind: instruction.kind.clone(),
                                    native_global_name: None,
                                    kind: RegionInstructionKind::NewObject {
                                        dst: *dst,
                                        class: class_index,
                                        prepared: class_has_publication_stable_layout(
                                            unit,
                                            class_index,
                                            published_external_signatures,
                                        ),
                                        linked_class: None,
                                    },
                                });
                                next_continuation = next_continuation.saturating_add(1);
                            } else if local_class_external_parent(unit, class_index).is_some() {
                                let signature = published_external_parent_constructor_signature(
                                    unit,
                                    class_index,
                                    external_function_signatures,
                                )
                                .and_then(|signature| {
                                    direct_external_constructor_signature(unit, signature, args)
                                });
                                let direct_constructor_result =
                                    signature.is_some_and(|signature| signature.native_arity != 0);
                                instructions.push(RegionInstruction {
                                    id: instruction.id,
                                    span: instruction.span,
                                    continuation_id: next_continuation,
                                    live_locals: Vec::new(),
                                    transition_live_registers: None,
                                    optimizer_transition_entry: false,
                                    source_kind: instruction.kind.clone(),
                                    native_global_name: None,
                                    kind: if runtime_metadata.tier == NativeCompilerTier::Optimizing
                                        && signature.is_some()
                                    {
                                        RegionInstructionKind::NewObject {
                                            dst: *dst,
                                            class: class_index,
                                            prepared: class_has_publication_stable_layout(
                                                unit,
                                                class_index,
                                                published_external_signatures,
                                            ),
                                            linked_class: None,
                                        }
                                    } else {
                                        // Preserve the allocation continuation
                                        // before the external parent becomes
                                        // visible. The original instruction
                                        // remains the constructor continuation.
                                        RegionInstructionKind::Discard {
                                            src: RegionOperand::I64(0),
                                        }
                                    },
                                });
                                next_continuation = next_continuation.saturating_add(1);
                                if !(runtime_metadata.tier == NativeCompilerTier::Optimizing
                                    && direct_constructor_result)
                                {
                                    region_register_count = region_register_count.saturating_add(1);
                                }
                                if let Some(signature) = signature {
                                    let (prepared, bindings) = if signature.native_arity == 0 {
                                        (args.clone(), Vec::new())
                                    } else {
                                        prepare_reference_call_arguments(
                                            unit,
                                            ir_function,
                                            instruction,
                                            args,
                                            &signature.native_params,
                                            runtime_metadata.tier == NativeCompilerTier::Optimizing,
                                            &mut region_local_count,
                                            &mut region_locals,
                                            &known_object_registers,
                                            published_external_signatures,
                                            &source_register_use_counts,
                                            &mut instructions,
                                        )
                                    };
                                    for kind in bindings {
                                        instructions.push(RegionInstruction {
                                            id: instruction.id,
                                            span: instruction.span,
                                            continuation_id: next_continuation,
                                            live_locals: Vec::new(),
                                            transition_live_registers: None,
                                            optimizer_transition_entry: false,
                                            source_kind: instruction.kind.clone(),
                                            native_global_name: None,
                                            kind,
                                        });
                                        next_continuation = next_continuation.saturating_add(1);
                                    }
                                    prepared_call_args = Some(prepared);
                                }
                            }
                        } else {
                            let signature = published_external_method_signature(
                                external_function_signatures,
                                class_name,
                                "__construct",
                            )
                            .and_then(|signature| {
                                direct_external_constructor_signature(unit, signature, args)
                            });
                            let direct_constructor_result = signature.is_some_and(|signature| {
                                !(signature.native_arity == 0
                                    && signature.native_params.is_empty()
                                    && args.is_empty())
                            });
                            instructions.push(RegionInstruction {
                                id: instruction.id,
                                span: instruction.span,
                                continuation_id: next_continuation,
                                live_locals: Vec::new(),
                                transition_live_registers: None,
                                optimizer_transition_entry: false,
                                source_kind: instruction.kind.clone(),
                                native_global_name: None,
                                kind: if runtime_metadata.tier == NativeCompilerTier::Optimizing
                                    && let Some(signature) = signature
                                {
                                    RegionInstructionKind::NewObject {
                                        dst: *dst,
                                        class: 0,
                                        prepared: true,
                                        linked_class: Some(signature.link_index),
                                    }
                                } else {
                                    // Reserve the optimizer's allocation
                                    // continuation in baseline code without
                                    // allocating the object twice.  Discarding
                                    // an immediate is a baseline-native
                                    // no-op, remains a resumable instruction,
                                    // and carries no runtime owner.
                                    RegionInstructionKind::Discard {
                                        src: RegionOperand::I64(0),
                                    }
                                },
                            });
                            next_continuation = next_continuation.saturating_add(1);
                            // Every external constructor reserves one ignored
                            // native result register.  A linked non-empty
                            // constructor allocates it in the main lowering;
                            // all other visibility/tier combinations reserve
                            // it here so later register identities cannot move.
                            if !(runtime_metadata.tier == NativeCompilerTier::Optimizing
                                && direct_constructor_result)
                            {
                                region_register_count = region_register_count.saturating_add(1);
                            }
                            if let Some(signature) = signature {
                                let (prepared, bindings) = if signature.native_arity == 0 {
                                    (args.clone(), Vec::new())
                                } else {
                                    prepare_reference_call_arguments(
                                        unit,
                                        ir_function,
                                        instruction,
                                        args,
                                        &signature.native_params,
                                        runtime_metadata.tier == NativeCompilerTier::Optimizing,
                                        &mut region_local_count,
                                        &mut region_locals,
                                        &known_object_registers,
                                        published_external_signatures,
                                        &source_register_use_counts,
                                        &mut instructions,
                                    )
                                };
                                for kind in bindings {
                                    instructions.push(RegionInstruction {
                                        id: instruction.id,
                                        span: instruction.span,
                                        continuation_id: next_continuation,
                                        live_locals: Vec::new(),
                                        transition_live_registers: None,
                                        optimizer_transition_entry: false,
                                        source_kind: instruction.kind.clone(),
                                        native_global_name: None,
                                        kind,
                                    });
                                    next_continuation = next_continuation.saturating_add(1);
                                }
                                prepared_call_args = Some(prepared);
                            }
                        }
                        if let Some((class_index, _)) = find_class(unit, class_name) {
                            known_object_registers.insert(*dst, class_index);
                            exact_object_registers.insert(*dst);
                        } else {
                            known_external_object_registers
                                .insert(*dst, class_name.trim_start_matches('\\').to_owned());
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
                        if let Operand::Register(register) = object
                            && exact_object_registers.contains(register)
                            && let Some(class) =
                                known_external_object_registers.get(register).cloned()
                        {
                            known_external_object_registers.insert(*dst, class);
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
                    InstructionKind::Discard {
                        src: Operand::Register(register),
                    } if consumed_method_callable_receivers.contains_key(register) => {
                        let consumed = consumed_method_callable_receivers
                            .remove(register)
                            .expect("checked consumed callable receiver");
                        instructions.retain(|instruction| {
                            !consumed.construction_ids.contains(&instruction.id)
                        });
                        match consumed.receiver_owner {
                            Some(receiver) => RegionInstructionKind::Discard { src: receiver },
                            None => RegionInstructionKind::Nop,
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
                        if runtime_metadata.tier == NativeCompilerTier::Optimizing
                            && {
                                let normalized = name.trim_start_matches('\\');
                                normalized.eq_ignore_ascii_case("array_map")
                                    || normalized.eq_ignore_ascii_case("array_filter")
                                    || normalized.eq_ignore_ascii_case("array_reduce")
                                    || normalized.eq_ignore_ascii_case("array_all")
                                    || normalized.eq_ignore_ascii_case("array_any")
                                    || normalized.eq_ignore_ascii_case("array_find")
                                    || normalized.eq_ignore_ascii_case("array_find_key")
                                    || normalized.eq_ignore_ascii_case("usort")
                                    || normalized.eq_ignore_ascii_case("uasort")
                                    || normalized.eq_ignore_ascii_case("uksort")
                                    || normalized.eq_ignore_ascii_case("array_walk")
                                    || normalized.eq_ignore_ascii_case("array_walk_recursive")
                                    || normalized.eq_ignore_ascii_case("preg_replace_callback")
                                    || normalized
                                        .eq_ignore_ascii_case("preg_replace_callback_array")
                            }
                            && args
                                .iter()
                                .all(|argument| argument.name.is_none() && !argument.unpack) =>
                    {
                        let normalized = name.trim_start_matches('\\');
                        let planned_preg_callback_array =
                            if normalized.eq_ignore_ascii_case("preg_replace_callback_array")
                                && (2..=4).contains(&args.len())
                                && args
                                    .get(3)
                                    .is_none_or(|argument| argument.by_ref_local.is_some())
                            {
                                match args[0].value {
                                    Operand::Register(callback_map_register) => {
                                        known_preg_callback_arrays
                                            .get(&callback_map_register)
                                            .filter(|callback_map| {
                                                callback_map.root_register
                                                    == Some(callback_map_register)
                                            })
                                            .and_then(|callback_map| {
                                                planned_preg_callback_array(
                                                    unit,
                                                    published_external_signatures,
                                                    callback_map,
                                                    RegionOperand::Register(callback_map_register),
                                                    &known_register_strings,
                                                    &known_local_strings,
                                                    &known_callables,
                                                    &known_callable_locals,
                                                    &known_method_callable_registers,
                                                    &known_method_callable_locals,
                                                    &known_closure_registers,
                                                    &known_closure_locals,
                                                )
                                                .map(|entries| {
                                                    let delete_construction =
                                                        entries.iter().all(|entry| {
                                                            matches!(
                                                                &entry.callback,
                                                                RegionArrayCallbackTarget::Stable(
                                                                    callback
                                                                ) if callback.receiver.is_none()
                                                                    && callback.closure.is_none()
                                                            )
                                                        });
                                                    (
                                                        callback_map_register,
                                                        callback_map.construction_ids.clone(),
                                                        delete_construction,
                                                        RegionPregCallbackArrayCall {
                                                            result: *dst,
                                                            entries,
                                                            subject: lower_operand(
                                                                unit,
                                                                args[1].value,
                                                            ),
                                                            limit: args.get(2).map_or(
                                                                RegionOperand::I64(-1),
                                                                |argument| {
                                                                    lower_operand(
                                                                        unit,
                                                                        argument.value,
                                                                    )
                                                                },
                                                            ),
                                                            count_local: args.get(3).and_then(
                                                                |argument| argument.by_ref_local,
                                                            ),
                                                            caller_strict_types: unit.strict_types,
                                                        },
                                                    )
                                                })
                                            })
                                    }
                                    Operand::Local(_) | Operand::Constant(_) => None,
                                }
                            } else {
                                None
                            };
                        let planned = if normalized.eq_ignore_ascii_case("preg_replace_callback")
                            && (3..=6).contains(&args.len())
                            && args
                                .get(4)
                                .is_none_or(|argument| argument.by_ref_local.is_some())
                        {
                            stable_array_callback_operand(
                                unit,
                                published_external_signatures,
                                args[1].value,
                                1,
                                true,
                                &known_register_strings,
                                &known_local_strings,
                                &known_callables,
                                &known_callable_locals,
                                &known_method_callable_registers,
                                &known_method_callable_locals,
                                &known_closure_registers,
                                &known_closure_locals,
                            )
                            .filter(|callback| {
                                stable_callback_has_releasable_scalar_return(unit, callback)
                            })
                            .map(|callback| RegionArrayCallbackCall {
                                result: *dst,
                                operation: RegionArrayCallbackOperation::PregReplace,
                                callback: RegionArrayCallbackTarget::Stable(callback),
                                arrays: vec![
                                    lower_operand(unit, args[0].value),
                                    lower_operand(unit, args[2].value),
                                    args.get(3).map_or(RegionOperand::I64(-1), |argument| {
                                        lower_operand(unit, argument.value)
                                    }),
                                    args.get(5).map_or(RegionOperand::I64(0), |argument| {
                                        lower_operand(unit, argument.value)
                                    }),
                                ],
                                initial: None,
                                mutable_local: args
                                    .get(4)
                                    .and_then(|argument| argument.by_ref_local),
                                caller_strict_types: unit.strict_types,
                            })
                        } else if normalized.eq_ignore_ascii_case("array_map") && args.len() >= 2 {
                            stable_array_callback_operand(
                                unit,
                                published_external_signatures,
                                args[0].value,
                                args.len() - 1,
                                true,
                                &known_register_strings,
                                &known_local_strings,
                                &known_callables,
                                &known_callable_locals,
                                &known_method_callable_registers,
                                &known_method_callable_locals,
                                &known_closure_registers,
                                &known_closure_locals,
                            )
                            .map(|callback| RegionArrayCallbackCall {
                                result: *dst,
                                operation: RegionArrayCallbackOperation::Map,
                                callback: RegionArrayCallbackTarget::Stable(callback),
                                arrays: args[1..]
                                    .iter()
                                    .map(|argument| lower_operand(unit, argument.value))
                                    .collect(),
                                initial: None,
                                mutable_local: None,
                                caller_strict_types: unit.strict_types,
                            })
                        } else if normalized.eq_ignore_ascii_case("array_filter")
                            && (2..=3).contains(&args.len())
                        {
                            let mode = if args.len() == 3 {
                                constant_integer_operand(unit, args[2].value)
                            } else {
                                Some(0)
                            };
                            let operation = match mode {
                                Some(0) => Some(RegionArrayCallbackOperation::FilterValue),
                                Some(1) => Some(RegionArrayCallbackOperation::FilterValueAndKey),
                                Some(2) => Some(RegionArrayCallbackOperation::FilterKey),
                                _ => None,
                            };
                            operation.and_then(|operation| {
                                let callback_argument_count = usize::from(matches!(
                                    operation,
                                    RegionArrayCallbackOperation::FilterValueAndKey
                                )) + 1;
                                stable_array_callback_operand(
                                    unit,
                                    published_external_signatures,
                                    args[1].value,
                                    callback_argument_count,
                                    true,
                                    &known_register_strings,
                                    &known_local_strings,
                                    &known_callables,
                                    &known_callable_locals,
                                    &known_method_callable_registers,
                                    &known_method_callable_locals,
                                    &known_closure_registers,
                                    &known_closure_locals,
                                )
                                .map(|callback| {
                                    RegionArrayCallbackCall {
                                        result: *dst,
                                        operation,
                                        callback: RegionArrayCallbackTarget::Stable(callback),
                                        arrays: vec![lower_operand(unit, args[0].value)],
                                        initial: None,
                                        mutable_local: None,
                                        caller_strict_types: unit.strict_types,
                                    }
                                })
                            })
                        } else if normalized.eq_ignore_ascii_case("array_reduce")
                            && (2..=3).contains(&args.len())
                        {
                            stable_array_callback_operand(
                                unit,
                                published_external_signatures,
                                args[1].value,
                                2,
                                true,
                                &known_register_strings,
                                &known_local_strings,
                                &known_callables,
                                &known_callable_locals,
                                &known_method_callable_registers,
                                &known_method_callable_locals,
                                &known_closure_registers,
                                &known_closure_locals,
                            )
                            .map(|callback| RegionArrayCallbackCall {
                                result: *dst,
                                operation: RegionArrayCallbackOperation::Reduce,
                                callback: RegionArrayCallbackTarget::Stable(callback),
                                arrays: vec![lower_operand(unit, args[0].value)],
                                initial: args
                                    .get(2)
                                    .map(|argument| lower_operand(unit, argument.value)),
                                mutable_local: None,
                                caller_strict_types: unit.strict_types,
                            })
                        } else if !ir_function.flags.is_top_level
                            && (2..=3).contains(&args.len())
                            && (normalized.eq_ignore_ascii_case("array_walk")
                                || normalized.eq_ignore_ascii_case("array_walk_recursive"))
                            && args[0].by_ref_local.is_some()
                        {
                            stable_array_callback_operand(
                                unit,
                                published_external_signatures,
                                args[1].value,
                                args.len(),
                                true,
                                &known_register_strings,
                                &known_local_strings,
                                &known_callables,
                                &known_callable_locals,
                                &known_method_callable_registers,
                                &known_method_callable_locals,
                                &known_closure_registers,
                                &known_closure_locals,
                            )
                            .filter(|callback| {
                                callback.function.is_some()
                                    && callback.receiver.is_none()
                                    && callback.closure.is_none()
                                    && stable_callback_has_releasable_scalar_return(unit, callback)
                            })
                            .map(|callback| RegionArrayCallbackCall {
                                result: *dst,
                                operation: if normalized
                                    .eq_ignore_ascii_case("array_walk_recursive")
                                {
                                    RegionArrayCallbackOperation::WalkRecursive
                                } else {
                                    RegionArrayCallbackOperation::Walk
                                },
                                callback: RegionArrayCallbackTarget::Stable(callback),
                                arrays: Vec::new(),
                                initial: args
                                    .get(2)
                                    .map(|argument| lower_operand(unit, argument.value)),
                                mutable_local: args[0].by_ref_local,
                                caller_strict_types: unit.strict_types,
                            })
                        } else if !ir_function.flags.is_top_level
                            && args.len() == 2
                            && (normalized.eq_ignore_ascii_case("usort")
                                || normalized.eq_ignore_ascii_case("uasort")
                                || normalized.eq_ignore_ascii_case("uksort"))
                            && args[0].by_ref_local.is_some()
                        {
                            let operation = if normalized.eq_ignore_ascii_case("usort") {
                                RegionArrayCallbackOperation::Usort
                            } else if normalized.eq_ignore_ascii_case("uasort") {
                                RegionArrayCallbackOperation::Uasort
                            } else {
                                RegionArrayCallbackOperation::Uksort
                            };
                            stable_array_callback_operand(
                                unit,
                                published_external_signatures,
                                args[1].value,
                                2,
                                true,
                                &known_register_strings,
                                &known_local_strings,
                                &known_callables,
                                &known_callable_locals,
                                &known_method_callable_registers,
                                &known_method_callable_locals,
                                &known_closure_registers,
                                &known_closure_locals,
                            )
                            .filter(|callback| {
                                callback.returns_int
                                    && callback.function.is_some()
                                    && callback.receiver.is_none()
                                    && callback.closure.is_none()
                            })
                            .map(|callback| RegionArrayCallbackCall {
                                result: *dst,
                                operation,
                                callback: RegionArrayCallbackTarget::Stable(callback),
                                arrays: Vec::new(),
                                initial: None,
                                mutable_local: args[0].by_ref_local,
                                caller_strict_types: unit.strict_types,
                            })
                        } else if args.len() == 2 {
                            let operation = if normalized.eq_ignore_ascii_case("array_all") {
                                Some(RegionArrayCallbackOperation::All)
                            } else if normalized.eq_ignore_ascii_case("array_any") {
                                Some(RegionArrayCallbackOperation::Any)
                            } else if normalized.eq_ignore_ascii_case("array_find") {
                                Some(RegionArrayCallbackOperation::Find)
                            } else if normalized.eq_ignore_ascii_case("array_find_key") {
                                Some(RegionArrayCallbackOperation::FindKey)
                            } else {
                                None
                            };
                            operation.and_then(|operation| {
                                stable_array_callback_operand(
                                    unit,
                                    published_external_signatures,
                                    args[1].value,
                                    2,
                                    true,
                                    &known_register_strings,
                                    &known_local_strings,
                                    &known_callables,
                                    &known_callable_locals,
                                    &known_method_callable_registers,
                                    &known_method_callable_locals,
                                    &known_closure_registers,
                                    &known_closure_locals,
                                )
                                .map(|callback| {
                                    RegionArrayCallbackCall {
                                        result: *dst,
                                        operation,
                                        callback: RegionArrayCallbackTarget::Stable(callback),
                                        arrays: vec![lower_operand(unit, args[0].value)],
                                        initial: None,
                                        mutable_local: None,
                                        caller_strict_types: unit.strict_types,
                                    }
                                })
                            })
                        } else {
                            None
                        };
                        let planned = planned.or_else(|| {
                            let runtime_callback =
                                if normalized.eq_ignore_ascii_case("preg_replace_callback")
                                    && (3..=6).contains(&args.len())
                                    && args
                                        .get(4)
                                        .is_none_or(|argument| argument.by_ref_local.is_some())
                                {
                                    Some(RegionArrayCallbackCall {
                                        result: *dst,
                                        operation: RegionArrayCallbackOperation::PregReplace,
                                        callback: RegionArrayCallbackTarget::Runtime(
                                            lower_operand(unit, args[1].value),
                                        ),
                                        arrays: vec![
                                            lower_operand(unit, args[0].value),
                                            lower_operand(unit, args[2].value),
                                            args.get(3)
                                                .map_or(RegionOperand::I64(-1), |argument| {
                                                    lower_operand(unit, argument.value)
                                                }),
                                            args.get(5).map_or(RegionOperand::I64(0), |argument| {
                                                lower_operand(unit, argument.value)
                                            }),
                                        ],
                                        initial: None,
                                        mutable_local: args
                                            .get(4)
                                            .and_then(|argument| argument.by_ref_local),
                                        caller_strict_types: unit.strict_types,
                                    })
                                } else if normalized.eq_ignore_ascii_case("array_map")
                                    && args.len() >= 2
                                    && !constant_null_operand(unit, args[0].value)
                                {
                                    Some(RegionArrayCallbackCall {
                                        result: *dst,
                                        operation: RegionArrayCallbackOperation::Map,
                                        callback: RegionArrayCallbackTarget::Runtime(
                                            lower_operand(unit, args[0].value),
                                        ),
                                        arrays: args[1..]
                                            .iter()
                                            .map(|argument| lower_operand(unit, argument.value))
                                            .collect(),
                                        initial: None,
                                        mutable_local: None,
                                        caller_strict_types: unit.strict_types,
                                    })
                                } else if normalized.eq_ignore_ascii_case("array_filter")
                                    && (2..=3).contains(&args.len())
                                    && !constant_null_operand(unit, args[1].value)
                                {
                                    let mode = if args.len() == 3 {
                                        constant_integer_operand(unit, args[2].value)
                                    } else {
                                        Some(0)
                                    };
                                    let operation = match mode {
                                        Some(0) => Some(RegionArrayCallbackOperation::FilterValue),
                                        Some(1) => {
                                            Some(RegionArrayCallbackOperation::FilterValueAndKey)
                                        }
                                        Some(2) => Some(RegionArrayCallbackOperation::FilterKey),
                                        _ => None,
                                    }?;
                                    Some(RegionArrayCallbackCall {
                                        result: *dst,
                                        operation,
                                        callback: RegionArrayCallbackTarget::Runtime(
                                            lower_operand(unit, args[1].value),
                                        ),
                                        arrays: vec![lower_operand(unit, args[0].value)],
                                        initial: None,
                                        mutable_local: None,
                                        caller_strict_types: unit.strict_types,
                                    })
                                } else if normalized.eq_ignore_ascii_case("array_reduce")
                                    && (2..=3).contains(&args.len())
                                {
                                    Some(RegionArrayCallbackCall {
                                        result: *dst,
                                        operation: RegionArrayCallbackOperation::Reduce,
                                        callback: RegionArrayCallbackTarget::Runtime(
                                            lower_operand(unit, args[1].value),
                                        ),
                                        arrays: vec![lower_operand(unit, args[0].value)],
                                        initial: args
                                            .get(2)
                                            .map(|argument| lower_operand(unit, argument.value)),
                                        mutable_local: None,
                                        caller_strict_types: unit.strict_types,
                                    })
                                } else if !ir_function.flags.is_top_level
                                    && args.len() == 2
                                    && (normalized.eq_ignore_ascii_case("usort")
                                        || normalized.eq_ignore_ascii_case("uasort")
                                        || normalized.eq_ignore_ascii_case("uksort"))
                                    && args[0].by_ref_local.is_some()
                                {
                                    let operation = if normalized.eq_ignore_ascii_case("usort") {
                                        RegionArrayCallbackOperation::Usort
                                    } else if normalized.eq_ignore_ascii_case("uasort") {
                                        RegionArrayCallbackOperation::Uasort
                                    } else {
                                        RegionArrayCallbackOperation::Uksort
                                    };
                                    Some(RegionArrayCallbackCall {
                                        result: *dst,
                                        operation,
                                        callback: RegionArrayCallbackTarget::Runtime(
                                            lower_operand(unit, args[1].value),
                                        ),
                                        arrays: Vec::new(),
                                        initial: None,
                                        mutable_local: args[0].by_ref_local,
                                        caller_strict_types: unit.strict_types,
                                    })
                                } else if !ir_function.flags.is_top_level
                                    && (2..=3).contains(&args.len())
                                    && (normalized.eq_ignore_ascii_case("array_walk")
                                        || normalized.eq_ignore_ascii_case("array_walk_recursive"))
                                    && args[0].by_ref_local.is_some()
                                {
                                    Some(RegionArrayCallbackCall {
                                        result: *dst,
                                        operation: if normalized
                                            .eq_ignore_ascii_case("array_walk_recursive")
                                        {
                                            RegionArrayCallbackOperation::WalkRecursive
                                        } else {
                                            RegionArrayCallbackOperation::Walk
                                        },
                                        callback: RegionArrayCallbackTarget::Runtime(
                                            lower_operand(unit, args[1].value),
                                        ),
                                        arrays: Vec::new(),
                                        initial: args
                                            .get(2)
                                            .map(|argument| lower_operand(unit, argument.value)),
                                        mutable_local: args[0].by_ref_local,
                                        caller_strict_types: unit.strict_types,
                                    })
                                } else if args.len() == 2 {
                                    let operation = if normalized.eq_ignore_ascii_case("array_all")
                                    {
                                        Some(RegionArrayCallbackOperation::All)
                                    } else if normalized.eq_ignore_ascii_case("array_any") {
                                        Some(RegionArrayCallbackOperation::Any)
                                    } else if normalized.eq_ignore_ascii_case("array_find") {
                                        Some(RegionArrayCallbackOperation::Find)
                                    } else if normalized.eq_ignore_ascii_case("array_find_key") {
                                        Some(RegionArrayCallbackOperation::FindKey)
                                    } else {
                                        None
                                    }?;
                                    Some(RegionArrayCallbackCall {
                                        result: *dst,
                                        operation,
                                        callback: RegionArrayCallbackTarget::Runtime(
                                            lower_operand(unit, args[1].value),
                                        ),
                                        arrays: vec![lower_operand(unit, args[0].value)],
                                        initial: None,
                                        mutable_local: None,
                                        caller_strict_types: unit.strict_types,
                                    })
                                } else {
                                    None
                                };
                            runtime_callback
                        });
                        if let Some((
                            callback_map_register,
                            construction_ids,
                            delete_construction,
                            call,
                        )) = planned_preg_callback_array
                        {
                            if delete_construction {
                                consumed_method_callable_receivers.insert(
                                    callback_map_register,
                                    ConsumedMethodCallableArray {
                                        receiver_owner: None,
                                        construction_ids,
                                    },
                                );
                            }
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            RegionInstructionKind::PregCallbackArray(call)
                        } else if let Some(call) = planned {
                            let callback_operand = if normalized.eq_ignore_ascii_case("array_map") {
                                args.first().map(|argument| argument.value)
                            } else {
                                args.get(1).map(|argument| argument.value)
                            };
                            if let Some(Operand::Register(callable_register)) = callback_operand
                                && let Some(callable) =
                                    known_method_callable_registers.get(&callable_register)
                                && callable.root_register == Some(callable_register)
                                && matches!(
                                    &call.callback,
                                    RegionArrayCallbackTarget::Stable(callback)
                                        if callback
                                            .receiver
                                            .is_none_or(|_| callable.receiver_owner_captured)
                                )
                            {
                                let receiver_owner = match &call.callback {
                                    RegionArrayCallbackTarget::Stable(callback) => {
                                        callback.receiver
                                    }
                                    RegionArrayCallbackTarget::Runtime(_) => None,
                                };
                                consumed_method_callable_receivers.insert(
                                    callable_register,
                                    ConsumedMethodCallableArray {
                                        receiver_owner,
                                        construction_ids: callable.construction_ids.clone(),
                                    },
                                );
                            }
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            RegionInstructionKind::ArrayCallback(call)
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
                            .eq_ignore_ascii_case("call_user_func")
                            && !args.is_empty()
                            && args
                                .iter()
                                .all(|argument| argument.name.is_none() && !argument.unpack) =>
                    {
                        let callee = args[0].value;
                        let callback_args = args[1..].to_vec();
                        let closure = known_closure_operand(
                            callee,
                            &known_closure_registers,
                            &known_closure_locals,
                        )
                        .cloned()
                        .filter(|closure| {
                            direct_closure_runtime_context_is_lowerable(unit, closure)
                                && unit.functions.get(closure.function.index()).is_some_and(
                                    |function| {
                                        !function.flags.is_generator
                                            && !function.returns_by_ref
                                            && closure.capture_count == function.captures.len()
                                            && function
                                                .params
                                                .iter()
                                                .all(|parameter| !parameter.by_ref)
                                    },
                                )
                        });
                        if let Some(closure) = closure {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_closure_call(
                                unit,
                                *dst,
                                callee,
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
                        .filter(|name| {
                            stable_named_callable_is_by_value_only(
                                unit,
                                published_external_signatures,
                                name,
                            )
                        }) {
                            let (call, direct) = lower_stable_named_callable(
                                unit,
                                published_external_signatures,
                                *dst,
                                name,
                                &callback_args,
                            );
                            fast_path_operations =
                                fast_path_operations.saturating_add(u64::from(direct));
                            call
                        } else if runtime_metadata.tier == NativeCompilerTier::Optimizing
                            && let Some((call, receiver, callable)) = known_method_callable_operand(
                                callee,
                                &known_method_callable_registers,
                                &known_method_callable_locals,
                            )
                            .cloned()
                            .and_then(|callable| {
                                lower_stable_method_callable_call(
                                    unit,
                                    published_external_signatures,
                                    *dst,
                                    &callable,
                                    &callback_args,
                                )
                                .map(|(call, receiver)| (call, receiver, callable))
                            })
                        {
                            if let Operand::Register(callable_register) = callee
                                && callable.root_register == Some(callable_register)
                                && receiver.is_none_or(|_| callable.receiver_owner_captured)
                            {
                                consumed_method_callable_receivers.insert(
                                    callable_register,
                                    ConsumedMethodCallableArray {
                                        receiver_owner: receiver,
                                        construction_ids: callable.construction_ids.clone(),
                                    },
                                );
                            }
                            fast_path_operations = fast_path_operations.saturating_add(1);
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
                        let dynamic_native_unpack =
                            runtime_metadata.tier == NativeCompilerTier::Optimizing;
                        let closure = (runtime_metadata.tier == NativeCompilerTier::Optimizing)
                            .then(|| {
                                known_closure_operand(
                                    callee,
                                    &known_closure_registers,
                                    &known_closure_locals,
                                )
                                .cloned()
                                .filter(|closure| {
                                    direct_closure_runtime_context_is_lowerable(unit, closure)
                                        && unit.functions.get(closure.function.index()).is_some_and(
                                            |function| {
                                                !function.flags.is_generator
                                                    && !function.returns_by_ref
                                                    && closure.capture_count
                                                        == function.captures.len()
                                                    && stable_unpack_callback_parameters_are_direct(
                                                        &function.params,
                                                    )
                                            },
                                        )
                                })
                            })
                            .flatten();
                        if let Some(closure) = closure {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_closure_call(
                                unit,
                                *dst,
                                callee,
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
                        .filter(|name| {
                            stable_named_callable_accepts_direct_unpack(
                                unit,
                                published_external_signatures,
                                name,
                            )
                        }) {
                            lower_stable_named_callable(
                                unit,
                                published_external_signatures,
                                *dst,
                                name,
                                &callback_args,
                            )
                            .0
                        } else if runtime_metadata.tier == NativeCompilerTier::Optimizing
                            && let Some((call, receiver, callable)) = known_method_callable_operand(
                                callee,
                                &known_method_callable_registers,
                                &known_method_callable_locals,
                            )
                            .cloned()
                            .and_then(|callable| {
                                lower_stable_method_callable_call(
                                    unit,
                                    published_external_signatures,
                                    *dst,
                                    &callable,
                                    &callback_args,
                                )
                                .map(|(call, receiver)| (call, receiver, callable))
                            })
                        {
                            if let Operand::Register(callable_register) = callee
                                && callable.root_register == Some(callable_register)
                                && receiver.is_none_or(|_| callable.receiver_owner_captured)
                            {
                                consumed_method_callable_receivers.insert(
                                    callable_register,
                                    ConsumedMethodCallableArray {
                                        receiver_owner: receiver,
                                        construction_ids: callable.construction_ids.clone(),
                                    },
                                );
                            }
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            call
                        } else if dynamic_native_unpack {
                            // Keep the unresolved callable and its one
                            // runtime-shaped direct-array segment explicit.
                            // Lowering resolves the already prepared same-unit
                            // target once, validates every array entry before
                            // effects, and invokes its native binding entry.
                            RegionInstructionKind::NativeCall(RegionNativeCall {
                                result: RegionCallResult::Register(*dst),
                                target: RegionCallTarget::Callable { callee },
                                args: callback_args,
                                argument_operand_offset: 1,
                                operands: vec![
                                    Some(lower_operand(unit, callee)),
                                    Some(lower_operand(unit, args[1].value)),
                                ],
                                direct_arity: None,
                                variadic: false,
                                returns_by_reference: false,
                                caller_strict_types: unit.strict_types,
                            })
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
                        let function = find_function(unit, name);
                        let local_target =
                            function.and_then(|function| unit.functions.get(function.index()));
                        let external_target = local_target.is_none().then(|| {
                            published_external_function_signature(
                                published_external_signatures,
                                name,
                            )
                        });
                        let target_params = local_target
                            .map(|target| target.params.as_slice())
                            .or_else(|| {
                                external_target
                                    .flatten()
                                    .map(|target| target.native_params.as_slice())
                            });
                        if local_target.is_some() || external_target.flatten().is_some() {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                        }
                        let variadic = target_params
                            .and_then(|parameters| parameters.last())
                            .is_some_and(|parameter| parameter.variadic);
                        let prepared_operands = local_target
                            .and_then(|target| {
                                prepare_direct_call_operands_for_parameters(
                                    unit,
                                    &target.params,
                                    args,
                                )
                            })
                            .or_else(|| {
                                external_target.flatten().and_then(|signature| {
                                    prepare_direct_external_call_operands(unit, signature, args)
                                })
                            });
                        let operands = prepared_operands
                            .clone()
                            .unwrap_or_else(|| lower_call_operands(unit, args));
                        let direct_function = function.filter(|function| {
                            unit.functions.get(function.index()).is_some_and(|target| {
                                if target.flags.is_generator || prepared_operands.is_none() {
                                    return false;
                                }
                                let instructions =
                                    || target.blocks.iter().flat_map(|block| &block.instructions);
                                if args.iter().any(|argument| argument.unpack)
                                    && instructions().any(|instruction| {
                                        matches!(
                                            &instruction.kind,
                                            InstructionKind::CallFunction { name, .. }
                                                if matches!(
                                                    name.to_ascii_lowercase().as_str(),
                                                    "func_num_args" | "func_get_arg" | "func_get_args"
                                                )
                                        )
                                    })
                                {
                                    return false;
                                }
                                if instructions().any(|instruction| {
                                    matches!(
                                        instruction.kind,
                                        InstructionKind::Yield { .. }
                                            | InstructionKind::YieldFrom { .. }
                                    )
                                }) {
                                    return false;
                                }
                                !instructions().any(|instruction| {
                                    matches!(
                                        &instruction.kind,
                                        InstructionKind::CallStaticMethod {
                                            class_name,
                                            method,
                                            ..
                                        } if class_name.eq_ignore_ascii_case("Fiber")
                                            && method.eq_ignore_ascii_case("suspend")
                                    )
                                })
                            })
                        });
                        let direct_arity = direct_function
                            .and_then(|function| {
                                unit.functions
                                    .get(function.index())
                                    .and_then(|target| u32::try_from(target.params.len()).ok())
                            })
                            .or_else(|| {
                                prepared_operands
                                    .as_ref()
                                    .and(external_target.flatten())
                                    .map(|target| target.native_arity)
                            });
                        let mut native_args = args.to_vec();
                        if let Some(parameters) = target_params {
                            mark_prepared_reference_arguments_for_parameters(
                                &mut native_args,
                                parameters,
                            );
                        }
                        // A PHP namespaced function call falls back to the
                        // global builtin only while no exact namespaced
                        // declaration is visible. Region construction already
                        // resolved that symbol set above. Publish the fixed
                        // builtin identity here so optimizing lowering can use
                        // its compiled ABI instead of redispatching the
                        // namespaced spelling. A later declaration advances
                        // the external-signature epoch and republishes this
                        // caller with the userland target.
                        let published_name =
                            if local_target.is_none() && external_target.flatten().is_none() {
                                resolved_internal_builtin_name(name).unwrap_or(name)
                            } else {
                                name
                            };
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Function {
                                name: published_name.to_owned(),
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
                            returns_by_reference: local_target
                                .is_some_and(|target| target.returns_by_ref)
                                || external_target
                                    .flatten()
                                    .is_some_and(|target| target.returns_by_reference),
                            caller_strict_types: unit.strict_types,
                        })
                    }
                    InstructionKind::CallMethod {
                        dst,
                        object,
                        method,
                        args,
                    } => {
                        let specialization = method_specializations.iter().find(|specialization| {
                            specialization.instruction_id == instruction.id.raw()
                        });
                        let args = prepared_call_args.as_deref().unwrap_or(args);
                        let external_signature = matches!(
                            object,
                            Operand::Register(register)
                                if exact_object_registers.contains(register)
                        )
                        .then(|| {
                            known_external_object_class(
                                *object,
                                &known_external_object_registers,
                                &known_external_object_locals,
                            )
                            .and_then(|class| {
                                published_external_method_signature(
                                    published_external_signatures,
                                    class,
                                    method,
                                )
                            })
                            .filter(|signature| {
                                usize::try_from(signature.native_arity).ok()
                                    == Some(signature.native_params.len().saturating_add(1))
                            })
                        })
                        .flatten();
                        let specialized_external = specialization
                            .and_then(|specialization| match &specialization.target {
                                crate::JitMethodSpecializationTarget::Linked(signature) => {
                                    Some((signature, specialization.receiver_layout_id))
                                }
                                crate::JitMethodSpecializationTarget::Local(_) => None,
                            })
                            .and_then(|(signature, receiver_layout_id)| {
                                lower_specialized_external_method_call(
                                    unit,
                                    RegionCallResult::Register(*dst),
                                    signature,
                                    *object,
                                    method,
                                    receiver_layout_id,
                                    args,
                                )
                            });
                        if let Some(kind) = specialized_external.or_else(|| {
                            external_signature.and_then(|signature| {
                                lower_direct_external_method_call(
                                    unit,
                                    RegionCallResult::Register(*dst),
                                    signature,
                                    Some(*object),
                                    args,
                                )
                            })
                        }) {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            kind
                        } else {
                            stable_local_method_function(
                                unit,
                                *object,
                                method,
                                &known_object_registers,
                                &exact_object_registers,
                            )
                            .map(|function| (function, None))
                            .or_else(|| {
                                specialization.and_then(|specialization| {
                                    match &specialization.target {
                                        crate::JitMethodSpecializationTarget::Local(function) => {
                                            Some((
                                                *function,
                                                Some(specialization.receiver_layout_id),
                                            ))
                                        }
                                        crate::JitMethodSpecializationTarget::Linked(_) => None,
                                    }
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
                                            function: None,
                                            linked_function: None,
                                            receiver_layout_id: None,
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
                                |(function, receiver_layout_id)| {
                                    fast_path_operations = fast_path_operations.saturating_add(1);
                                    if let Some(receiver_layout_id) = receiver_layout_id {
                                        lower_specialized_method_call(
                                            unit,
                                            RegionCallResult::Register(*dst),
                                            function,
                                            *object,
                                            method,
                                            receiver_layout_id,
                                            args,
                                        )
                                    } else {
                                        lower_direct_method_call(
                                            unit, *dst, function, *object, args,
                                        )
                                    }
                                },
                            )
                        }
                    }
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
                    } => {
                        let args = prepared_call_args.as_deref().unwrap_or(args);
                        if let Some(function) = find_direct_static_method(unit, class_name, method)
                        {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_function_call(
                                unit,
                                *dst,
                                unit.functions[function.index()].name.clone(),
                                function,
                                args,
                            )
                        } else if let Some(kind) = published_external_method_signature(
                            published_external_signatures,
                            class_name,
                            method,
                        )
                        .filter(|signature| {
                            usize::try_from(signature.native_arity).ok()
                                == Some(signature.native_params.len())
                        })
                        .and_then(|signature| {
                            lower_direct_external_method_call(
                                unit,
                                RegionCallResult::Register(*dst),
                                signature,
                                None,
                                args,
                            )
                        }) {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            kind
                        } else {
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
                        }
                    }
                    InstructionKind::CallClosure { dst, callee, args } => {
                        let closure = match callee {
                            Operand::Register(register) => {
                                known_closure_registers.get(register).cloned()
                            }
                            _ => None,
                        };
                        if let Some(closure) = closure.filter(|closure| {
                            direct_closure_runtime_context_is_lowerable(unit, closure)
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
                            lower_direct_closure_call(
                                unit,
                                *dst,
                                *callee,
                                closure,
                                args,
                                semantic_context,
                            )
                        } else {
                            let mut operands = vec![Some(lower_operand(unit, *callee))];
                            operands.extend(lower_call_operands(unit, args));
                            RegionInstructionKind::NativeCall(RegionNativeCall {
                                result: RegionCallResult::Register(*dst),
                                target: RegionCallTarget::Closure {
                                    callee: *callee,
                                    function: None,
                                    bound_object_count: 0,
                                    capture_count: 0,
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
                            direct_closure_runtime_context_is_lowerable(unit, closure)
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
                            lower_direct_closure_call(
                                unit,
                                *dst,
                                *callee,
                                closure,
                                args,
                                semantic_context,
                            )
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
                                let (call, direct) = lower_stable_named_callable(
                                    unit,
                                    published_external_signatures,
                                    *dst,
                                    name,
                                    args,
                                );
                                fast_path_operations =
                                    fast_path_operations.saturating_add(u64::from(direct));
                                call
                            } else if runtime_metadata.tier == NativeCompilerTier::Optimizing
                                && let Some((call, receiver, callable)) =
                                    known_method_callable_operand(
                                        *callee,
                                        &known_method_callable_registers,
                                        &known_method_callable_locals,
                                    )
                                    .cloned()
                                    .and_then(|callable| {
                                        lower_stable_method_callable_call(
                                            unit,
                                            published_external_signatures,
                                            *dst,
                                            &callable,
                                            args,
                                        )
                                        .map(|(call, receiver)| (call, receiver, callable))
                                    })
                            {
                                if let Operand::Register(callable_register) = callee
                                    && callable.root_register == Some(*callable_register)
                                    && receiver.is_none_or(|_| callable.receiver_owner_captured)
                                {
                                    consumed_method_callable_receivers.insert(
                                        *callable_register,
                                        ConsumedMethodCallableArray {
                                            receiver_owner: receiver,
                                            construction_ids: callable.construction_ids.clone(),
                                        },
                                    );
                                }
                                fast_path_operations = fast_path_operations.saturating_add(1);
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
                        if let Some(closure) = known_closure.filter(|closure| {
                            direct_closure_runtime_context_is_lowerable(unit, closure)
                        }) {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_closure_call(
                                unit,
                                *dst,
                                *callable,
                                closure,
                                &[argument],
                                semantic_context,
                            )
                        } else if let Some(name) = known_name {
                            let (call, direct) = lower_stable_named_callable(
                                unit,
                                published_external_signatures,
                                *dst,
                                name,
                                &[argument],
                            );
                            fast_path_operations =
                                fast_path_operations.saturating_add(u64::from(direct));
                            call
                        } else if runtime_metadata.tier == NativeCompilerTier::Optimizing
                            && let Some((call, receiver, callable_plan)) =
                                known_method_callable_operand(
                                    *callable,
                                    &known_method_callable_registers,
                                    &known_method_callable_locals,
                                )
                                .cloned()
                                .and_then(|callable_plan| {
                                    lower_stable_method_callable_call(
                                        unit,
                                        published_external_signatures,
                                        *dst,
                                        &callable_plan,
                                        std::slice::from_ref(&argument),
                                    )
                                    .map(|(call, receiver)| (call, receiver, callable_plan))
                                })
                        {
                            if let Operand::Register(callable_register) = callable
                                && callable_plan.root_register == Some(*callable_register)
                                && receiver.is_none_or(|_| callable_plan.receiver_owner_captured)
                            {
                                consumed_method_callable_receivers.insert(
                                    *callable_register,
                                    ConsumedMethodCallableArray {
                                        receiver_owner: receiver,
                                        construction_ids: callable_plan.construction_ids.clone(),
                                    },
                                );
                            }
                            fast_path_operations = fast_path_operations.saturating_add(1);
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
                        let args = prepared_call_args.as_deref().unwrap_or(args);
                        let function = find_function(unit, name);
                        let local_target =
                            function.and_then(|function| unit.functions.get(function.index()));
                        let external_target = local_target.is_none().then(|| {
                            published_external_function_signature(
                                published_external_signatures,
                                name,
                            )
                        });
                        let target_params = local_target
                            .map(|target| target.params.as_slice())
                            .or_else(|| {
                                external_target
                                    .flatten()
                                    .map(|target| target.native_params.as_slice())
                            });
                        let prepared_operands = local_target
                            .and_then(|target| {
                                prepare_direct_call_operands_for_parameters(
                                    unit,
                                    &target.params,
                                    args,
                                )
                            })
                            .or_else(|| {
                                external_target.flatten().and_then(|signature| {
                                    prepare_direct_external_call_operands(unit, signature, args)
                                })
                            });
                        let direct_arity = prepared_operands.as_ref().and_then(|_| {
                            local_target
                                .and_then(|target| u32::try_from(target.params.len()).ok())
                                .or_else(|| {
                                    external_target.flatten().map(|target| target.native_arity)
                                })
                        });
                        let operands =
                            prepared_operands.unwrap_or_else(|| lower_call_operands(unit, args));
                        let mut native_args = args.to_vec();
                        if let Some(parameters) = target_params {
                            mark_prepared_reference_arguments_for_parameters(
                                &mut native_args,
                                parameters,
                            );
                        }
                        let variadic = target_params
                            .and_then(|parameters| parameters.last())
                            .is_some_and(|parameter| parameter.variadic);
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::ReferenceLocal(*target),
                            target: RegionCallTarget::Function {
                                name: name.clone(),
                                function,
                            },
                            args: native_args,
                            argument_operand_offset: 0,
                            operands,
                            direct_arity,
                            variadic,
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
                        let args = prepared_call_args.as_deref().unwrap_or(args);
                        let external = matches!(
                            object,
                            Operand::Register(register)
                                if exact_object_registers.contains(register)
                        )
                        .then(|| {
                            known_external_object_class(
                                *object,
                                &known_external_object_registers,
                                &known_external_object_locals,
                            )
                            .and_then(|class| {
                                published_external_method_signature(
                                    published_external_signatures,
                                    class,
                                    method,
                                )
                            })
                        })
                        .flatten();
                        if let Some(kind) = external.and_then(|signature| {
                            lower_direct_external_method_call(
                                unit,
                                RegionCallResult::ReferenceLocal(*target),
                                signature,
                                Some(*object),
                                args,
                            )
                        }) {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            kind
                        } else if let Some(function) = stable_local_method_function(
                            unit,
                            *object,
                            method,
                            &known_object_registers,
                            &exact_object_registers,
                        )
                        .filter(|function| {
                            unit.functions
                                .get(function.index())
                                .is_some_and(|function| function.returns_by_ref)
                        }) {
                            fast_path_operations = fast_path_operations.saturating_add(1);
                            lower_direct_method_call_result(
                                unit,
                                RegionCallResult::ReferenceLocal(*target),
                                function,
                                *object,
                                args,
                            )
                        } else {
                            let mut operands = vec![Some(lower_operand(unit, *object))];
                            operands.extend(lower_call_operands(unit, args));
                            RegionInstructionKind::NativeCall(RegionNativeCall {
                                result: RegionCallResult::ReferenceLocal(*target),
                                target: RegionCallTarget::Method {
                                    receiver: *object,
                                    method: method.clone(),
                                    function: None,
                                    linked_function: None,
                                    receiver_layout_id: None,
                                },
                                args: args.to_vec(),
                                argument_operand_offset: 1,
                                operands,
                                direct_arity: None,
                                variadic: false,
                                returns_by_reference: true,
                                caller_strict_types: unit.strict_types,
                            })
                        }
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
                            None => {
                                let args = prepared_call_args.as_deref().unwrap_or(args);
                                let inherited = published_external_parent_constructor_signature(
                                    unit,
                                    class_index,
                                    published_external_signatures,
                                );
                                if inherited.is_some_and(|signature| {
                                    signature.native_arity == 0
                                        && signature.native_params.is_empty()
                                        && args.is_empty()
                                }) {
                                    fast_path_operations = fast_path_operations.saturating_add(1);
                                    RegionInstructionKind::Nop
                                } else if let Some(kind) = inherited.and_then(|signature| {
                                    lower_direct_external_method_call(
                                        unit,
                                        RegionCallResult::Register(RegId::new(
                                            region_register_count,
                                        )),
                                        signature,
                                        Some(Operand::Register(*dst)),
                                        args,
                                    )
                                }) {
                                    region_register_count = region_register_count.saturating_add(1);
                                    fast_path_operations = fast_path_operations.saturating_add(1);
                                    kind
                                } else if local_class_external_parent(unit, class_index).is_none()
                                    && args.is_empty()
                                {
                                    RegionInstructionKind::NewObject {
                                        dst: *dst,
                                        class: class_index,
                                        prepared: class_has_publication_stable_layout(
                                            unit,
                                            class_index,
                                            published_external_signatures,
                                        ),
                                        linked_class: None,
                                    }
                                } else {
                                    RegionInstructionKind::NativeCall(RegionNativeCall {
                                        result: RegionCallResult::Register(*dst),
                                        target: RegionCallTarget::Constructor {
                                            display_class_name: display_class_name.clone(),
                                            class_name: class_name.clone(),
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
                        },
                        None => {
                            let args = prepared_call_args.as_deref().unwrap_or(args);
                            let signature = published_external_method_signature(
                                published_external_signatures,
                                class_name,
                                "__construct",
                            );
                            if signature.is_some_and(|signature| {
                                signature.native_arity == 0
                                    && signature.native_params.is_empty()
                                    && args.is_empty()
                            }) {
                                fast_path_operations = fast_path_operations.saturating_add(1);
                                RegionInstructionKind::Nop
                            } else if let Some(kind) = signature.and_then(|signature| {
                                lower_direct_external_method_call(
                                    unit,
                                    RegionCallResult::Register(RegId::new(region_register_count)),
                                    signature,
                                    Some(Operand::Register(*dst)),
                                    args,
                                )
                            }) {
                                region_register_count = region_register_count.saturating_add(1);
                                fast_path_operations = fast_path_operations.saturating_add(1);
                                kind
                            } else {
                                RegionInstructionKind::NativeCall(RegionNativeCall {
                                    result: RegionCallResult::Register(*dst),
                                    target: RegionCallTarget::Constructor {
                                        display_class_name: display_class_name.clone(),
                                        class_name: class_name.clone(),
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
                            function: closure_function,
                            captures,
                            ..
                        } = &instruction.kind
                        else {
                            unreachable!()
                        };
                        RegionInstructionKind::NativeDynamicCode(
                            RegionNativeDynamicCode::MakeClosure {
                                dst: *dst,
                                function: *closure_function,
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
                                bound_this_local: native_closure_bound_this_local(
                                    unit,
                                    function,
                                    *closure_function,
                                ),
                            },
                        )
                    }
                    InstructionKind::MakeClosure {
                        dst,
                        function: closure_function,
                        captures,
                    } => RegionInstructionKind::NativeDynamicCode(
                        RegionNativeDynamicCode::MakeClosure {
                            dst: *dst,
                            function: *closure_function,
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
                            bound_this_local: native_closure_bound_this_local(
                                unit,
                                function,
                                *closure_function,
                            ),
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
                        prepared_class: prepared_object_property_class(
                            unit,
                            *object,
                            &known_object_registers,
                            published_external_signatures,
                        ),
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
                        prepared_class: prepared_object_property_class(
                            unit,
                            *object,
                            &known_object_registers,
                            published_external_signatures,
                        ),
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
                    InstructionKind::AssignDynamicPropertyDim {
                        dst,
                        object,
                        property,
                        dims,
                        value,
                        append,
                    } => {
                        let object = lower_operand(unit, *object);
                        let property = lower_operand(unit, *property);
                        let dimensions = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let value = lower_operand(unit, *value);
                        let mut operands = vec![Some(object), Some(property)];
                        operands.extend(dimensions.iter().copied().map(Some));
                        operands.push(Some(value));
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(*dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::PropertyDimAssign {
                                    context: semantic_context,
                                    object,
                                    property: RegionPropertyName::Dynamic(property),
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
                    InstructionKind::UnsetDynamicPropertyDim {
                        object,
                        property,
                        dims,
                    } => {
                        let dst = RegId::new(region_register_count);
                        region_register_count = region_register_count.saturating_add(1);
                        let object = lower_operand(unit, *object);
                        let property = lower_operand(unit, *property);
                        let dimensions = dims
                            .iter()
                            .map(|dim| lower_operand(unit, *dim))
                            .collect::<Vec<_>>();
                        let mut operands = vec![Some(object), Some(property)];
                        operands.extend(dimensions.iter().copied().map(Some));
                        RegionInstructionKind::NativeCall(RegionNativeCall {
                            result: RegionCallResult::Register(dst),
                            target: RegionCallTarget::Semantic {
                                operation: RegionSemanticOp::PropertyDimUnset {
                                    context: semantic_context,
                                    object,
                                    property: RegionPropertyName::Dynamic(property),
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
                        prepared_class: prepared_object_property_class(
                            unit,
                            *object,
                            &known_object_registers,
                            published_external_signatures,
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
                        prepared_class: prepared_object_property_class(
                            unit,
                            *object,
                            &known_object_registers,
                            published_external_signatures,
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
                        prepared_class: prepared_object_property_class(
                            unit,
                            *object,
                            &known_object_registers,
                            published_external_signatures,
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
                        prepared_class: prepared_object_property_class(
                            unit,
                            *object,
                            &known_object_registers,
                            published_external_signatures,
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
                        prepared_class: prepared_object_property_class(
                            unit,
                            *object,
                            &known_object_registers,
                            published_external_signatures,
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
                                        class_has_publication_stable_layout(
                                            unit,
                                            *class,
                                            published_external_signatures,
                                        )
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
                let directly_consumed =
                    directly_consumed_method_callable_register(&instruction.kind, &kind);
                let mut source_register_uses = Vec::new();
                php_ir::instruction_register_uses(&instruction.kind, &mut source_register_uses);
                for register in source_register_uses {
                    if Some(register) != directly_consumed {
                        consumed_method_callable_receivers.remove(&register);
                    }
                }
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
                    transition_live_registers: None,
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
                terminator_live_registers: None,
                terminator_state_locals: Vec::new(),
                source_terminator: source_terminator.kind.clone(),
                terminator,
            });
            next_continuation = next_continuation.saturating_add(1);
        }
        let continuation_capacity = native_continuation_capacity_upper_bound(unit, function)
            .expect("verified RegionGraph function must belong to its source unit");
        if next_continuation as usize > continuation_capacity {
            return Err(NativeCompileError::new(
                "JIT_REGION_REJECT_CONTINUATION_CAPACITY",
                format!(
                    "function={} emitted={} publication-capacity={continuation_capacity}",
                    function.raw(),
                    next_continuation,
                ),
            ));
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

include!("executable/control_flow_analysis.rs");

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
        .map(|argument| {
            Some(
                if argument.value_kind == IrCallArgValueKind::ByRefLocationPlaceholder {
                    argument
                        .by_ref_local
                        .map(RegionOperand::Local)
                        .unwrap_or_else(|| lower_operand(unit, argument.value))
                } else {
                    lower_operand(unit, argument.value)
                },
            )
        })
        .collect()
}

/// Builds the immutable source-to-parameter plan for a statically published
/// userland signature. This is compile-time PHP argument binding: generated
/// optimizing code receives only the resulting numeric operand order.
fn prepared_call_argument_sources(
    args: &[IrCallArg],
    parameters: &[php_ir::IrParam],
) -> Option<Vec<Option<usize>>> {
    prepared_call_argument_plan(args, parameters).map(|plan| plan.parameter_sources)
}

fn prepared_call_argument_plan(
    args: &[IrCallArg],
    parameters: &[php_ir::IrParam],
) -> Option<PreparedCallArgumentPlan> {
    let variadic_index = parameters.iter().position(|parameter| parameter.variadic);
    let fixed_count = variadic_index.unwrap_or(parameters.len());
    let mut assigned = vec![None; fixed_count];
    let mut variadic = Vec::new();
    let mut extra = Vec::new();
    let mut positional = 0usize;
    let mut saw_named = false;

    for (source, argument) in args.iter().enumerate() {
        if argument.unpack {
            return None;
        }
        if let Some(name) = argument.name.as_deref() {
            saw_named = true;
            let parameter = parameters[..fixed_count]
                .iter()
                .position(|parameter| parameter.name == name);
            let Some(parameter) = parameter else {
                // Unknown named arguments use PHP-visible string keys in a
                // variadic array. Keep that uncommon keyed shape at the one
                // exact baseline continuation; fixed named arguments remain
                // fully compile-time bound.
                return None;
            };
            if assigned[parameter].replace(source).is_some() {
                return None;
            }
            continue;
        }
        if saw_named {
            return None;
        }
        while positional < fixed_count && assigned[positional].is_some() {
            positional += 1;
        }
        if positional < fixed_count {
            assigned[positional] = Some(source);
            positional += 1;
        } else if variadic_index.is_some() {
            variadic.push(Some(source));
        } else {
            extra.push(source);
        }
    }
    let visible_fixed_count = if !variadic.is_empty() || !extra.is_empty() {
        fixed_count
    } else {
        assigned
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |index| index + 1)
    };
    let visible_variadic_sources = variadic.iter().flatten().copied().collect();
    assigned.extend(variadic);
    Some(PreparedCallArgumentPlan {
        parameter_sources: assigned,
        visible_fixed_count,
        visible_variadic_sources,
        extra_sources: extra,
    })
}

fn prepare_direct_call_operands(
    unit: &IrUnit,
    target: &php_ir::IrFunction,
    args: &[IrCallArg],
) -> Option<Vec<Option<RegionOperand>>> {
    prepare_direct_call_operands_for_parameters(unit, &target.params, args)
}

fn prepare_direct_call_operands_for_parameters(
    unit: &IrUnit,
    parameters: &[php_ir::IrParam],
    args: &[IrCallArg],
) -> Option<Vec<Option<RegionOperand>>> {
    prepare_direct_call_operands_with_defaults(unit, parameters, args, None)
}

fn prepare_direct_external_call_operands(
    unit: &IrUnit,
    signature: &crate::JitExternalFunctionSignature,
    args: &[IrCallArg],
) -> Option<Vec<Option<RegionOperand>>> {
    prepare_direct_call_operands_with_defaults(
        unit,
        &signature.native_params,
        args,
        Some((
            signature.link_index,
            &signature.native_default_constant_indices,
        )),
    )
}

fn prepare_direct_call_operands_with_defaults(
    unit: &IrUnit,
    parameters: &[php_ir::IrParam],
    args: &[IrCallArg],
    linked_defaults: Option<(u32, &[Option<u32>])>,
) -> Option<Vec<Option<RegionOperand>>> {
    let sources = prepared_call_argument_sources(args, parameters)?;
    let fixed_count = parameters
        .iter()
        .position(|parameter| parameter.variadic)
        .unwrap_or(parameters.len());
    let mut operands = Vec::with_capacity(sources.len());
    for (parameter_index, source) in sources.iter().take(fixed_count).copied().enumerate() {
        if let Some(source) = source {
            let argument = args.get(source)?;
            let operand = if parameters
                .get(parameter_index)
                .is_some_and(|parameter| parameter.by_ref)
            {
                argument
                    .by_ref_local
                    .map(RegionOperand::Local)
                    .unwrap_or_else(|| lower_operand(unit, argument.value))
            } else {
                lower_operand(unit, argument.value)
            };
            operands.push(Some(operand));
            continue;
        }
        let default = parameters.get(parameter_index)?.default.as_ref()?;
        if !publication_constant_is_stable(default) {
            return None;
        }
        let operand = unit
            .constants
            .iter()
            .position(|constant| constant == default)
            .and_then(|index| u32::try_from(index).ok())
            .map(RegionOperand::Constant)
            .or_else(|| {
                let (link_index, defaults) = linked_defaults?;
                defaults
                    .get(parameter_index)
                    .copied()
                    .flatten()
                    .map(|constant| RegionOperand::LinkedConstant {
                        link_index,
                        constant,
                        class: publication_constant_class(default),
                    })
            })?;
        operands.push(Some(operand));
    }
    for source in sources.into_iter().skip(fixed_count) {
        let argument = args.get(source?)?;
        let operand = if parameters
            .get(fixed_count)
            .is_some_and(|parameter| parameter.by_ref && parameter.variadic)
        {
            argument
                .by_ref_local
                .map(RegionOperand::Local)
                .unwrap_or_else(|| lower_operand(unit, argument.value))
        } else {
            lower_operand(unit, argument.value)
        };
        operands.push(Some(operand));
    }
    Some(operands)
}

fn mark_prepared_reference_arguments(args: &mut [IrCallArg], target: &php_ir::IrFunction) {
    mark_prepared_reference_arguments_for_parameters(args, &target.params);
}

fn mark_prepared_reference_arguments_for_parameters(
    args: &mut [IrCallArg],
    parameters: &[php_ir::IrParam],
) {
    let Some(sources) = prepared_call_argument_sources(args, parameters) else {
        return;
    };
    let variadic_index = parameters.iter().position(|parameter| parameter.variadic);
    for (parameter_index, source) in sources.into_iter().enumerate() {
        let Some(source) = source else {
            continue;
        };
        let parameter = parameters
            .get(parameter_index)
            .or_else(|| variadic_index.and_then(|index| parameters.get(index)));
        if parameter.is_some_and(|parameter| parameter.by_ref)
            && let Some(argument) = args.get_mut(source)
        {
            argument.value_kind = IrCallArgValueKind::ByRefLocationPlaceholder;
        }
    }
}

fn prepared_parameter_for_source<'a>(
    params: &'a [php_ir::IrParam],
    args: &[IrCallArg],
    source: usize,
) -> Option<&'a php_ir::IrParam> {
    let sources = prepared_call_argument_sources(args, params)?;
    let parameter_index = sources
        .iter()
        .position(|candidate| *candidate == Some(source))?;
    params
        .get(parameter_index)
        .or_else(|| params.last().filter(|parameter| parameter.variadic))
}

/// Reconstructs only the compile-time argument-binding surface of a fixed
/// internal function. Builtins are not userland `IrFunction`s, but their
/// generated php-src arginfo is just as immutable. Without this bridge,
/// by-reference builtin outputs such as `preg_match(..., $matches)` reached
/// optimizing lowering without a prepared native reference and were forced
/// through the generic dispatcher.
///
/// The placeholder defaults are used only to map supplied source arguments
/// to parameter positions. Exact builtin lowering owns the real omitted
/// defaults and PHP-visible validation.
fn resolved_internal_builtin_name(name: &str) -> Option<&str> {
    let normalized = name.trim_start_matches('\\');
    php_std::arginfo::function_metadata_indexed(normalized)
        .map(|_| normalized)
        .or_else(|| {
            let fallback = normalized.rsplit('\\').next()?;
            php_std::arginfo::function_metadata_indexed(fallback).map(|_| fallback)
        })
}

fn internal_builtin_binding_parameters(name: &str) -> Option<Vec<php_ir::IrParam>> {
    let metadata =
        php_std::arginfo::function_metadata_indexed(resolved_internal_builtin_name(name)?)?;
    Some(
        metadata
            .params
            .iter()
            .enumerate()
            .map(|(index, parameter)| php_ir::IrParam {
                name: parameter.name.to_owned(),
                local: LocalId::new(u32::try_from(index).unwrap_or(u32::MAX)),
                required: !parameter.optional && !parameter.variadic,
                default: parameter.optional.then_some(IrConstant::Null),
                type_: None,
                by_ref: parameter.by_ref,
                variadic: parameter.variadic,
                attributes: Vec::new(),
            })
            .collect(),
    )
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
        InstructionKind::FetchDim {
            array: Operand::Register(register),
            key,
            ..
        } if globals.contains(register) => known_string_operand(unit, *key, strings),
        InstructionKind::ArrayGet {
            array: Operand::Register(register),
            index,
            ..
        } if globals.contains(register) => known_string_operand(unit, *index, strings),
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

fn local_is_entry_reference(function: &php_ir::IrFunction, local: LocalId) -> bool {
    function
        .params
        .iter()
        .any(|parameter| parameter.local == local && parameter.by_ref)
        || function
            .captures
            .iter()
            .any(|capture| capture.local == local && capture.by_ref)
}

fn published_external_function_signature<'a>(
    signatures: &'a [crate::JitExternalFunctionSignature],
    name: &str,
) -> Option<&'a crate::JitExternalFunctionSignature> {
    let normalized = name.trim_start_matches('\\');
    (!normalized.contains("::")).then_some(())?;
    signatures.iter().find(|signature| {
        signature.published
            && signature
                .name
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(normalized)
    })
}

fn published_external_method_signature<'a>(
    signatures: &'a [crate::JitExternalFunctionSignature],
    class_name: &str,
    method: &str,
) -> Option<&'a crate::JitExternalFunctionSignature> {
    let class_name = class_name.trim_start_matches('\\');
    signatures.iter().find(|signature| {
        signature.published
            && signature.name.rsplit_once("::").is_some_and(
                |(candidate_class, candidate_method)| {
                    candidate_class
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case(class_name)
                        && candidate_method.eq_ignore_ascii_case(method)
                },
            )
    })
}

fn published_external_named_callable_signature<'a>(
    signatures: &'a [crate::JitExternalFunctionSignature],
    name: &str,
) -> Option<&'a crate::JitExternalFunctionSignature> {
    name.trim_start_matches('\\').split_once("::").map_or_else(
        || published_external_function_signature(signatures, name),
        |(class_name, method)| published_external_method_signature(signatures, class_name, method),
    )
}

fn known_external_object_class<'a>(
    operand: Operand,
    registers: &'a BTreeMap<RegId, String>,
    locals: &'a BTreeMap<LocalId, String>,
) -> Option<&'a str> {
    match operand {
        Operand::Register(register) => registers.get(&register).map(String::as_str),
        Operand::Local(local) => locals.get(&local).map(String::as_str),
        _ => None,
    }
}

fn elide_superseded_reference_argument_fetch(
    instructions: &mut [RegionInstruction],
    argument: &IrCallArg,
    source_register_use_counts: &[usize],
) {
    if argument.by_ref_dim.is_none()
        && argument.by_ref_property.is_none()
        && argument.by_ref_property_dim.is_none()
    {
        return;
    }
    let Operand::Register(value) = argument.value else {
        return;
    };
    if source_register_use_counts.get(value.index()).copied() != Some(1) {
        return;
    }
    let Some(producer) =
        instructions
            .iter_mut()
            .rev()
            .find(|instruction| match instruction.source_kind {
                InstructionKind::FetchDim {
                    dst,
                    mode: php_ir::instruction::DimFetchMode::Lvalue,
                    ..
                } => dst == value,
                InstructionKind::FetchProperty { dst, .. } => {
                    dst == value && argument.by_ref_property.is_some()
                }
                _ => false,
            })
    else {
        return;
    };
    // The direct reference binding below owns the complete root/dimension
    // recipe and all PHP-visible missing-key/COW behavior. Retaining the
    // value-producing lvalue fetch would execute the superseded warm path
    // before creating the authoritative reference, even though its SSA result
    // has no other consumer.
    producer.kind = RegionInstructionKind::Nop;
}

#[allow(clippy::too_many_arguments)]
fn prepare_reference_call_arguments(
    unit: &IrUnit,
    function: &php_ir::IrFunction,
    instruction: &php_ir::Instruction,
    args: &[IrCallArg],
    parameters: &[php_ir::IrParam],
    optimizing: bool,
    region_local_count: &mut u32,
    region_locals: &mut Vec<String>,
    known_object_registers: &BTreeMap<RegId, u32>,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    source_register_use_counts: &[usize],
    preceding_instructions: &mut [RegionInstruction],
) -> (Vec<IrCallArg>, Vec<RegionInstructionKind>) {
    let mut prepared = args.to_vec();
    let mut bindings = Vec::new();
    for (index, argument) in prepared.iter_mut().enumerate() {
        if !prepared_parameter_for_source(parameters, args, index)
            .is_some_and(|parameter| parameter.by_ref)
        {
            continue;
        }
        if optimizing {
            elide_superseded_reference_argument_fetch(
                preceding_instructions,
                argument,
                source_register_use_counts,
            );
        }
        if let Some(local) = argument.by_ref_local
            && !local_is_entry_reference(function, local)
        {
            bindings.push(RegionInstructionKind::BindReference {
                target: local,
                source: local,
            });
        } else if let Some(dimension) = &argument.by_ref_dim {
            let temporary = LocalId::new(*region_local_count);
            *region_local_count = (*region_local_count).saturating_add(1);
            region_locals.push(format!(
                "__phrust:by_ref_call_{}_{}",
                instruction.id.raw(),
                index
            ));
            argument.by_ref_local = Some(temporary);
            bindings.push(RegionInstructionKind::BindReferenceDim {
                target: temporary,
                array: dimension.local,
                keys: dimension
                    .dims
                    .iter()
                    .map(|operand| lower_operand(unit, *operand))
                    .collect(),
            });
        } else if let Some(property) = &argument.by_ref_property {
            let temporary = LocalId::new(*region_local_count);
            *region_local_count = (*region_local_count).saturating_add(1);
            region_locals.push(format!(
                "__phrust:by_ref_call_{}_{}",
                instruction.id.raw(),
                index
            ));
            argument.by_ref_local = Some(temporary);
            bindings.push(RegionInstructionKind::BindReferenceFromProperty {
                target: temporary,
                object: lower_operand(unit, property.object),
                property: property.property.clone(),
                prepared_class: prepared_object_property_class(
                    unit,
                    property.object,
                    known_object_registers,
                    external_function_signatures,
                ),
            });
        } else if optimizing && let Some(property) = &argument.by_ref_property_dim {
            let temporary = LocalId::new(*region_local_count);
            *region_local_count = (*region_local_count).saturating_add(1);
            region_locals.push(format!(
                "__phrust:by_ref_call_{}_{}",
                instruction.id.raw(),
                index
            ));
            argument.by_ref_local = Some(temporary);
            bindings.push(RegionInstructionKind::BindReferenceFromPropertyDim {
                target: temporary,
                object: lower_operand(unit, property.object),
                keys: property
                    .dims
                    .iter()
                    .map(|operand| lower_operand(unit, *operand))
                    .collect(),
                property: property.property.clone(),
                prepared_class: prepared_object_property_class(
                    unit,
                    property.object,
                    known_object_registers,
                    external_function_signatures,
                ),
            });
        }
        if argument.by_ref_local.is_some() {
            // The preceding binding instruction now owns the complete lvalue
            // recipe. Calls in both tiers consume only its authoritative
            // native reference local; retaining the original dimension or
            // property shape here creates a second call-time lvalue plane and
            // makes baseline continuations demand already-consumed operands.
            // Unprepared property dimensions have no local and therefore
            // remain on their single exact baseline boundary.
            argument.by_ref_dim = None;
            argument.by_ref_property = None;
            argument.by_ref_property_dim = None;
        }
    }
    (prepared, bindings)
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

fn publication_constant_class(constant: &IrConstant) -> super::SsaValueClass {
    match constant {
        IrConstant::Null => super::SsaValueClass::Null,
        IrConstant::Bool(_) => super::SsaValueClass::Bool,
        IrConstant::Int(_) => super::SsaValueClass::Int,
        IrConstant::Float(_) => super::SsaValueClass::Float,
        IrConstant::String(_) | IrConstant::StringBytes(_) => super::SsaValueClass::StringHandle,
        IrConstant::Array(_) => super::SsaValueClass::ArrayHandle,
        IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => {
            super::SsaValueClass::MixedHandle
        }
    }
}

fn local_class_external_parent(unit: &IrUnit, class_index: u32) -> Option<&str> {
    let mut class = unit.classes.get(class_index as usize)?;
    let mut visited = std::collections::BTreeSet::new();
    loop {
        if !visited.insert(class.name.as_str()) {
            return None;
        }
        let parent = class.parent.as_deref()?;
        let Some((_, local_parent)) = find_class(unit, parent) else {
            return Some(parent);
        };
        class = local_parent;
    }
}

fn published_external_parent_constructor_signature<'a>(
    unit: &IrUnit,
    class_index: u32,
    signatures: &'a [crate::JitExternalFunctionSignature],
) -> Option<&'a crate::JitExternalFunctionSignature> {
    published_external_method_signature(
        signatures,
        local_class_external_parent(unit, class_index)?,
        "__construct",
    )
}

fn direct_external_constructor_signature<'a>(
    unit: &IrUnit,
    signature: &'a crate::JitExternalFunctionSignature,
    args: &[IrCallArg],
) -> Option<&'a crate::JitExternalFunctionSignature> {
    (!signature.requires_non_reference_trampoline
        && !signature.returns_by_reference
        && (usize::try_from(signature.native_arity).ok()
            == Some(signature.native_params.len().saturating_add(1))
            && prepare_direct_external_call_operands(unit, signature, args).is_some()
            || signature.native_arity == 0
                && signature.native_params.is_empty()
                && args.is_empty()))
    .then_some(signature)
}

fn class_has_publication_stable_layout(
    unit: &IrUnit,
    class_index: u32,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
) -> bool {
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
            // The cross-unit call index publishes an external-parent class
            // dependency before optimizing this local allocation. A
            // published constructor signature, including the zero-arity
            // class-only form, therefore proves that the parent's immutable
            // prepared layout already exists. Dynamically unresolved and
            // internal parents retain their exact baseline boundary.
            return published_external_method_signature(
                external_function_signatures,
                parent,
                "__construct",
            )
            .is_some();
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

/// Resolves one same-unit method whose target cannot change for the admitted
/// receiver. This is the shared publication contract for ordinary calls,
/// by-reference argument preparation, and reference-return binding; keeping
/// separate resolvers made the latter two fall back despite carrying the same
/// exact receiver and method identity.
fn stable_local_method_function(
    unit: &IrUnit,
    receiver: Operand,
    method: &str,
    known_object_registers: &BTreeMap<RegId, u32>,
    exact_object_registers: &BTreeSet<RegId>,
) -> Option<FunctionId> {
    known_object_class(receiver, known_object_registers)
        .and_then(|class| {
            let class = unit.classes.get(class as usize)?;
            let class_is_final = class.flags.is_final;
            let receiver_is_exact = matches!(
                receiver,
                Operand::Register(register) if exact_object_registers.contains(&register)
            );
            class
                .methods
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(method))
                .filter(|entry| {
                    !entry.flags.is_private
                        && !entry.flags.is_protected
                        && (receiver_is_exact || class_is_final || entry.flags.is_final)
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
}

fn prepared_object_property_class(
    unit: &IrUnit,
    operand: Operand,
    known_registers: &BTreeMap<RegId, u32>,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
) -> Option<u32> {
    let Operand::Register(register) = operand else {
        return None;
    };
    known_registers.get(&register).copied().filter(|class| {
        class_has_publication_stable_layout(unit, *class, external_function_signatures)
    })
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
    let capture_count = captures
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
        .collect::<Option<Vec<_>>>()?
        .len();
    Some(KnownClosure {
        function: closure_function,
        capture_count,
        bound_object: None,
        // `find_function` resolves only an ordinary same-unit function.
        // Such a factory publishes no lexical class/called-class context;
        // the prepared closure record itself owns every capture needed by a
        // direct invocation. Method and nested-closure factories never enter
        // this analysis and remain on their exact runtime-context boundary.
        requires_runtime_context: false,
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
    let prepared_operands = prepare_direct_call_operands(unit, target, args);
    let direct_arity = prepared_operands
        .as_ref()
        .and_then(|_| u32::try_from(target.params.len()).ok());
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
    let operands = prepared_operands.unwrap_or_else(|| lower_call_operands(unit, args));
    let mut native_args = args.to_vec();
    mark_prepared_reference_arguments(&mut native_args, target);
    RegionInstructionKind::NativeCall(RegionNativeCall {
        result: RegionCallResult::Register(dst),
        target: RegionCallTarget::Function {
            name,
            function: (!is_generator).then_some(function),
        },
        args: native_args,
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
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    dst: RegId,
    name: String,
    args: &[IrCallArg],
) -> (RegionInstructionKind, bool) {
    let static_method = name.split_once("::");
    if let Some((class_name, method)) = static_method
        && let Some(function) = find_direct_static_method(unit, class_name, method)
    {
        let call = lower_direct_function_call(unit, dst, name, function, args);
        let direct = matches!(
            &call,
            RegionInstructionKind::NativeCall(call)
                if call.direct_compiled_target().is_some()
        );
        return (call, direct);
    }
    if static_method.is_none()
        && let Some(function) = find_function(unit, &name)
    {
        let call = lower_direct_function_call(unit, dst, name, function, args);
        let direct = matches!(
            &call,
            RegionInstructionKind::NativeCall(call)
                if call.direct_compiled_target().is_some()
        );
        return (call, direct);
    }
    if let Some(signature) =
        published_external_named_callable_signature(external_function_signatures, &name)
    {
        let prepared_operands = prepare_direct_external_call_operands(unit, signature, args);
        let direct = prepared_operands.is_some()
            && !signature.requires_non_reference_trampoline
            && !signature.returns_by_reference;
        let direct_arity = prepared_operands.as_ref().map(|_| signature.native_arity);
        let operands = prepared_operands.unwrap_or_else(|| lower_call_operands(unit, args));
        let mut native_args = args.to_vec();
        mark_prepared_reference_arguments_for_parameters(
            &mut native_args,
            &signature.native_params,
        );
        let variadic = signature
            .native_params
            .last()
            .is_some_and(|parameter| parameter.variadic);
        return (
            RegionInstructionKind::NativeCall(RegionNativeCall {
                result: RegionCallResult::Register(dst),
                target: RegionCallTarget::Function {
                    name,
                    function: None,
                },
                args: native_args,
                argument_operand_offset: 0,
                operands,
                direct_arity,
                variadic,
                returns_by_reference: signature.returns_by_reference,
                caller_strict_types: unit.strict_types,
            }),
            direct,
        );
    }
    if let Some((class_name, method)) = static_method {
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

fn stable_callback_parameters_are_directly_bindable(
    parameters: &[php_ir::IrParam],
    argument_count: usize,
    allow_direct_types: bool,
) -> bool {
    let fixed_count = parameters
        .iter()
        .position(|parameter| parameter.variadic)
        .unwrap_or(parameters.len());
    let variadic = fixed_count < parameters.len();
    if parameters.iter().any(|parameter| parameter.by_ref) {
        // array_map/filter/reduce deliberately pass callback operands by
        // value. PHP warns for a by-reference declaration on every
        // invocation, so retain the stable callback plan and let optimizing
        // lowering take one baseline continuation before entering the loop.
        return true;
    }
    if argument_count > fixed_count && !variadic {
        return false;
    }
    if parameters
        .iter()
        .take(fixed_count)
        .skip(argument_count)
        .any(|parameter| parameter.required || parameter.default.is_none())
    {
        return false;
    }
    parameters
        .iter()
        .take(argument_count.min(fixed_count))
        .chain(
            parameters
                .get(fixed_count)
                .filter(|parameter| parameter.variadic && argument_count > fixed_count),
        )
        .all(|parameter| {
            parameter.type_.as_ref().is_none_or(|type_| {
                allow_direct_types && stable_callback_type_has_direct_guard(type_)
            })
        })
}

fn stable_callback_has_releasable_scalar_return(
    unit: &IrUnit,
    callback: &RegionStableCallback,
) -> bool {
    callback
        .function
        .and_then(|function| unit.functions.get(function.index()))
        .is_some_and(|function| {
            !function.returns_by_ref
                && function
                    .return_type
                    .as_ref()
                    .is_some_and(callback_return_type_is_releasable_scalar)
        })
}

fn callback_return_type_is_releasable_scalar(type_: &IrReturnType) -> bool {
    match type_ {
        IrReturnType::Int
        | IrReturnType::Float
        | IrReturnType::String
        | IrReturnType::Bool
        | IrReturnType::Null
        | IrReturnType::False
        | IrReturnType::True
        | IrReturnType::Void
        | IrReturnType::Never => true,
        IrReturnType::Nullable { inner } => callback_return_type_is_releasable_scalar(inner),
        IrReturnType::Union { members } => {
            !members.is_empty()
                && members
                    .iter()
                    .all(callback_return_type_is_releasable_scalar)
        }
        IrReturnType::Array
        | IrReturnType::Callable
        | IrReturnType::Iterable
        | IrReturnType::Object
        | IrReturnType::Mixed
        | IrReturnType::Class { .. }
        | IrReturnType::Intersection { .. }
        | IrReturnType::Dnf { .. } => false,
    }
}

fn stable_callback_type_has_direct_guard(type_: &php_ir::IrReturnType) -> bool {
    match type_ {
        php_ir::IrReturnType::Int
        | php_ir::IrReturnType::Float
        | php_ir::IrReturnType::String
        | php_ir::IrReturnType::Array
        | php_ir::IrReturnType::Callable
        | php_ir::IrReturnType::Iterable
        | php_ir::IrReturnType::Object
        | php_ir::IrReturnType::Bool
        | php_ir::IrReturnType::Null
        | php_ir::IrReturnType::False
        | php_ir::IrReturnType::True
        | php_ir::IrReturnType::Mixed => true,
        php_ir::IrReturnType::Nullable { .. } => true,
        php_ir::IrReturnType::Union { members } => {
            members.iter().any(stable_callback_type_has_direct_guard)
        }
        php_ir::IrReturnType::Class { name, .. } => matches!(
            name.trim_start_matches('\\').to_ascii_lowercase().as_str(),
            "closure" | "generator" | "fiber"
        ),
        php_ir::IrReturnType::Void
        | php_ir::IrReturnType::Never
        | php_ir::IrReturnType::Intersection { .. }
        | php_ir::IrReturnType::Dnf { .. } => false,
    }
}

fn stable_named_array_callback(
    unit: &IrUnit,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    name: &str,
    argument_count: usize,
    allow_direct_types: bool,
) -> Option<RegionStableCallback> {
    let local_function = name
        .split_once("::")
        .and_then(|(class_name, method)| find_direct_static_method(unit, class_name, method))
        .or_else(|| {
            (!name.contains("::"))
                .then(|| find_function(unit, name))
                .flatten()
        });
    if let Some(function) = local_function {
        let target = unit.functions.get(function.index())?;
        if target.flags.is_generator
            || !stable_callback_parameters_are_directly_bindable(
                &target.params,
                argument_count,
                allow_direct_types,
            )
        {
            return None;
        }
        return Some(RegionStableCallback {
            name: name.to_owned(),
            function: Some(function),
            receiver: None,
            closure: None,
            bound_object_count: 0,
            capture_count: 0,
            returns_int: matches!(target.return_type.as_ref(), Some(IrReturnType::Int)),
            returns_string: matches!(target.return_type.as_ref(), Some(IrReturnType::String)),
            returns_releasable_scalar: target
                .return_type
                .as_ref()
                .is_some_and(callback_return_type_is_releasable_scalar),
        });
    }
    let signature =
        published_external_named_callable_signature(external_function_signatures, name)?;
    (signature.native_arity as usize == signature.native_params.len()
        && stable_callback_parameters_are_directly_bindable(
            &signature.native_params,
            argument_count,
            allow_direct_types,
        ))
    .then(|| RegionStableCallback {
        name: name.to_owned(),
        function: None,
        receiver: None,
        closure: None,
        bound_object_count: 0,
        capture_count: 0,
        returns_int: false,
        returns_string: false,
        returns_releasable_scalar: false,
    })
}

fn stable_instance_method_function(
    unit: &IrUnit,
    class_name: &str,
    method: &str,
) -> Option<FunctionId> {
    let (_, class) = find_class(unit, class_name)?;
    let entry = class.methods.iter().find(|entry| {
        entry.name.eq_ignore_ascii_case(method)
            && !entry.flags.is_static
            && !entry.flags.is_private
            && !entry.flags.is_protected
    })?;
    let function = unit.functions.get(entry.function.index())?;
    let has_late_static_operation = function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                &instruction.kind,
                InstructionKind::FetchClassConstant { class_name, .. }
                    | InstructionKind::CallStaticMethod { class_name, .. }
                    if class_name.eq_ignore_ascii_case("static")
            )
        })
    });
    (!function.flags.is_generator && !has_late_static_operation).then_some(entry.function)
}

fn stable_method_array_callback(
    unit: &IrUnit,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    callable: &KnownMethodCallableArray,
    argument_count: usize,
    allow_direct_types: bool,
) -> Option<RegionStableCallback> {
    if callable.length != 2 {
        return None;
    }
    let method = callable.method.as_deref()?;
    let target = callable.target.as_ref()?;
    let (class_name, receiver) = match target {
        KnownMethodCallableTarget::Static { class_name } => {
            let name = format!("{class_name}::{method}");
            return stable_named_array_callback(
                unit,
                external_function_signatures,
                &name,
                argument_count,
                allow_direct_types,
            );
        }
        KnownMethodCallableTarget::Instance {
            receiver,
            class_name,
        } => (class_name.as_str(), *receiver),
    };
    let name = format!("{class_name}::{method}");
    if let Some(function_id) = stable_instance_method_function(unit, class_name, method) {
        let function = unit.functions.get(function_id.index())?;
        if stable_callback_parameters_are_directly_bindable(
            &function.params,
            argument_count,
            allow_direct_types,
        ) {
            return Some(RegionStableCallback {
                name,
                function: Some(function_id),
                receiver: Some(receiver),
                closure: None,
                bound_object_count: 1,
                capture_count: 0,
                returns_int: matches!(function.return_type.as_ref(), Some(IrReturnType::Int)),
                returns_string: matches!(function.return_type.as_ref(), Some(IrReturnType::String)),
                returns_releasable_scalar: function
                    .return_type
                    .as_ref()
                    .is_some_and(callback_return_type_is_releasable_scalar),
            });
        }
        return None;
    }

    let signature =
        published_external_method_signature(external_function_signatures, class_name, method)?;
    (signature.native_arity as usize == signature.native_params.len().saturating_add(1)
        && stable_callback_parameters_are_directly_bindable(
            &signature.native_params,
            argument_count,
            allow_direct_types,
        ))
    .then_some(RegionStableCallback {
        name,
        function: None,
        receiver: Some(receiver),
        closure: None,
        bound_object_count: 1,
        capture_count: 0,
        returns_int: false,
        returns_string: false,
        returns_releasable_scalar: false,
    })
}

#[allow(clippy::too_many_arguments)]
fn stable_array_callback_operand(
    unit: &IrUnit,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    operand: Operand,
    argument_count: usize,
    allow_direct_types: bool,
    register_strings: &BTreeMap<RegId, String>,
    local_strings: &BTreeMap<LocalId, String>,
    register_callables: &BTreeMap<RegId, String>,
    local_callables: &BTreeMap<LocalId, String>,
    register_method_callables: &BTreeMap<RegId, KnownMethodCallableArray>,
    local_method_callables: &BTreeMap<LocalId, KnownMethodCallableArray>,
    register_closures: &BTreeMap<RegId, KnownClosure>,
    local_closures: &BTreeMap<LocalId, KnownClosure>,
) -> Option<RegionStableCallback> {
    if let Some(name) = known_callable_operand_name(
        unit,
        operand,
        register_strings,
        local_strings,
        register_callables,
        local_callables,
    ) && let Some(callback) = stable_named_array_callback(
        unit,
        external_function_signatures,
        &name,
        argument_count,
        allow_direct_types,
    ) {
        return Some(callback);
    }

    let method_callable = match operand {
        Operand::Register(register) => register_method_callables.get(&register),
        Operand::Local(local) => local_method_callables.get(&local),
        Operand::Constant(_) => None,
    };
    if let Some(callback) = method_callable.and_then(|callable| {
        stable_method_array_callback(
            unit,
            external_function_signatures,
            callable,
            argument_count,
            allow_direct_types,
        )
    }) {
        return Some(callback);
    }

    let closure = match operand {
        Operand::Register(register) => register_closures.get(&register),
        Operand::Local(local) => local_closures.get(&local),
        Operand::Constant(_) => None,
    }?;
    let target = unit.functions.get(closure.function.index())?;
    if closure.requires_runtime_context
        || target.flags.is_generator
        || closure.capture_count != target.captures.len()
        || !stable_callback_parameters_are_directly_bindable(
            &target.params,
            argument_count,
            allow_direct_types,
        )
    {
        return None;
    }
    Some(RegionStableCallback {
        name: target.name.clone(),
        function: Some(closure.function),
        receiver: None,
        closure: Some(lower_operand(unit, operand)),
        bound_object_count: usize::from(closure.bound_object.is_some()),
        capture_count: closure.capture_count,
        returns_int: matches!(target.return_type.as_ref(), Some(IrReturnType::Int)),
        returns_string: matches!(target.return_type.as_ref(), Some(IrReturnType::String)),
        returns_releasable_scalar: target
            .return_type
            .as_ref()
            .is_some_and(callback_return_type_is_releasable_scalar),
    })
}

#[allow(clippy::too_many_arguments)]
fn planned_preg_callback_array(
    unit: &IrUnit,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    callback_map: &KnownPregCallbackArray,
    callback_map_operand: RegionOperand,
    register_strings: &BTreeMap<RegId, String>,
    local_strings: &BTreeMap<LocalId, String>,
    register_callables: &BTreeMap<RegId, String>,
    local_callables: &BTreeMap<LocalId, String>,
    register_method_callables: &BTreeMap<RegId, KnownMethodCallableArray>,
    local_method_callables: &BTreeMap<LocalId, KnownMethodCallableArray>,
    register_closures: &BTreeMap<RegId, KnownClosure>,
    local_closures: &BTreeMap<LocalId, KnownClosure>,
) -> Option<Vec<RegionPregCallbackArrayEntry>> {
    let mut pattern_cache = php_runtime::experimental::pcre::PcreCache::default();
    callback_map
        .entries
        .iter()
        .map(|(pattern, callable)| {
            let pattern_bytes = known_string_operand(unit, *pattern, register_strings)?;
            pattern_cache
                .compile_bytes_with_limits(
                    pattern_bytes.as_bytes(),
                    php_runtime::experimental::pcre::PcreMatchLimits::default(),
                )
                .ok()?;
            let callback = stable_array_callback_operand(
                unit,
                external_function_signatures,
                *callable,
                1,
                true,
                register_strings,
                local_strings,
                register_callables,
                local_callables,
                register_method_callables,
                local_method_callables,
                register_closures,
                local_closures,
            )
            .filter(|callback| stable_callback_has_releasable_scalar_return(unit, callback))
            .map_or(
                RegionArrayCallbackTarget::Runtime(callback_map_operand),
                RegionArrayCallbackTarget::Stable,
            );
            Some(RegionPregCallbackArrayEntry {
                pattern: lower_operand(unit, *pattern),
                callback,
            })
        })
        .collect()
}

fn known_method_callable_operand<'a>(
    operand: Operand,
    register_callables: &'a BTreeMap<RegId, KnownMethodCallableArray>,
    local_callables: &'a BTreeMap<LocalId, KnownMethodCallableArray>,
) -> Option<&'a KnownMethodCallableArray> {
    match operand {
        Operand::Register(register) => register_callables.get(&register),
        Operand::Local(local) => local_callables.get(&local),
        Operand::Constant(_) => None,
    }
}

fn directly_consumed_method_callable_register(
    source: &InstructionKind,
    lowered: &RegionInstructionKind,
) -> Option<RegId> {
    match source {
        InstructionKind::CallFunction { name, args, .. } => {
            let normalized = name.trim_start_matches('\\');
            let callback_index = if normalized.eq_ignore_ascii_case("array_map")
                || normalized.eq_ignore_ascii_case("call_user_func")
                || normalized.eq_ignore_ascii_case("call_user_func_array")
                || normalized.eq_ignore_ascii_case("preg_replace_callback_array")
            {
                0
            } else if normalized.eq_ignore_ascii_case("array_filter")
                || normalized.eq_ignore_ascii_case("array_reduce")
                || normalized.eq_ignore_ascii_case("usort")
                || normalized.eq_ignore_ascii_case("uasort")
                || normalized.eq_ignore_ascii_case("uksort")
                || normalized.eq_ignore_ascii_case("array_walk")
                || normalized.eq_ignore_ascii_case("preg_replace_callback")
            {
                1
            } else {
                return None;
            };
            let direct = match lowered {
                RegionInstructionKind::ArrayCallback(_)
                | RegionInstructionKind::PregCallbackArray(_) => true,
                RegionInstructionKind::NativeCall(call)
                    if normalized.eq_ignore_ascii_case("call_user_func")
                        || normalized.eq_ignore_ascii_case("call_user_func_array") =>
                {
                    matches!(
                        &call.target,
                        RegionCallTarget::Function { name: target, .. }
                            if !target.eq_ignore_ascii_case(normalized)
                    )
                }
                _ => false,
            };
            if !direct {
                return None;
            }
            match args.get(callback_index)?.value {
                Operand::Register(register) => Some(register),
                Operand::Local(_) | Operand::Constant(_) => None,
            }
        }
        InstructionKind::CallCallable { callee, .. }
        | InstructionKind::Pipe {
            callable: callee, ..
        } if matches!(
            lowered,
            RegionInstructionKind::NativeCall(RegionNativeCall {
                target: RegionCallTarget::Function { .. },
                ..
            })
        ) =>
        {
            match callee {
                Operand::Register(register) => Some(*register),
                Operand::Local(_) | Operand::Constant(_) => None,
            }
        }
        _ => None,
    }
}

fn known_closure_operand<'a>(
    operand: Operand,
    register_closures: &'a BTreeMap<RegId, KnownClosure>,
    local_closures: &'a BTreeMap<LocalId, KnownClosure>,
) -> Option<&'a KnownClosure> {
    match operand {
        Operand::Register(register) => register_closures.get(&register),
        Operand::Local(local) => local_closures.get(&local),
        Operand::Constant(_) => None,
    }
}

fn constant_integer_operand(unit: &IrUnit, operand: Operand) -> Option<i64> {
    let Operand::Constant(constant) = operand else {
        return None;
    };
    match unit.constants.get(constant.index()) {
        Some(IrConstant::Int(value)) => Some(*value),
        _ => None,
    }
}

fn constant_null_operand(unit: &IrUnit, operand: Operand) -> bool {
    let Operand::Constant(constant) = operand else {
        return false;
    };
    matches!(unit.constants.get(constant.index()), Some(IrConstant::Null))
}

fn stable_named_callable_is_by_value_only(
    unit: &IrUnit,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    name: &str,
) -> bool {
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
    if let Some(signature) =
        published_external_named_callable_signature(external_function_signatures, name)
    {
        return !signature.requires_non_reference_trampoline
            && !signature.returns_by_reference
            && signature
                .native_params
                .iter()
                .all(|parameter| !parameter.by_ref);
    }
    let normalized = name.trim_start_matches('\\');
    !normalized.contains("::")
        && php_std::arginfo::function_metadata_indexed(normalized)
            .is_some_and(|function| function.params.iter().all(|parameter| !parameter.by_ref))
}

fn stable_named_callable_accepts_direct_unpack(
    unit: &IrUnit,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    name: &str,
) -> bool {
    let local_target = name
        .split_once("::")
        .and_then(|(class_name, method)| find_direct_static_method(unit, class_name, method))
        .or_else(|| find_function(unit, name));
    if let Some(function) = local_target {
        return unit
            .functions
            .get(function.index())
            .is_some_and(|function| {
                !function.flags.is_generator
                    && !function.returns_by_ref
                    && stable_unpack_callback_parameters_are_direct(&function.params)
            });
    }
    published_external_named_callable_signature(external_function_signatures, name).is_some_and(
        |signature| {
            !signature.requires_non_reference_trampoline
                && !signature.returns_by_reference
                && signature.native_arity as usize == signature.native_params.len()
                && stable_unpack_callback_parameters_are_direct(&signature.native_params)
        },
    )
}

fn lower_direct_method_call(
    unit: &IrUnit,
    dst: RegId,
    function: FunctionId,
    receiver: Operand,
    args: &[IrCallArg],
) -> RegionInstructionKind {
    lower_direct_method_call_result(
        unit,
        RegionCallResult::Register(dst),
        function,
        receiver,
        args,
    )
}

fn lower_direct_method_call_result(
    unit: &IrUnit,
    result: RegionCallResult,
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
    let prepared_operands = prepare_direct_call_operands(unit, target, args);
    let direct_arity = prepared_operands
        .as_ref()
        .and_then(|_| u32::try_from(receiver_count + target.params.len()).ok());
    let mut operands = if is_static {
        Vec::new()
    } else {
        vec![Some(lower_operand(unit, receiver))]
    };
    operands.extend(prepared_operands.unwrap_or_else(|| lower_call_operands(unit, args)));
    let mut native_args = args.to_vec();
    mark_prepared_reference_arguments(&mut native_args, target);
    RegionInstructionKind::NativeCall(RegionNativeCall {
        result,
        target: RegionCallTarget::Function {
            name: target.name.clone(),
            function: Some(function),
        },
        args: native_args,
        argument_operand_offset: receiver_count,
        operands,
        direct_arity,
        variadic,
        returns_by_reference: target.returns_by_ref,
        caller_strict_types: unit.strict_types,
    })
}

fn lower_specialized_method_call(
    unit: &IrUnit,
    result: RegionCallResult,
    function: FunctionId,
    receiver: Operand,
    method: &str,
    receiver_layout_id: u64,
    args: &[IrCallArg],
) -> RegionInstructionKind {
    let mut lowered = lower_direct_method_call_result(unit, result, function, receiver, args);
    let RegionInstructionKind::NativeCall(call) = &mut lowered else {
        unreachable!("direct method lowering must produce one native call");
    };
    call.target = RegionCallTarget::Method {
        receiver,
        method: method.to_owned(),
        function: Some(function),
        linked_function: None,
        receiver_layout_id: Some(receiver_layout_id),
    };
    lowered
}

fn lower_specialized_external_method_call(
    unit: &IrUnit,
    result: RegionCallResult,
    signature: &crate::JitExternalFunctionSignature,
    receiver: Operand,
    method: &str,
    receiver_layout_id: u64,
    args: &[IrCallArg],
) -> Option<RegionInstructionKind> {
    let mut lowered =
        lower_direct_external_method_call(unit, result, signature, Some(receiver), args)?;
    let RegionInstructionKind::NativeCall(call) = &mut lowered else {
        unreachable!("direct external method lowering must produce one native call");
    };
    call.target = RegionCallTarget::Method {
        receiver,
        method: method.to_owned(),
        function: None,
        linked_function: Some(signature.link_index),
        receiver_layout_id: Some(receiver_layout_id),
    };
    Some(lowered)
}

fn lower_direct_external_method_call(
    unit: &IrUnit,
    result: RegionCallResult,
    signature: &crate::JitExternalFunctionSignature,
    receiver: Option<Operand>,
    args: &[IrCallArg],
) -> Option<RegionInstructionKind> {
    if signature.requires_non_reference_trampoline
        || signature.returns_by_reference != matches!(result, RegionCallResult::ReferenceLocal(_))
    {
        return None;
    }
    let receiver_count = usize::try_from(signature.native_arity)
        .ok()?
        .checked_sub(signature.native_params.len())?;
    if receiver_count > 1 || receiver_count != usize::from(receiver.is_some()) {
        return None;
    }
    let prepared_operands = prepare_direct_external_call_operands(unit, signature, args)?;
    let mut operands = receiver
        .map(|receiver| vec![Some(lower_operand(unit, receiver))])
        .unwrap_or_default();
    operands.extend(prepared_operands.clone());
    let mut native_args = args.to_vec();
    mark_prepared_reference_arguments_for_parameters(&mut native_args, &signature.native_params);
    Some(RegionInstructionKind::NativeCall(RegionNativeCall {
        result,
        target: RegionCallTarget::Function {
            name: signature.name.clone(),
            function: None,
        },
        args: native_args,
        argument_operand_offset: receiver_count,
        operands,
        direct_arity: u32::try_from(receiver_count + signature.native_params.len()).ok(),
        variadic: signature
            .native_params
            .last()
            .is_some_and(|parameter| parameter.variadic),
        returns_by_reference: signature.returns_by_reference,
        caller_strict_types: unit.strict_types,
    }))
}

fn stable_unpack_callback_parameters_are_direct(parameters: &[php_ir::IrParam]) -> bool {
    parameters.iter().all(|parameter| {
        // By-reference admission belongs to each authoritative direct-array
        // entry, not to the array operand. Optimizing lowering scans every
        // supplied entry before allocating defaults or invoking the callee,
        // so fixed and variadic by-reference parameters share the direct
        // compiled boundary. A rejected entry takes the one complete
        // continuation before any binding effect.
        parameter.default.as_ref().is_none_or(|default| {
            matches!(
                default,
                IrConstant::Null
                    | IrConstant::Bool(_)
                    | IrConstant::Int(_)
                    | IrConstant::Float(_)
                    | IrConstant::String(_)
                    | IrConstant::StringBytes(_)
            )
        }) && parameter
            .type_
            .as_ref()
            .is_none_or(stable_callback_type_has_direct_guard)
    })
}

include!("executable/method_callable_lowering.rs");

/// Returns the bound receiver when a closure's complete body is the exact
/// `static::class` projection.
///
/// A closure created in a method normally needs lexical runtime context and
/// therefore cannot be compiled as an ordinary context-free function. This
/// one vertical shape is different: the prepared closure already owns the
/// authoritative receiver, and its dynamic class name is the complete PHP
/// result. Recognizing the whole body keeps that operation on its dedicated
/// native metadata handler without admitting arbitrary context-dependent
/// closure bodies.
fn exact_bound_closure_class_receiver(
    unit: &IrUnit,
    closure: &KnownClosure,
) -> Option<RegionOperand> {
    let bound_object = closure.bound_object?;
    let target = unit.functions.get(closure.function.index())?;
    let [block] = target.blocks.as_slice() else {
        return None;
    };
    let [instruction] = block.instructions.as_slice() else {
        return None;
    };
    let InstructionKind::FetchClassConstant {
        dst,
        class_name,
        constant,
    } = &instruction.kind
    else {
        return None;
    };
    if !class_name.eq_ignore_ascii_case("static") || !constant.eq_ignore_ascii_case("class") {
        return None;
    }
    let returned = block.terminator.as_ref().is_some_and(|terminator| {
        matches!(
            &terminator.kind,
            TerminatorKind::Return {
                value: Some(Operand::Register(register)),
                by_ref_local: None,
            } if *register == *dst
        )
    });
    returned.then_some(bound_object)
}

fn direct_closure_runtime_context_is_lowerable(unit: &IrUnit, closure: &KnownClosure) -> bool {
    !closure.requires_runtime_context || exact_bound_closure_class_receiver(unit, closure).is_some()
}

fn lower_direct_closure_call(
    unit: &IrUnit,
    dst: RegId,
    callee: Operand,
    closure: KnownClosure,
    args: &[IrCallArg],
    semantic_context: RegionSemanticContext,
) -> RegionInstructionKind {
    let target = &unit.functions[closure.function.index()];
    let variadic = target
        .params
        .last()
        .is_some_and(|parameter| parameter.variadic);
    if let Some(bound_object) = exact_bound_closure_class_receiver(unit, &closure) {
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
    let prepared_operands = prepare_direct_call_operands(unit, target, args);
    let trailing_unpack = args.split_last().is_some_and(|(last, prefix)| {
        last.unpack
            && last.name.is_none()
            && prefix
                .iter()
                .all(|argument| argument.name.is_none() && !argument.unpack)
    });
    if prepared_operands.is_none() && !trailing_unpack {
        let mut operands = vec![Some(lower_operand(unit, callee))];
        operands.extend(lower_call_operands(unit, args));
        return RegionInstructionKind::NativeCall(RegionNativeCall {
            result: RegionCallResult::Register(dst),
            target: RegionCallTarget::Closure {
                callee,
                function: None,
                bound_object_count: 0,
                capture_count: 0,
            },
            args: args.to_vec(),
            argument_operand_offset: 1,
            operands,
            direct_arity: None,
            variadic: false,
            returns_by_reference: false,
            caller_strict_types: unit.strict_types,
        });
    }
    let direct_arity = prepared_operands.as_ref().and_then(|_| {
        u32::try_from(bound_object_count + target.captures.len() + target.params.len()).ok()
    });
    let argument_operand_offset = bound_object_count + closure.capture_count;
    let mut operands = closure
        .bound_object
        .into_iter()
        .map(Some)
        .collect::<Vec<_>>();
    operands.extend(std::iter::repeat_n(
        Some(RegionOperand::I64(0)),
        closure.capture_count,
    ));
    operands.extend(prepared_operands.unwrap_or_else(|| lower_call_operands(unit, args)));
    let mut native_args = args.to_vec();
    mark_prepared_reference_arguments(&mut native_args, target);
    RegionInstructionKind::NativeCall(RegionNativeCall {
        result: RegionCallResult::Register(dst),
        target: RegionCallTarget::Closure {
            callee,
            function: Some(closure.function),
            bound_object_count,
            capture_count: target.captures.len(),
        },
        args: native_args,
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
        Some(IrConstant::Int(value))
            if crate::jit_decode_runtime_value(*value).is_none()
                && crate::jit_decode_constant(*value).is_none() =>
        {
            RegionOperand::I64(*value)
        }
        Some(IrConstant::Int(_)) => RegionOperand::Constant(constant.raw()),
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
