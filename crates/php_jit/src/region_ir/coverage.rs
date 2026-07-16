//! Exhaustive baseline-lowering classification and generated manifest data.

use crate::JitHelperId;
use php_ir::instruction::{CallableKind, IrCallArgValueKind, TerminatorKind};
use php_ir::{BinaryOp, CastKind, CompareOp, IncludeKind, InstructionKind, UnaryOp};
use php_runtime::api::{
    JIT_HELPER_ECHO_VALUE, JIT_HELPER_SCALAR_BINARY, JIT_HELPER_SCALAR_CAST,
    JIT_HELPER_SCALAR_COMPARE, JIT_HELPER_SCALAR_UNARY,
};

/// Exactly one baseline lowering route for an IR operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BaselineLoweringClass {
    DirectClif,
    TypedRuntimeHelper(JitHelperId),
    NativeControlFlow,
    NativeStateMachine,
    CompileTimeFatal,
}

/// Typed PHP-visible effects used by lowering and safepoint audits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaselineEffectFlags(u16);

impl BaselineEffectFlags {
    pub const NONE: Self = Self(0);
    pub const READS_STATE: Self = Self(1 << 0);
    pub const WRITES_STATE: Self = Self(1 << 1);
    pub const ALLOCATES: Self = Self(1 << 2);
    pub const CONTROL_FLOW: Self = Self(1 << 3);
    pub const DECLARATION: Self = Self(1 << 4);
    pub const IO: Self = Self(1 << 5);

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }
}

/// One generated manifest row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaselineLoweringManifestEntry {
    pub variant: &'static str,
    pub class: BaselineLoweringClass,
    pub effects: BaselineEffectFlags,
    pub may_throw: bool,
    pub may_diagnose: bool,
    pub may_call_user_code: bool,
    pub may_suspend: bool,
    pub requires_safepoint: bool,
}

const PURE: BaselineEffectFlags = BaselineEffectFlags::NONE;
const READ: BaselineEffectFlags = BaselineEffectFlags::READS_STATE;
const WRITE: BaselineEffectFlags = BaselineEffectFlags::WRITES_STATE;
const ALLOCATE: BaselineEffectFlags = BaselineEffectFlags::ALLOCATES;
const CONTROL: BaselineEffectFlags = BaselineEffectFlags::CONTROL_FLOW;
const DECLARE: BaselineEffectFlags = BaselineEffectFlags::DECLARATION;
const IO: BaselineEffectFlags = BaselineEffectFlags::IO;
const READ_WRITE: BaselineEffectFlags = READ.union(WRITE);
const ALLOCATE_WRITE: BaselineEffectFlags = ALLOCATE.union(WRITE);
const CONTROL_WRITE: BaselineEffectFlags = CONTROL.union(WRITE);

const HELPER_UNARY: JitHelperId = JIT_HELPER_SCALAR_UNARY;
const HELPER_BINARY: JitHelperId = JIT_HELPER_SCALAR_BINARY;
const HELPER_COMPARE: JitHelperId = JIT_HELPER_SCALAR_COMPARE;
const HELPER_CAST: JitHelperId = JIT_HELPER_SCALAR_CAST;
const HELPER_ECHO: JitHelperId = JIT_HELPER_ECHO_VALUE;

macro_rules! define_instruction_coverage {
    ($($pattern:pat => ($variant:literal, $class:expr, $effects:expr, $throw:literal, $diagnose:literal, $user:literal, $suspend:literal, $safepoint:literal);)+) => {
        /// Exhaustive authoritative classification. No wildcard is permitted.
        #[must_use]
        pub fn baseline_instruction_lowering(
            instruction: &InstructionKind,
        ) -> BaselineLoweringManifestEntry {
            match instruction {
                $($pattern => BaselineLoweringManifestEntry {
                    variant: $variant,
                    class: $class,
                    effects: $effects,
                    may_throw: $throw,
                    may_diagnose: $diagnose,
                    may_call_user_code: $user,
                    may_suspend: $suspend,
                    requires_safepoint: $safepoint,
                },)+
            }
        }

        /// Manifest generated from the exact same typed variant list.
        pub const BASELINE_INSTRUCTION_MANIFEST: &[BaselineLoweringManifestEntry] = &[
            $(BaselineLoweringManifestEntry {
                variant: $variant,
                class: $class,
                effects: $effects,
                may_throw: $throw,
                may_diagnose: $diagnose,
                may_call_user_code: $user,
                may_suspend: $suspend,
                requires_safepoint: $safepoint,
            },)+
        ];
    };
}

