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
pub enum GenericLoweringClass {
    DirectClif,
    TypedRuntimeHelper(JitHelperId),
    NativeControlFlow,
    NativeStateMachine,
    CompileTimeFatal,
}

/// Typed PHP-visible effects used by lowering and safepoint audits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericEffectFlags(u16);

impl GenericEffectFlags {
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
pub struct GenericLoweringManifestEntry {
    pub variant: &'static str,
    pub class: GenericLoweringClass,
    pub effects: GenericEffectFlags,
    pub may_throw: bool,
    pub may_diagnose: bool,
    pub may_call_user_code: bool,
    pub may_suspend: bool,
    pub requires_safepoint: bool,
}

const PURE: GenericEffectFlags = GenericEffectFlags::NONE;
const READ: GenericEffectFlags = GenericEffectFlags::READS_STATE;
const WRITE: GenericEffectFlags = GenericEffectFlags::WRITES_STATE;
const ALLOCATE: GenericEffectFlags = GenericEffectFlags::ALLOCATES;
const CONTROL: GenericEffectFlags = GenericEffectFlags::CONTROL_FLOW;
const DECLARE: GenericEffectFlags = GenericEffectFlags::DECLARATION;
const IO: GenericEffectFlags = GenericEffectFlags::IO;
const READ_WRITE: GenericEffectFlags = READ.union(WRITE);
const ALLOCATE_WRITE: GenericEffectFlags = ALLOCATE.union(WRITE);
const CONTROL_WRITE: GenericEffectFlags = CONTROL.union(WRITE);

const HELPER_UNARY: JitHelperId = JIT_HELPER_SCALAR_UNARY;
const HELPER_BINARY: JitHelperId = JIT_HELPER_SCALAR_BINARY;
const HELPER_COMPARE: JitHelperId = JIT_HELPER_SCALAR_COMPARE;
const HELPER_CAST: JitHelperId = JIT_HELPER_SCALAR_CAST;
const HELPER_ECHO: JitHelperId = JIT_HELPER_ECHO_VALUE;

macro_rules! define_instruction_coverage {
    ($($pattern:pat => ($variant:literal, $class:expr, $effects:expr, $throw:literal, $diagnose:literal, $user:literal, $suspend:literal, $safepoint:literal);)+) => {
        /// Exhaustive authoritative classification. No wildcard is permitted.
        #[must_use]
        pub fn generic_instruction_lowering(
            instruction: &InstructionKind,
        ) -> GenericLoweringManifestEntry {
            match instruction {
                $($pattern => GenericLoweringManifestEntry {
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
        pub const GENERIC_INSTRUCTION_MANIFEST: &[GenericLoweringManifestEntry] = &[
            $(GenericLoweringManifestEntry {
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
    InstructionKind::Nop => ("Nop", GenericLoweringClass::DirectClif, PURE, false, false, false, false, false);
    InstructionKind::LoadConst { .. } => ("LoadConst", GenericLoweringClass::DirectClif, PURE, false, false, false, false, false);
    InstructionKind::FetchConst { .. } => ("FetchConst", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::RegisterConstant { .. } => ("RegisterConstant", GenericLoweringClass::NativeStateMachine, WRITE.union(DECLARE), true, true, false, false, true);
    InstructionKind::DeclareFunction { .. } => ("DeclareFunction", GenericLoweringClass::NativeStateMachine, DECLARE, false, true, false, false, true);
    InstructionKind::DeclareClass { .. } => ("DeclareClass", GenericLoweringClass::NativeStateMachine, DECLARE, true, true, true, false, true);
    InstructionKind::Move { .. } => ("Move", GenericLoweringClass::DirectClif, PURE, false, false, false, false, false);
    InstructionKind::LoadLocal { .. } => ("LoadLocal", GenericLoweringClass::NativeStateMachine, READ, false, true, false, false, false);
    InstructionKind::LoadLocalQuiet { .. } => ("LoadLocalQuiet", GenericLoweringClass::DirectClif, READ, false, false, false, false, false);
    InstructionKind::StoreLocal { .. } => ("StoreLocal", GenericLoweringClass::DirectClif, WRITE, false, false, false, false, false);
    InstructionKind::BindReference { .. } => ("BindReference", GenericLoweringClass::NativeStateMachine, READ_WRITE, false, false, false, false, true);
    InstructionKind::BindGlobal { .. } => ("BindGlobal", GenericLoweringClass::NativeStateMachine, READ_WRITE, false, false, false, false, true);
    InstructionKind::BindReferenceDim { .. } => ("BindReferenceDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceProperty { .. } => ("BindReferenceProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferencePropertyDim { .. } => ("BindReferencePropertyDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceDimFromProperty { .. } => ("BindReferenceDimFromProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromProperty { .. } => ("BindReferenceFromProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromPropertyDim { .. } => ("BindReferenceFromPropertyDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromDim { .. } => ("BindReferenceFromDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromStaticPropertyDim { .. } => ("BindReferenceFromStaticPropertyDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceStaticProperty { .. } => ("BindReferenceStaticProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromCall { .. } => ("BindReferenceFromCall", GenericLoweringClass::NativeControlFlow, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::BindReferenceFromMethodCall { .. } => ("BindReferenceFromMethodCall", GenericLoweringClass::NativeControlFlow, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::InitStaticLocal { .. } => ("InitStaticLocal", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, false, false, true);
    InstructionKind::Binary { .. } => ("Binary", GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY), PURE, true, true, true, false, true);
    InstructionKind::Compare { .. } => ("Compare", GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE), PURE, true, true, true, false, true);
    InstructionKind::InstanceOf { .. } => ("InstanceOf", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::DynamicInstanceOf { .. } => ("DynamicInstanceOf", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::Unary { .. } => ("Unary", GenericLoweringClass::TypedRuntimeHelper(HELPER_UNARY), PURE, true, true, true, false, true);
    InstructionKind::Cast { .. } => ("Cast", GenericLoweringClass::TypedRuntimeHelper(HELPER_CAST), ALLOCATE, true, true, true, false, true);
    InstructionKind::Discard { .. } => ("Discard", GenericLoweringClass::DirectClif, PURE, false, false, false, false, false);
    InstructionKind::Echo { .. } => ("Echo", GenericLoweringClass::TypedRuntimeHelper(HELPER_ECHO), IO, true, true, true, false, true);
    InstructionKind::EmitDiagnostic { .. } => ("EmitDiagnostic", GenericLoweringClass::NativeStateMachine, IO, false, true, true, false, true);
    InstructionKind::Yield { .. } => ("Yield", GenericLoweringClass::NativeStateMachine, CONTROL_WRITE, true, true, true, true, true);
    InstructionKind::YieldFrom { .. } => ("YieldFrom", GenericLoweringClass::NativeStateMachine, CONTROL_WRITE, true, true, true, true, true);
    InstructionKind::CallFunction { .. } => ("CallFunction", GenericLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::CallMethod { .. } => ("CallMethod", GenericLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::CallStaticMethod { .. } => ("CallStaticMethod", GenericLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::CloneObject { .. } => ("CloneObject", GenericLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, true, false, true);
    InstructionKind::CloneWith { .. } => ("CloneWith", GenericLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, true, false, true);
    InstructionKind::EnterTry { .. } => ("EnterTry", GenericLoweringClass::NativeStateMachine, CONTROL_WRITE, false, false, false, false, true);
    InstructionKind::LeaveTry => ("LeaveTry", GenericLoweringClass::NativeStateMachine, CONTROL_WRITE, false, false, false, false, true);
    InstructionKind::EndFinally { .. } => ("EndFinally", GenericLoweringClass::NativeStateMachine, CONTROL_WRITE, true, false, false, false, true);
    InstructionKind::Throw { .. } => ("Throw", GenericLoweringClass::NativeStateMachine, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::MakeException { .. } => ("MakeException", GenericLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, false, false, true);
    InstructionKind::MakeClosure { .. } => ("MakeClosure", GenericLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, false, false, true);
    InstructionKind::CallClosure { .. } => ("CallClosure", GenericLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::ResolveCallable { .. } => ("ResolveCallable", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::AcquireCallable { .. } => ("AcquireCallable", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::CallCallable { .. } => ("CallCallable", GenericLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::Pipe { .. } => ("Pipe", GenericLoweringClass::NativeControlFlow, CONTROL, true, true, true, false, true);
    InstructionKind::Include { .. } => ("Include", GenericLoweringClass::NativeControlFlow, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::Eval { .. } => ("Eval", GenericLoweringClass::NativeControlFlow, CONTROL_WRITE, true, true, true, false, true);
    InstructionKind::NewObject { .. } => ("NewObject", GenericLoweringClass::NativeControlFlow, ALLOCATE_WRITE.union(CONTROL), true, true, true, false, true);
    InstructionKind::DynamicNewObject { .. } => ("DynamicNewObject", GenericLoweringClass::NativeControlFlow, ALLOCATE_WRITE.union(CONTROL), true, true, true, false, true);
    InstructionKind::FetchProperty { .. } => ("FetchProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::FetchDynamicProperty { .. } => ("FetchDynamicProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetProperty { .. } => ("IssetProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetDynamicProperty { .. } => ("IssetDynamicProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyProperty { .. } => ("EmptyProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyDynamicProperty { .. } => ("EmptyDynamicProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetDynamicPropertyDim { .. } => ("IssetDynamicPropertyDim", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyDynamicPropertyDim { .. } => ("EmptyDynamicPropertyDim", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetPropertyDim { .. } => ("IssetPropertyDim", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyPropertyDim { .. } => ("EmptyPropertyDim", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::UnsetProperty { .. } => ("UnsetProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::UnsetPropertyDim { .. } => ("UnsetPropertyDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::UnsetDynamicProperty { .. } => ("UnsetDynamicProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::UnsetDynamicPropertyDim { .. } => ("UnsetDynamicPropertyDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::FetchStaticProperty { .. } => ("FetchStaticProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::FetchDynamicStaticProperty { .. } => ("FetchDynamicStaticProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetStaticProperty { .. } => ("IssetStaticProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyStaticProperty { .. } => ("EmptyStaticProperty", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::IssetStaticPropertyDim { .. } => ("IssetStaticPropertyDim", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyStaticPropertyDim { .. } => ("EmptyStaticPropertyDim", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::UnsetStaticPropertyDim { .. } => ("UnsetStaticPropertyDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::FetchClassConstant { .. } => ("FetchClassConstant", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::FetchObjectClassName { .. } => ("FetchObjectClassName", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, false);
    InstructionKind::AssignProperty { .. } => ("AssignProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AssignPropertyDim { .. } => ("AssignPropertyDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AssignDynamicProperty { .. } => ("AssignDynamicProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AssignDynamicPropertyDim { .. } => ("AssignDynamicPropertyDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AssignStaticProperty { .. } => ("AssignStaticProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AssignDynamicStaticProperty { .. } => ("AssignDynamicStaticProperty", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::NewArray { .. } => ("NewArray", GenericLoweringClass::NativeStateMachine, ALLOCATE, true, true, false, false, true);
    InstructionKind::ArrayInsert { .. } => ("ArrayInsert", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::ArraySpread { .. } => ("ArraySpread", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::FetchDim { .. } => ("FetchDim", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::AssignDim { .. } => ("AssignDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::AppendDim { .. } => ("AppendDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::IssetLocal { .. } => ("IssetLocal", GenericLoweringClass::NativeStateMachine, READ, false, false, false, false, false);
    InstructionKind::EmptyLocal { .. } => ("EmptyLocal", GenericLoweringClass::NativeStateMachine, READ, false, false, false, false, false);
    InstructionKind::UnsetLocal { .. } => ("UnsetLocal", GenericLoweringClass::NativeStateMachine, WRITE, false, false, false, false, false);
    InstructionKind::IssetDim { .. } => ("IssetDim", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::EmptyDim { .. } => ("EmptyDim", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::UnsetDim { .. } => ("UnsetDim", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::ForeachInit { .. } => ("ForeachInit", GenericLoweringClass::NativeStateMachine, ALLOCATE.union(READ), true, true, true, false, true);
    InstructionKind::ForeachNext { .. } => ("ForeachNext", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::ForeachCleanup { .. } => ("ForeachCleanup", GenericLoweringClass::NativeStateMachine, WRITE, false, false, false, false, true);
    InstructionKind::ForeachInitRef { .. } => ("ForeachInitRef", GenericLoweringClass::NativeStateMachine, ALLOCATE_WRITE, true, true, true, false, true);
    InstructionKind::ForeachNextRef { .. } => ("ForeachNextRef", GenericLoweringClass::NativeStateMachine, READ_WRITE, true, true, true, false, true);
    InstructionKind::ArrayGet { .. } => ("ArrayGet", GenericLoweringClass::NativeStateMachine, READ, true, true, true, false, true);
    InstructionKind::RuntimeError { .. } => ("RuntimeError", GenericLoweringClass::CompileTimeFatal, CONTROL, false, true, false, false, true);
}

macro_rules! define_terminator_coverage {
    ($($pattern:pat => ($variant:literal, $class:expr, $effects:expr, $throw:literal, $diagnose:literal, $user:literal, $suspend:literal, $safepoint:literal);)+) => {
        #[must_use]
        pub fn generic_terminator_lowering(
            terminator: &TerminatorKind,
        ) -> GenericLoweringManifestEntry {
            match terminator {
                $($pattern => GenericLoweringManifestEntry {
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

        pub const GENERIC_TERMINATOR_MANIFEST: &[GenericLoweringManifestEntry] = &[
            $(GenericLoweringManifestEntry {
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
    TerminatorKind::Jump { .. } => ("Jump", GenericLoweringClass::NativeControlFlow, CONTROL, false, false, false, false, false);
    TerminatorKind::JumpIfFalse { .. } => ("JumpIfFalse", GenericLoweringClass::NativeControlFlow, CONTROL, false, false, false, false, false);
    TerminatorKind::JumpIfTrue { .. } => ("JumpIfTrue", GenericLoweringClass::NativeControlFlow, CONTROL, false, false, false, false, false);
    TerminatorKind::JumpIf { .. } => ("JumpIf", GenericLoweringClass::NativeControlFlow, CONTROL, false, false, false, false, false);
    TerminatorKind::Return { .. } => ("Return", GenericLoweringClass::NativeControlFlow, CONTROL, true, true, false, false, true);
    TerminatorKind::Exit { .. } => ("Exit", GenericLoweringClass::NativeControlFlow, CONTROL_WRITE, false, true, false, false, true);
}

#[must_use]
pub const fn baseline_unary_class(op: UnaryOp) -> GenericLoweringClass {
    match op {
        UnaryOp::Plus => GenericLoweringClass::TypedRuntimeHelper(HELPER_UNARY),
        UnaryOp::Minus => GenericLoweringClass::TypedRuntimeHelper(HELPER_UNARY),
        UnaryOp::Not => GenericLoweringClass::TypedRuntimeHelper(HELPER_UNARY),
        UnaryOp::BitNot => GenericLoweringClass::TypedRuntimeHelper(HELPER_UNARY),
    }
}

#[must_use]
pub const fn baseline_binary_class(op: BinaryOp) -> GenericLoweringClass {
    match op {
        BinaryOp::Add => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Sub => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Mul => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Div => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Mod => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Concat => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::Pow => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::BitAnd => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::BitOr => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::BitXor => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::ShiftLeft => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
        BinaryOp::ShiftRight => GenericLoweringClass::TypedRuntimeHelper(HELPER_BINARY),
    }
}

#[must_use]
pub const fn baseline_compare_class(op: CompareOp) -> GenericLoweringClass {
    match op {
        CompareOp::Equal => GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::NotEqual => GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::Identical => GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::NotIdentical => GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::Less => GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::LessEqual => GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::Greater => GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::GreaterEqual => GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
        CompareOp::Spaceship => GenericLoweringClass::TypedRuntimeHelper(HELPER_COMPARE),
    }
}

#[must_use]
pub const fn baseline_cast_class(kind: CastKind) -> GenericLoweringClass {
    match kind {
        CastKind::Bool => GenericLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Int => GenericLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Float => GenericLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::String => GenericLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Array => GenericLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Object => GenericLoweringClass::TypedRuntimeHelper(HELPER_CAST),
        CastKind::Void => GenericLoweringClass::TypedRuntimeHelper(HELPER_CAST),
    }
}

#[must_use]
pub const fn baseline_include_class(kind: IncludeKind) -> GenericLoweringClass {
    match kind {
        IncludeKind::Include => GenericLoweringClass::NativeControlFlow,
        IncludeKind::IncludeOnce => GenericLoweringClass::NativeControlFlow,
        IncludeKind::Require => GenericLoweringClass::NativeControlFlow,
        IncludeKind::RequireOnce => GenericLoweringClass::NativeControlFlow,
    }
}

#[must_use]
pub fn baseline_callable_class(kind: &CallableKind) -> GenericLoweringClass {
    match kind {
        CallableKind::FunctionName { .. } => GenericLoweringClass::NativeControlFlow,
        CallableKind::MethodPlaceholder { .. } => GenericLoweringClass::NativeControlFlow,
        CallableKind::UnresolvedDynamic { .. } => GenericLoweringClass::NativeControlFlow,
    }
}

#[must_use]
pub const fn baseline_call_arg_class(kind: IrCallArgValueKind) -> GenericLoweringClass {
    match kind {
        IrCallArgValueKind::Direct => GenericLoweringClass::NativeControlFlow,
        IrCallArgValueKind::IndirectTemporary => GenericLoweringClass::NativeControlFlow,
        IrCallArgValueKind::ByRefLocationPlaceholder => GenericLoweringClass::NativeControlFlow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::api::{NATIVE_OPERATION_REGISTRY, lookup_native_operation};

    #[test]
    fn manifest_has_every_current_instruction_and_terminator() {
        assert_eq!(GENERIC_INSTRUCTION_MANIFEST.len(), 103);
        assert_eq!(GENERIC_TERMINATOR_MANIFEST.len(), 6);
        assert_eq!(
            GENERIC_INSTRUCTION_MANIFEST
                .iter()
                .filter(|entry| entry.variant == "RuntimeError")
                .count(),
            1
        );
    }

    #[test]
    fn every_helper_mapped_instruction_has_a_real_typed_runtime_operation() {
        let mapped = GENERIC_INSTRUCTION_MANIFEST
            .iter()
            .filter_map(|entry| match entry.class {
                GenericLoweringClass::TypedRuntimeHelper(id) => Some(id),
                GenericLoweringClass::DirectClif
                | GenericLoweringClass::NativeControlFlow
                | GenericLoweringClass::NativeStateMachine
                | GenericLoweringClass::CompileTimeFatal => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(mapped.len(), 5);
        for id in mapped {
            let operation = lookup_native_operation(id).expect("registered runtime operation");
            assert!(operation.native_callable);
            assert!(operation.gc_safepoint);
            assert!(operation.native_callers.contains(&"baseline"));
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
