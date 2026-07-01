//! IR instruction families.

use crate::ids::{BlockId, ConstId, FunctionId, InstrId, LocalId, RegId};
use crate::operand::Operand;
use crate::source_map::IrSpan;
use serde::{Deserialize, Serialize};

/// Unary operator family.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnaryOp {
    /// Numeric plus.
    Plus,
    /// Numeric negation.
    Minus,
    /// Boolean not.
    Not,
    /// Bitwise not.
    BitNot,
}

/// Binary operator family.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Remainder.
    Mod,
    /// String concatenation.
    Concat,
    /// Exponentiation.
    Pow,
    /// Bitwise and.
    BitAnd,
    /// Bitwise or.
    BitOr,
    /// Bitwise xor.
    BitXor,
    /// Shift left.
    ShiftLeft,
    /// Shift right.
    ShiftRight,
}

/// Comparison operator family.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    /// Loose equality.
    Equal,
    /// Loose inequality.
    NotEqual,
    /// Strict identity.
    Identical,
    /// Strict non-identity.
    NotIdentical,
    /// Less-than comparison.
    Less,
    /// Less-than-or-equal comparison.
    LessEqual,
    /// Greater-than comparison.
    Greater,
    /// Greater-than-or-equal comparison.
    GreaterEqual,
    /// Three-way comparison.
    Spaceship,
}

/// Cast operation family.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CastKind {
    /// Cast to bool.
    Bool,
    /// Cast to int.
    Int,
    /// Cast to float.
    Float,
    /// Cast to string.
    String,
    /// Cast to array.
    Array,
    /// Cast to object.
    Object,
    /// PHP 8.5 `(void)` discard.
    Void,
}

/// One IR instruction with a stable ID and source span.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Instruction {
    /// Instruction ID within its function.
    pub id: InstrId,
    /// Source span that produced this instruction.
    pub span: IrSpan,
    /// Instruction operation.
    pub kind: InstructionKind,
}

/// One operand captured into a closure value.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClosureCaptureArg {
    /// Captured variable name without `$`.
    pub name: String,
    /// Source operand evaluated in the enclosing frame.
    pub src: Operand,
    /// True when the closure captures the source local's reference cell.
    pub by_ref: bool,
}

/// One lowered PHP call argument.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct IrCallArg {
    /// Optional named-argument label without trailing `:`.
    pub name: Option<String>,
    /// Evaluated value operand.
    pub value: Operand,
    /// True when this argument came from `...`.
    pub unpack: bool,
    /// Source value class for PHP by-reference send compatibility.
    pub value_kind: IrCallArgValueKind,
    /// Caller local when this argument can satisfy a by-reference parameter.
    pub by_ref_local: Option<LocalId>,
    /// Caller array dimension when this argument can satisfy a by-reference parameter.
    pub by_ref_dim: Option<IrCallDimTarget>,
    /// Caller property when this argument can satisfy a by-reference parameter.
    pub by_ref_property: Option<IrCallPropertyTarget>,
    /// Caller property array dimension when this argument can satisfy a by-reference parameter.
    pub by_ref_property_dim: Option<IrCallPropertyDimTarget>,
}

/// Source value class for a lowered PHP call argument.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IrCallArgValueKind {
    /// A normal value expression. Non-referenceable by-ref sends are fatal.
    Direct,
    /// A temporary produced by an expression PHP allows with a notice.
    IndirectTemporary,
}

/// Array-dimension lvalue metadata for a call argument.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IrCallDimTarget {
    /// Root caller local.
    pub local: LocalId,
    /// Evaluated dimension operands.
    pub dims: Vec<Operand>,
}

/// Property lvalue metadata for a call argument.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IrCallPropertyTarget {
    /// Evaluated object operand.
    pub object: Operand,
    /// Static property name.
    pub property: String,
}

/// Property array-dimension lvalue metadata for a call argument.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IrCallPropertyDimTarget {
    /// Evaluated object operand.
    pub object: Operand,
    /// Static property name.
    pub property: String,
    /// Evaluated dimension operands.
    pub dims: Vec<Operand>,
}