define_instruction_coverage! {
    InstructionKind::Nop => ("Nop", BaselineLoweringClass::DirectClif, PURE, false, false, false, false, false);
    InstructionKind::LoadConst { .. } => ("LoadConst", BaselineLoweringClass::DirectClif, PURE, false, false, false, false, false);
    InstructionKind::FetchConst { .. } => ("FetchConst", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::RegisterConstant { .. } => ("RegisterConstant", BaselineLoweringClass::NativeStateMachine, WRITE.union(DECLARE), true, true, false, false, true);
    InstructionKind::DeclareFunction { .. } => ("DeclareFunction", BaselineLoweringClass::NativeStateMachine, DECLARE, false, true, false, false, true);
    InstructionKind::DeclareClass { .. } => ("DeclareClass", BaselineLoweringClass::NativeStateMachine, DECLARE, true, true, true, false, true);
    InstructionKind::Move { .. } => ("Move", BaselineLoweringClass::DirectClif, PURE, false, false, false, false, false);
    InstructionKind::LoadLocal { .. } => ("LoadLocal", BaselineLoweringClass::NativeStateMachine, READ, false, true, false, false, false);
    InstructionKind::LoadLocalQuiet { .. } => ("LoadLocalQuiet", BaselineLoweringClass::DirectClif, READ, false, false, false, false, false);
    InstructionKind::StoreLocal { .. } => ("StoreLocal", BaselineLoweringClass::DirectClif, WRITE, false, false, false, false, false);
    InstructionKind::BindReference { .. } => ("BindReference", BaselineLoweringClass::NativeStateMachine, READ_WRITE, false, false, false, false, true);
    InstructionKind::BindGlobal { .. } => ("BindGlobal", BaselineLoweringClass::NativeStateMachine, READ_WRITE, false, false, false, false, true);
    InstructionKind::BindReferenceDim { .. } => ("BindReferenceDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceProperty { .. } => ("BindReferenceProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferencePropertyDim { .. } => ("BindReferencePropertyDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceDimFromProperty { .. } => ("BindReferenceDimFromProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromProperty { .. } => ("BindReferenceFromProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromPropertyDim { .. } => ("BindReferenceFromPropertyDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromDim { .. } => ("BindReferenceFromDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromStaticPropertyDim { .. } => ("BindReferenceFromStaticPropertyDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceStaticProperty { .. } => ("BindReferenceStaticProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromCall { .. } => ("BindReferenceFromCall", BaselineLoweringClass::NativeControlFlow, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromMethodCall { .. } => ("BindReferenceFromMethodCall", BaselineLoweringClass::NativeControlFlow, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::InitStaticLocal { .. } => ("InitStaticLocal", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, false, false, true);
    InstructionKind::Binary { .. } => ("Binary", BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY), PURE, true, true, true, false, true);
    InstructionKind::Compare { .. } => ("Compare", BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE), PURE, true, true, true, false, true);
    InstructionKind::InstanceOf { .. } => ("InstanceOf", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::DynamicInstanceOf { .. } => ("DynamicInstanceOf", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::Unary { .. } => ("Unary", BaselineLoweringClass::TypedRuntimeHelper(HELPER_UNARY), PURE, true, true, true, false, true);
    InstructionKind::Cast { .. } => ("Cast", BaselineLoweringClass::TypedRuntimeHelper(HELPER_CAST), ALLOCATE, true, true, true, false, true);
    InstructionKind::Discard { .. } => ("Discard", BaselineLoweringClass::DirectClif, PURE, false, false, false, false, false);
    InstructionKind::Echo { .. } => ("Echo", BaselineLoweringClass::TypedRuntimeHelper(HELPER_ECHO), IO, true, true, true, false, true);
    InstructionKind::EmitDiagnostic { .. } => ("EmitDiagnostic", BaselineLoweringClass::NativeStateMachine, IO, false, true, true, false, true);
    InstructionKind::Yield { .. } => ("Yield", BaselineLoweringClass::NativeStateMachine, CONTROL_WRITE, true, true, true, true, true);
    InstructionKind::YieldFrom { .. } => ("YieldFrom", BaselineLoweringClass::NativeStateMachine, CONTROL_WRITE, true, true, true, true, true);
    InstructionKind::CallFunction { .. } => ("CallFunction", BaselineLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::CallMethod { .. } => ("CallMethod", BaselineLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::CallStaticMethod { .. } => ("CallStaticMethod", BaselineLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::CloneObject { .. } => ("CloneObject", BaselineLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, true, false, true);
    InstructionKind::CloneWith { .. } => ("CloneWith", BaselineLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, true, false, true);
    InstructionKind::EnterTry { .. } => ("EnterTry", BaselineLoweringClass::NativeStateMachine, CONTROL_WRITE, false, false, false, false, true);
    InstructionKind::LeaveTry => ("LeaveTry", BaselineLoweringClass::NativeStateMachine, CONTROL_WRITE, false, false, false, false, true);
    InstructionKind::EndFinally { .. } => ("EndFinally", BaselineLoweringClass::NativeStateMachine, CONTROL_WRITE, true, false, false, false, true);
    InstructionKind::Throw { .. } => ("Throw", BaselineLoweringClass::NativeStateMachine, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::MakeException { .. } => ("MakeException", BaselineLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, false, false, true);
    InstructionKind::MakeClosure { .. } => ("MakeClosure", BaselineLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, false, false, true);
    InstructionKind::CallClosure { .. } => ("CallClosure", BaselineLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::ResolveCallable { .. } => ("ResolveCallable", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::AcquireCallable { .. } => ("AcquireCallable", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::CallCallable { .. } => ("CallCallable", BaselineLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::Pipe { .. } => ("Pipe", BaselineLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::Include { .. } => ("Include", BaselineLoweringClass::NativeControlFlow, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::Eval { .. } => ("Eval", BaselineLoweringClass::NativeControlFlow, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::NewObject { .. } => ("NewObject", BaselineLoweringClass::NativeControlFlow, ALLOCATE_WRITE.union(CONTROL), true, true, true, false, true);
    InstructionKind::DynamicNewObject { .. } => ("DynamicNewObject", BaselineLoweringClass::NativeControlFlow, ALLOCATE_WRITE.union(CONTROL), true, true, true, false, true);
    InstructionKind::FetchProperty { .. } => ("FetchProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::FetchDynamicProperty { .. } => ("FetchDynamicProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetProperty { .. } => ("IssetProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetDynamicProperty { .. } => ("IssetDynamicProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyProperty { .. } => ("EmptyProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyDynamicProperty { .. } => ("EmptyDynamicProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetDynamicPropertyDim { .. } => ("IssetDynamicPropertyDim", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyDynamicPropertyDim { .. } => ("EmptyDynamicPropertyDim", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetPropertyDim { .. } => ("IssetPropertyDim", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyPropertyDim { .. } => ("EmptyPropertyDim", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::UnsetProperty { .. } => ("UnsetProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::UnsetPropertyDim { .. } => ("UnsetPropertyDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::UnsetDynamicProperty { .. } => ("UnsetDynamicProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::FetchStaticProperty { .. } => ("FetchStaticProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::FetchDynamicStaticProperty { .. } => ("FetchDynamicStaticProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetStaticProperty { .. } => ("IssetStaticProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyStaticProperty { .. } => ("EmptyStaticProperty", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetStaticPropertyDim { .. } => ("IssetStaticPropertyDim", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyStaticPropertyDim { .. } => ("EmptyStaticPropertyDim", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::UnsetStaticPropertyDim { .. } => ("UnsetStaticPropertyDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::FetchClassConstant { .. } => ("FetchClassConstant", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::FetchObjectClassName { .. } => ("FetchObjectClassName", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, false);
    InstructionKind::AssignProperty { .. } => ("AssignProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AssignPropertyDim { .. } => ("AssignPropertyDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AssignDynamicProperty { .. } => ("AssignDynamicProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AssignStaticProperty { .. } => ("AssignStaticProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AssignDynamicStaticProperty { .. } => ("AssignDynamicStaticProperty", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::NewArray { .. } => ("NewArray", BaselineLoweringClass::NativeStateMachine, ALLOCATE, true, true, false, false, true);
    InstructionKind::ArrayInsert { .. } => ("ArrayInsert", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::ArraySpread { .. } => ("ArraySpread", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::FetchDim { .. } => ("FetchDim", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::AssignDim { .. } => ("AssignDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AppendDim { .. } => ("AppendDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::IssetLocal { .. } => ("IssetLocal", BaselineLoweringClass::NativeStateMachine, READ, false, false, false, false, false);
    InstructionKind::EmptyLocal { .. } => ("EmptyLocal", BaselineLoweringClass::NativeStateMachine, READ, false, false, false, false, false);
    InstructionKind::UnsetLocal { .. } => ("UnsetLocal", BaselineLoweringClass::NativeStateMachine, WRITE, false, false, false, false, false);
    InstructionKind::IssetDim { .. } => ("IssetDim", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyDim { .. } => ("EmptyDim", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::UnsetDim { .. } => ("UnsetDim", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::ForeachInit { .. } => ("ForeachInit", BaselineLoweringClass::NativeStateMachine, ALLOCATE.union(READ), true, true, true, false, true);
    InstructionKind::ForeachNext { .. } => ("ForeachNext", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::ForeachCleanup { .. } => ("ForeachCleanup", BaselineLoweringClass::NativeStateMachine, WRITE, false, false, false, false, true);
    InstructionKind::ForeachInitRef { .. } => ("ForeachInitRef", BaselineLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, true, false, true);
    InstructionKind::ForeachNextRef { .. } => ("ForeachNextRef", BaselineLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::ArrayGet { .. } => ("ArrayGet", BaselineLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::RuntimeError { .. } => ("RuntimeError", BaselineLoweringClass::CompileTimeFatal, CONTROL, false, true, false, false, true);
}

macro_rules! define_terminator_coverage {
    ($($pattern:pat => ($variant:literal, $class:expr, $effects:expr, $throw:literal, $diagnose:literal, $user:literal, $suspend:literal, $safepoint:literal);)+) => {
        #[must_use]
        pub fn baseline_terminator_lowering(
            terminator: &TerminatorKind,
        ) -> BaselineLoweringManifestEntry {
            match terminator {
                $($pattern => BaselineLoweringManifestEntry {
                    variant: $variant,
                    class: $class,
                    effects: $effects,
                    may_throw: $throw,
                    may_diagnose: $diagnose,
                    may_call_user_code: $user,
                    may_suspend: $suspend,
                    requires_safepoint: $safepoint,
                },)+
            }
        }

        pub const BASELINE_TERMINATOR_MANIFEST: &[BaselineLoweringManifestEntry] = &[
            $(BaselineLoweringManifestEntry {
                variant: $variant,
                class: $class,
                effects: $effects,
                may_throw: $throw,
                may_diagnose: $diagnose,
                may_call_user_code: $user,
                may_suspend: $suspend,
                requires_safepoint: $safepoint,
            },)+
        ];
    };
}

define_terminator_coverage! {
    TerminatorKind::Jump { .. } => ("Jump", BaselineLoweringClass::NativeControlFlow, CONTROL, false, false, false, false, false);
    TerminatorKind::JumpIfFalse { .. } => ("JumpIfFalse", BaselineLoweringClass::NativeControlFlow, CONTROL, false, false, false, false, false);
    TerminatorKind::JumpIfTrue { .. } => ("JumpIfTrue", BaselineLoweringClass::NativeControlFlow, CONTROL, false, false, false, false, false);
    TerminatorKind::JumpIf { .. } => ("JumpIf", BaselineLoweringClass::NativeControlFlow, CONTROL, false, false, false, false, false);
    TerminatorKind::Return { .. } => ("Return", BaselineLoweringClass::NativeControlFlow, CONTROL, true, true, false, false, true);
    TerminatorKind::Exit { .. } => ("Exit", BaselineLoweringClass::NativeControlFlow, CONTROL_WRITE, false, true, false, false, true);
}

#[must_use]
pub const fn baseline_unary_class(op: UnaryOp) -> BaselineLoweringClass {
    match op {
        UnaryOp::Plus => BaselineLoweringClass::TypedRuntimeHelper(HELPER_UNARY),
        UnaryOp::Minus => BaselineLoweringClass::TypedRuntimeHelper(HELPER_UNARY),
        UnaryOp::Not => BaselineLoweringClass::TypedRuntimeHelper(HELPER_UNARY),
        UnaryOp::BitNot => BaselineLoweringClass::TypedRuntimeHelper(HELPER_UNARY),
    }
}

#[must_use]
pub const fn baseline_binary_class(op: BinaryOp) -> BaselineLoweringClass {
    match op {
        BinaryOp::Add => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Sub => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Mul => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Div => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Mod => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Concat => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Pow => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::BitAnd => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::BitOr => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::BitXor => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::ShiftLeft => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::ShiftRight => BaselineLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
    }
}

#[must_use]
pub const fn baseline_compare_class(op: CompareOp) -> BaselineLoweringClass {
    match op {
        CompareOp::Equal => BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::NotEqual => BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::Identical => BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::NotIdentical => BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::Less => BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::LessEqual => BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::Greater => BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::GreaterEqual => BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::Spaceship => BaselineLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
    }
}

#[must_use]
pub const fn baseline_cast_class(kind: CastKind) -> BaselineLoweringClass {
    match kind {
        CastKind::Bool => BaselineLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Int => BaselineLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Float => BaselineLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::String => BaselineLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Array => BaselineLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Object => BaselineLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Void => BaselineLoweringClass::TypedRuntimeHelper(HELPER_CAST),
    }
}

#[must_use]
pub const fn baseline_include_class(kind: IncludeKind) -> BaselineLoweringClass {
    match kind {
        IncludeKind::Include => BaselineLoweringClass::NativeControlFlow,
        IncludeKind::IncludeOnce => BaselineLoweringClass::NativeControlFlow,
        IncludeKind::Require => BaselineLoweringClass::NativeControlFlow,
        IncludeKind::RequireOnce => BaselineLoweringClass::NativeControlFlow,
    }
}

#[must_use]
pub fn baseline_callable_class(kind: &CallableKind) -> BaselineLoweringClass {
    match kind {
        CallableKind::FunctionName { .. } => BaselineLoweringClass::NativeControlFlow,
        CallableKind::MethodPlaceholder { .. } => BaselineLoweringClass::NativeControlFlow,
        CallableKind::UnresolvedDynamic { .. } => BaselineLoweringClass::NativeControlFlow,
    }
}

#[must_use]
pub const fn baseline_call_arg_class(kind: IrCallArgValueKind) -> BaselineLoweringClass {
    match kind {
        IrCallArgValueKind::Direct => BaselineLoweringClass::NativeControlFlow,
        IrCallArgValueKind::IndirectTemporary => BaselineLoweringClass::NativeControlFlow,
        IrCallArgValueKind::ByRefLocationPlaceholder => BaselineLoweringClass::NativeControlFlow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::api::{NATIVE_OPERATION_REGISTRY, lookup_native_operation};

    #[test]
    fn manifest_has_every_current_instruction_and_terminator() {
        assert_eq!(BASELINE_INSTRUCTION_MANIFEST.len(), 101);
        assert_eq!(BASELINE_TERMINATOR_MANIFEST.len(), 6);
        assert_eq!(
            BASELINE_INSTRUCTION_MANIFEST
                .iter()
                .filter(|entry| entry.variant == "RuntimeError")
                .count(),
            1
        );
    }

    #[test]
    fn every_helper_mapped_instruction_has_a_real_typed_runtime_operation() {
        let mapped = BASELINE_INSTRUCTION_MANIFEST
            .iter()
            .filter_map(|entry| match entry.class {
                BaselineLoweringClass::TypedRuntimeHelper(id) => Some(id),
                BaselineLoweringClass::DirectClif
                | BaselineLoweringClass::NativeControlFlow
                | BaselineLoweringClass::NativeStateMachine
                | BaselineLoweringClass::CompileTimeFatal => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(mapped.len(), 5);
        for id in mapped {
            let operation = lookup_native_operation(id).expect("registered runtime operation");
            assert!(operation.native_callable);
            assert!(operation.gc_safepoint);
            assert!(operation.native_callers.contains(&"baseline"));
            assert!(operation.native_callers.contains(&"optimizing"));
        }
        assert_eq!(
            NATIVE_OPERATION_REGISTRY
                .iter()
                .filter(|operation| operation.native_callable)
                .count(),
            5
        );
    }
}
