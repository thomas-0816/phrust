use super::super::*;

pub(super) fn defined_registers(kind: &InstructionKind) -> Vec<RegId> {
    match kind {
        InstructionKind::LoadConst { dst, .. }
        | InstructionKind::FetchConst { dst, .. }
        | InstructionKind::Move { dst, .. }
        | InstructionKind::LoadLocal { dst, .. }
        | InstructionKind::LoadLocalQuiet { dst, .. }
        | InstructionKind::Binary { dst, .. }
        | InstructionKind::Compare { dst, .. }
        | InstructionKind::InstanceOf { dst, .. }
        | InstructionKind::DynamicInstanceOf { dst, .. }
        | InstructionKind::Unary { dst, .. }
        | InstructionKind::Cast { dst, .. }
        | InstructionKind::Yield { dst, .. }
        | InstructionKind::YieldFrom { dst, .. }
        | InstructionKind::CallFunction { dst, .. }
        | InstructionKind::CallMethod { dst, .. }
        | InstructionKind::CallStaticMethod { dst, .. }
        | InstructionKind::CloneObject { dst, .. }
        | InstructionKind::CloneWith { dst, .. }
        | InstructionKind::MakeException { dst, .. }
        | InstructionKind::MakeClosure { dst, .. }
        | InstructionKind::CallClosure { dst, .. }
        | InstructionKind::ResolveCallable { dst, .. }
        | InstructionKind::AcquireCallable { dst, .. }
        | InstructionKind::CallCallable { dst, .. }
        | InstructionKind::Pipe { dst, .. }
        | InstructionKind::Include { dst, .. }
        | InstructionKind::Eval { dst, .. }
        | InstructionKind::NewObject { dst, .. }
        | InstructionKind::DynamicNewObject { dst, .. }
        | InstructionKind::FetchProperty { dst, .. }
        | InstructionKind::FetchDynamicProperty { dst, .. }
        | InstructionKind::IssetProperty { dst, .. }
        | InstructionKind::IssetDynamicProperty { dst, .. }
        | InstructionKind::EmptyProperty { dst, .. }
        | InstructionKind::EmptyDynamicProperty { dst, .. }
        | InstructionKind::IssetDynamicPropertyDim { dst, .. }
        | InstructionKind::EmptyDynamicPropertyDim { dst, .. }
        | InstructionKind::IssetPropertyDim { dst, .. }
        | InstructionKind::EmptyPropertyDim { dst, .. }
        | InstructionKind::FetchStaticProperty { dst, .. }
        | InstructionKind::FetchDynamicStaticProperty { dst, .. }
        | InstructionKind::IssetStaticProperty { dst, .. }
        | InstructionKind::EmptyStaticProperty { dst, .. }
        | InstructionKind::IssetStaticPropertyDim { dst, .. }
        | InstructionKind::EmptyStaticPropertyDim { dst, .. }
        | InstructionKind::FetchClassConstant { dst, .. }
        | InstructionKind::FetchObjectClassName { dst, .. }
        | InstructionKind::AssignProperty { dst, .. }
        | InstructionKind::AssignPropertyDim { dst, .. }
        | InstructionKind::AssignDynamicProperty { dst, .. }
        | InstructionKind::AssignStaticProperty { dst, .. }
        | InstructionKind::AssignDynamicStaticProperty { dst, .. }
        | InstructionKind::NewArray { dst }
        | InstructionKind::FetchDim { dst, .. }
        | InstructionKind::AssignDim { dst, .. }
        | InstructionKind::AppendDim { dst, .. }
        | InstructionKind::IssetLocal { dst, .. }
        | InstructionKind::EmptyLocal { dst, .. }
        | InstructionKind::IssetDim { dst, .. }
        | InstructionKind::EmptyDim { dst, .. }
        | InstructionKind::ArrayGet { dst, .. } => vec![*dst],
        InstructionKind::ArrayInsert { array, .. } | InstructionKind::ArraySpread { array, .. } => {
            vec![*array]
        }
        InstructionKind::ForeachInit { iterator, .. }
        | InstructionKind::ForeachInitRef { iterator, .. } => vec![*iterator],
        InstructionKind::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            let mut registers = vec![*has_value, *iterator, *value];
            if let Some(key) = key {
                registers.push(*key);
            }
            registers
        }
        InstructionKind::ForeachNextRef {
            has_value,
            iterator,
            key,
            ..
        } => {
            let mut registers = vec![*has_value, *iterator];
            if let Some(key) = key {
                registers.push(*key);
            }
            registers
        }
        InstructionKind::Nop
        | InstructionKind::DeclareFunction { .. }
        | InstructionKind::DeclareClass { .. }
        | InstructionKind::RegisterConstant { .. }
        | InstructionKind::StoreLocal { .. }
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
        | InstructionKind::BindReferenceFromCall { .. }
        | InstructionKind::BindReferenceFromMethodCall { .. }
        | InstructionKind::InitStaticLocal { .. }
        | InstructionKind::Discard { .. }
        | InstructionKind::Echo { .. }
        | InstructionKind::EmitDiagnostic { .. }
        | InstructionKind::EnterTry { .. }
        | InstructionKind::LeaveTry
        | InstructionKind::EndFinally { .. }
        | InstructionKind::Throw { .. }
        | InstructionKind::UnsetProperty { .. }
        | InstructionKind::UnsetPropertyDim { .. }
        | InstructionKind::UnsetDynamicProperty { .. }
        | InstructionKind::UnsetStaticPropertyDim { .. }
        | InstructionKind::UnsetLocal { .. }
        | InstructionKind::UnsetDim { .. }
        | InstructionKind::ForeachCleanup { .. }
        | InstructionKind::Unsupported { .. }
        | InstructionKind::RuntimeError { .. } => Vec::new(),
    }
}