/// Callable kind encoded in IR before VM resolution/execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallableKind {
    /// Simple unqualified function name. VM resolves user functions before
    /// selected builtins.
    FunctionName { name: String },
    /// Method callable placeholder; execution is a known gap until object
    /// runtime support exists.
    MethodPlaceholder { target: String },
    /// Explicit unresolved/dynamic callable gap.
    UnresolvedDynamic { target: String },
}

/// Include/require operation kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IncludeKind {
    /// `include`.
    Include,
    /// `include_once`.
    IncludeOnce,
    /// `require`.
    Require,
    /// `require_once`.
    RequireOnce,
}

/// Non-fatal PHP diagnostic severity emitted by IR.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IrDiagnosticSeverity {
    /// PHP warning.
    Warning,
    /// PHP deprecation.
    Deprecation,
}

/// Supported IR snapshot instruction set.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "opcode", rename_all = "snake_case")]
pub enum InstructionKind {
    /// No operation.
    Nop,
    /// `dst = constants[constant]`.
    LoadConst { dst: RegId, constant: ConstId },
    /// `dst = global_constant(name)`.
    FetchConst { dst: RegId, name: String },
    /// Registers a global constant at runtime.
    RegisterConstant { name: String, value: Operand },
    /// Registers a function declaration when execution reaches it.
    DeclareFunction { name: String, function: FunctionId },
    /// Registers a class-like declaration when execution reaches it.
    DeclareClass { name: String },
    /// `dst = src`.
    Move { dst: RegId, src: Operand },
    /// `dst = local`.
    LoadLocal { dst: RegId, local: LocalId },
    /// `dst = local` without undefined-variable warning side effects.
    LoadLocalQuiet { dst: RegId, local: LocalId },
    /// `local = src`.
    StoreLocal { local: LocalId, src: Operand },
    /// `target =& source` for local references.
    BindReference { target: LocalId, source: LocalId },
    /// `local =& $GLOBALS[name]` for `global $name`.
    BindGlobal { local: LocalId, name: String },
    /// `local[dims...] =& source` or `local[dims...][] =& source`.
    BindReferenceDim {
        local: LocalId,
        dims: Vec<Operand>,
        append: bool,
        source: LocalId,
    },
    /// `object->property =& source`.
    BindReferenceProperty {
        object: Operand,
        property: String,
        source: LocalId,
    },
    /// `object->property[dims...] =& source` or `object->property[dims...][] =& source`.
    BindReferencePropertyDim {
        object: Operand,
        property: String,
        dims: Vec<Operand>,
        append: bool,
        source: LocalId,
    },
    /// `local[dims...] =& object->property`.
    BindReferenceDimFromProperty {
        local: LocalId,
        dims: Vec<Operand>,
        append: bool,
        object: Operand,
        property: String,
    },
    /// `target =& local[dims...]`.
    BindReferenceFromDim {
        target: LocalId,
        local: LocalId,
        dims: Vec<Operand>,
    },
    /// `target =& class::$property[dims...]`.
    BindReferenceFromStaticPropertyDim {
        target: LocalId,
        class_name: String,
        property: String,
        dims: Vec<Operand>,
    },
    /// `class::$property =& source` for static property references.
    BindReferenceStaticProperty {
        class_name: String,
        property: String,
        source: LocalId,
    },
    /// `target =& function(args...)` for by-reference function returns.
    BindReferenceFromCall {
        target: LocalId,
        name: String,
        args: Vec<IrCallArg>,
    },
    /// `target =& object->method(args...)` for by-reference method returns.
    BindReferenceFromMethodCall {
        target: LocalId,
        object: Operand,
        method: String,
        args: Vec<IrCallArg>,
    },
    /// Binds a local to persistent function-local static storage.
    InitStaticLocal {
        local: LocalId,
        name: String,
        default: Operand,
    },
    /// `dst = op(lhs, rhs)`.
    Binary {
        dst: RegId,
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
    },
    /// `dst = compare(lhs, rhs)`.
    Compare {
        dst: RegId,
        op: CompareOp,
        lhs: Operand,
        rhs: Operand,
    },
    /// `object instanceof class_name`, resolved through VM class/interface metadata.
    InstanceOf {
        dst: RegId,
        object: Operand,
        class_name: String,
    },
    /// `object instanceof target`, where target is a runtime string/object value.
    DynamicInstanceOf {
        dst: RegId,
        object: Operand,
        target: Operand,
    },
    /// `dst = op(src)`.
    Unary {
        dst: RegId,
        op: UnaryOp,
        src: Operand,
    },
    /// `dst = cast(src)`.
    Cast {
        dst: RegId,
        kind: CastKind,
        src: Operand,
    },
    /// Evaluate and discard a value.
    Discard { src: Operand },
    /// Emit a value to stdout.
    Echo { src: Operand },
    /// Emit a non-fatal PHP diagnostic.
    EmitDiagnostic {
        severity: IrDiagnosticSeverity,
        diagnostic_id: String,
        message: String,
        leading_newline: bool,
    },
    /// Suspend a generator with an optional key and yielded value.
    Yield {
        dst: RegId,
        key: Option<Operand>,
        value: Option<Operand>,
    },
    /// Delegate generator iteration to an array or generator source.
    YieldFrom { dst: RegId, source: Operand },
    /// Direct user-function call with positional arguments.
    CallFunction {
        dst: RegId,
        name: String,
        args: Vec<IrCallArg>,
    },
    /// Public instance method call with positional arguments.
    CallMethod {
        dst: RegId,
        object: Operand,
        method: String,
        args: Vec<IrCallArg>,
    },
    /// Public static method call with positional arguments.
    CallStaticMethod {
        dst: RegId,
        class_name: String,
        method: String,
        args: Vec<IrCallArg>,
    },
    /// Shallow-clone an object into a new object identity.
    CloneObject { dst: RegId, object: Operand },
    /// Shallow-clone an object and apply public property replacements.
    CloneWith {
        dst: RegId,
        object: Operand,
        replacements: Operand,
    },
    /// Push an MVP exception handler for a try statement.
    EnterTry {
        catch: Option<BlockId>,
        catch_types: Vec<String>,
        finally: Option<BlockId>,
        after: BlockId,
        exception_local: Option<LocalId>,
    },
    /// Pop the active try handler on normal control-flow.
    LeaveTry,
    /// End a finally block and resume pending throw/return control-flow.
    EndFinally { after: BlockId },
    /// Throw a throwable MVP object.
    Throw { value: Operand },
    /// Build a VM-internal MVP throwable object.
    MakeException {
        dst: RegId,
        class_name: String,
        message: Operand,
    },
    /// Create a closure value from a synthesized closure function and captures.
    MakeClosure {
        dst: RegId,
        function: FunctionId,
        captures: Vec<ClosureCaptureArg>,
    },
    /// Call a closure value through the existing frame machinery.
    CallClosure {
        dst: RegId,
        callee: Operand,
        args: Vec<IrCallArg>,
    },
    /// Resolve a callable descriptor into a runtime callable value.
    ResolveCallable { dst: RegId, callable: CallableKind },
    /// Validate/acquire a runtime value as a first-class callable.
    AcquireCallable { dst: RegId, value: Operand },
    /// Call a runtime callable value through the unified callable path.
    CallCallable {
        dst: RegId,
        callee: Operand,
        args: Vec<IrCallArg>,
    },
    /// PHP 8.5 pipe operator MVP: call `callable(input)`.
    Pipe {
        dst: RegId,
        input: Operand,
        callable: Operand,
    },
    /// Runtime include/require operation. The VM resolves and compiles the
    /// path through the configured local include loader.
    Include {
        dst: RegId,
        kind: IncludeKind,
        path: Operand,
    },
    /// Runtime eval operation. The VM compiles the evaluated code string
    /// through the same frontend and IR pipeline using a synthetic source file.
    Eval { dst: RegId, code: Operand },
    /// Creates a new object and invokes its constructor when one is declared.
    NewObject {
        dst: RegId,
        display_class_name: String,
        class_name: String,
        args: Vec<IrCallArg>,
    },
    /// Creates a new object from a runtime class-name expression.
    DynamicNewObject {
        dst: RegId,
        class_name: Operand,
        args: Vec<IrCallArg>,
    },
    /// Fetches an instance property by static property name.
    FetchProperty {
        dst: RegId,
        object: Operand,
        property: String,
    },
    /// Fetches an instance property by runtime property name.
    FetchDynamicProperty {
        dst: RegId,
        object: Operand,
        property: Operand,
    },
    /// Tests whether an instance property exists and is not null.
    IssetProperty {
        dst: RegId,
        object: Operand,
        property: String,
    },
    /// Tests whether a runtime-named instance property exists and is not null.
    IssetDynamicProperty {
        dst: RegId,
        object: Operand,
        property: Operand,
    },
    /// Tests PHP empty() for an instance property.
    EmptyProperty {
        dst: RegId,
        object: Operand,
        property: String,
    },
    /// Tests PHP empty() for a runtime-named instance property.
    EmptyDynamicProperty {
        dst: RegId,
        object: Operand,
        property: Operand,
    },
    /// Tests whether a runtime-named instance property array dimension exists and is not null.
    IssetDynamicPropertyDim {
        dst: RegId,
        object: Operand,
        property: Operand,
        dims: Vec<Operand>,
    },
    /// Tests whether a runtime-named instance property array dimension is empty.
    EmptyDynamicPropertyDim {
        dst: RegId,
        object: Operand,
        property: Operand,
        dims: Vec<Operand>,
    },
    /// Tests whether an instance property array dimension exists and is not null.
    IssetPropertyDim {
        dst: RegId,
        object: Operand,
        property: String,
        dims: Vec<Operand>,
    },
    /// Tests whether an instance property array dimension is empty.
    EmptyPropertyDim {
        dst: RegId,
        object: Operand,
        property: String,
        dims: Vec<Operand>,
    },
    /// Unsets an instance property slot.
    UnsetProperty { object: Operand, property: String },
    /// Unsets an instance property array dimension.
    UnsetPropertyDim {
        object: Operand,
        property: String,
        dims: Vec<Operand>,
    },
    /// Unsets an instance property slot by runtime property name.
    UnsetDynamicProperty { object: Operand, property: Operand },
    /// Fetches a static property by class and static property name.
    FetchStaticProperty {
        dst: RegId,
        class_name: String,
        property: String,
    },
    /// Tests whether a static property exists and is not null.
    IssetStaticProperty {
        dst: RegId,
        class_name: String,
        property: String,
    },
    /// Tests PHP empty() for a static property.
    EmptyStaticProperty {
        dst: RegId,
        class_name: String,
        property: String,
    },
    /// Tests whether a static property array dimension exists and is not null.
    IssetStaticPropertyDim {
        dst: RegId,
        class_name: String,
        property: String,
        dims: Vec<Operand>,
    },
    /// Tests PHP empty() for a static property array dimension.
    EmptyStaticPropertyDim {
        dst: RegId,
        class_name: String,
        property: String,
        dims: Vec<Operand>,
    },
    /// Unsets a static property array dimension.
    UnsetStaticPropertyDim {
        class_name: String,
        property: String,
        dims: Vec<Operand>,
    },
    /// Fetches a class constant by class and constant name.
    FetchClassConstant {
        dst: RegId,
        class_name: String,
        constant: String,
    },
    /// Fetches the runtime class name for an object `::class` expression.
    FetchObjectClassName { dst: RegId, object: Operand },
    /// Assigns an instance property by static property name.
    AssignProperty {
        dst: RegId,
        object: Operand,
        property: String,
        value: Operand,
    },
    /// Assigns or appends to an instance property array dimension by static property name.
    AssignPropertyDim {
        dst: RegId,
        object: Operand,
        property: String,
        dims: Vec<Operand>,
        value: Operand,
        append: bool,
    },
    /// Assigns an instance property by runtime property name.
    AssignDynamicProperty {
        dst: RegId,
        object: Operand,
        property: Operand,
        value: Operand,
    },
    /// Assigns a static property by class and static property name.
    AssignStaticProperty {
        dst: RegId,
        class_name: String,
        property: String,
        value: Operand,
    },
    /// Creates an empty PHP array.
    NewArray { dst: RegId },
    /// Inserts or appends one element into an array register.
    ArrayInsert {
        array: RegId,
        key: Option<Operand>,
        value: Operand,
        by_ref_local: Option<LocalId>,
    },
    /// Extends an array register with PHP array-unpack/spread semantics.
    ArraySpread { array: RegId, source: Operand },
    /// General array dimension fetch.
    FetchDim {
        dst: RegId,
        array: Operand,
        key: Operand,
        quiet: bool,
    },
    /// Assigns a local array dimension by value.
    AssignDim {
        dst: RegId,
        local: LocalId,
        dims: Vec<Operand>,
        value: Operand,
    },
    /// Appends to a local array dimension by value.
    AppendDim {
        dst: RegId,
        local: LocalId,
        dims: Vec<Operand>,
        value: Operand,
    },
    /// Tests whether a local exists and is not null.
    IssetLocal { dst: RegId, local: LocalId },
    /// Tests whether a local is empty using PHP MVP truthiness.
    EmptyLocal { dst: RegId, local: LocalId },
    /// Unsets a local variable.
    UnsetLocal { local: LocalId },
    /// Tests whether a local array dimension exists and is not null.
    IssetDim {
        dst: RegId,
        local: LocalId,
        dims: Vec<Operand>,
    },
    /// Tests whether a local array dimension is empty.
    EmptyDim {
        dst: RegId,
        local: LocalId,
        dims: Vec<Operand>,
    },
    /// Unsets a local array dimension.
    UnsetDim { local: LocalId, dims: Vec<Operand> },
    /// Creates a by-value foreach snapshot iterator from an array operand.
    ForeachInit { iterator: RegId, source: Operand },
    /// Advances a foreach snapshot iterator.
    ForeachNext {
        has_value: RegId,
        iterator: RegId,
        key: Option<RegId>,
        value: RegId,
    },
    /// Releases foreach iterator state after normal exhaustion or break.
    ForeachCleanup { iterator: RegId },
    /// Creates a by-reference foreach iterator from a local array variable.
    ForeachInitRef { iterator: RegId, local: LocalId },
    /// Advances a by-reference foreach iterator and binds the value local.
    ForeachNextRef {
        has_value: RegId,
        iterator: RegId,
        key: Option<RegId>,
        value_local: LocalId,
    },
    /// Packed-array fetch used by the variadic-argument MVP.
    ArrayGet {
        dst: RegId,
        array: Operand,
        index: Operand,
    },
    /// Explicit unsupported feature marker.
    Unsupported { diagnostic_id: String },
    /// Deterministic internal runtime error for MVP runtime gaps.
    RuntimeError {
        diagnostic_id: String,
        message: String,
    },
}

/// Block terminator with source span.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TerminatorKind {
    /// Unconditional jump.
    Jump { target: BlockId },
    /// Conditional jump when operand is false.
    JumpIfFalse { condition: Operand, target: BlockId },
    /// Conditional jump when operand is true.
    JumpIfTrue { condition: Operand, target: BlockId },
    /// Conditional jump with explicit true and false targets.
    JumpIf {
        condition: Operand,
        if_true: BlockId,
        if_false: BlockId,
    },
    /// Return from the current function.
    Return {
        value: Option<Operand>,
        by_ref_local: Option<LocalId>,
    },
    /// Terminate the current script/request.
    Exit { value: Option<Operand> },
}

/// A terminator plus source span.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Terminator {
    /// Source span that produced this terminator.
    pub span: IrSpan,
    /// Terminator operation.
    pub kind: TerminatorKind,
}
