use super::super::*;

/// Deduplicates literal pool entries and remaps all constant IDs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiteralCompactionPass;

impl OptimizerPass for LiteralCompactionPass {
    fn name(&self) -> &'static str {
        "literal_compaction"
    }

    fn phase(&self) -> PassPhase {
        PassPhase::PreVerify
    }

    fn run(
        &self,
        transaction: &mut PassTransaction<'_>,
        _context: &PassContext,
    ) -> Result<PassReport, PassError> {
        let mut constants = Vec::<IrConstant>::new();
        let mut remap = Vec::<ConstId>::with_capacity(transaction.unit().constants.len());
        let mut stats = LiteralCompactionStats::default();

        for constant in &transaction.unit().constants {
            stats.constants_seen += 1;
            if let Some(index) = constants
                .iter()
                .position(|candidate| candidate == constant)
                .and_then(|index| u32::try_from(index).ok())
            {
                remap.push(ConstId::new(index));
                stats.duplicates_removed += 1;
            } else if let Ok(index) = u32::try_from(constants.len()) {
                remap.push(ConstId::new(index));
                constants.push(constant.clone());
            } else {
                remap.push(ConstId::new(u32::MAX));
                stats.skipped_index_overflow += 1;
            }
        }

        if stats.duplicates_removed > 0 && stats.skipped_index_overflow == 0 {
            *transaction.constants_mut() = constants;
            remap_unit_constants(transaction, &remap);
        }

        Ok(PassReport {
            name: self.name(),
            phase: self.phase(),
            enabled: true,
            changed: stats.duplicates_removed > 0 && stats.skipped_index_overflow == 0,
            source_spans_preserved: true,
            rolled_back: false,
            scope: PassScopeReport::default(),
            stats: stats.into_report_stats(),
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct LiteralCompactionStats {
    constants_seen: u64,
    duplicates_removed: u64,
    skipped_index_overflow: u64,
}

impl LiteralCompactionStats {
    fn into_report_stats(self) -> BTreeMap<&'static str, u64> {
        BTreeMap::from([
            ("constants_seen", self.constants_seen),
            ("duplicates_removed", self.duplicates_removed),
            ("skipped_index_overflow", self.skipped_index_overflow),
            ("transformations_attempted", self.constants_seen),
            ("transformations_applied", self.duplicates_removed),
            ("transformations_skipped", self.skipped_index_overflow),
        ])
    }
}

fn remap_unit_constants(transaction: &mut PassTransaction<'_>, remap: &[ConstId]) {
    for function_index in 0..transaction.unit().functions.len() {
        let function = transaction.function_mut(function_index);
        for attribute in &mut function.attributes {
            remap_attribute_constants(attribute, remap);
        }
        for param in &mut function.params {
            for attribute in &mut param.attributes {
                remap_attribute_constants(attribute, remap);
            }
        }
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                remap_instruction_constants(&mut instruction.kind, remap);
            }
            if let Some(terminator) = &mut block.terminator {
                remap_terminator_constants(&mut terminator.kind, remap);
            }
        }
        let block_count = function.blocks.len();
        for block_index in 0..block_count {
            transaction.touch_block(function_index, block_index);
        }
    }
    for class in transaction.classes_mut() {
        for attribute in &mut class.attributes {
            remap_attribute_constants(attribute, remap);
        }
        for method in &mut class.methods {
            for attribute in &mut method.attributes {
                remap_attribute_constants(attribute, remap);
            }
        }
        for property in &mut class.properties {
            remap_optional_const(&mut property.default, remap);
            for attribute in &mut property.attributes {
                remap_attribute_constants(attribute, remap);
            }
        }
        for constant in &mut class.constants {
            remap_optional_const(&mut constant.value, remap);
            for attribute in &mut constant.attributes {
                remap_attribute_constants(attribute, remap);
            }
        }
        for case in &mut class.enum_cases {
            remap_optional_const(&mut case.value, remap);
            for attribute in &mut case.attributes {
                remap_attribute_constants(attribute, remap);
            }
        }
    }
    for constant in transaction.constant_table_mut() {
        constant.value = remapped_const(constant.value, remap);
    }
}

fn remap_optional_const(value: &mut Option<ConstId>, remap: &[ConstId]) {
    if let Some(constant) = value {
        *constant = remapped_const(*constant, remap);
    }
}

fn remap_attribute_constants(attribute: &mut php_ir::AttributeEntry, remap: &[ConstId]) {
    for argument in &mut attribute.arguments {
        *argument = remapped_const(*argument, remap);
    }
}

fn remapped_const(value: ConstId, remap: &[ConstId]) -> ConstId {
    remap.get(value.index()).copied().unwrap_or(value)
}

fn remap_operand_constants(operand: &mut Operand, remap: &[ConstId]) {
    if let Operand::Constant(constant) = operand {
        *constant = remapped_const(*constant, remap);
    }
}

fn remap_optional_operand_constants(operand: &mut Option<Operand>, remap: &[ConstId]) {
    if let Some(operand) = operand {
        remap_operand_constants(operand, remap);
    }
}

fn remap_operands_constants(operands: &mut [Operand], remap: &[ConstId]) {
    for operand in operands {
        remap_operand_constants(operand, remap);
    }
}

fn remap_call_args_constants(args: &mut [php_ir::instruction::IrCallArg], remap: &[ConstId]) {
    for arg in args {
        remap_operand_constants(&mut arg.value, remap);
        if let Some(dim) = &mut arg.by_ref_dim {
            remap_operands_constants(&mut dim.dims, remap);
        }
        if let Some(property) = &mut arg.by_ref_property {
            remap_operand_constants(&mut property.object, remap);
        }
        if let Some(property_dim) = &mut arg.by_ref_property_dim {
            remap_operand_constants(&mut property_dim.object, remap);
            remap_operands_constants(&mut property_dim.dims, remap);
        }
    }
}

fn remap_instruction_constants(kind: &mut InstructionKind, remap: &[ConstId]) {
    match kind {
        InstructionKind::LoadConst { constant, .. } => {
            *constant = remapped_const(*constant, remap);
        }
        InstructionKind::Move { src, .. }
        | InstructionKind::RegisterConstant { value: src, .. }
        | InstructionKind::StoreLocal { src, .. }
        | InstructionKind::InitStaticLocal { default: src, .. }
        | InstructionKind::Discard { src }
        | InstructionKind::Echo { src }
        | InstructionKind::YieldFrom { source: src, .. }
        | InstructionKind::Throw { value: src }
        | InstructionKind::Include { path: src, .. }
        | InstructionKind::Eval { code: src, .. }
        | InstructionKind::DynamicNewObject {
            class_name: src, ..
        }
        | InstructionKind::UnsetProperty { object: src, .. }
        | InstructionKind::AcquireCallable { value: src, .. }
        | InstructionKind::ForeachInit { source: src, .. } => remap_operand_constants(src, remap),
        InstructionKind::UnsetPropertyDim { object, dims, .. } => {
            remap_operand_constants(object, remap);
            remap_operands_constants(dims, remap);
        }
        InstructionKind::UnsetDynamicProperty { object, property } => {
            remap_operand_constants(object, remap);
            remap_operand_constants(property, remap);
        }
        InstructionKind::Binary { lhs, rhs, .. }
        | InstructionKind::Compare { lhs, rhs, .. }
        | InstructionKind::DynamicInstanceOf {
            object: lhs,
            target: rhs,
            ..
        } => {
            remap_operand_constants(lhs, remap);
            remap_operand_constants(rhs, remap);
        }
        InstructionKind::InstanceOf { object, .. }
        | InstructionKind::Unary { src: object, .. }
        | InstructionKind::Cast { src: object, .. }
        | InstructionKind::CloneObject { object, .. }
        | InstructionKind::FetchProperty { object, .. }
        | InstructionKind::IssetProperty { object, .. }
        | InstructionKind::EmptyProperty { object, .. } => {
            remap_operand_constants(object, remap);
        }
        InstructionKind::BindReferenceProperty { object, .. } => {
            remap_operand_constants(object, remap);
        }
        InstructionKind::FetchDynamicProperty {
            object, property, ..
        }
        | InstructionKind::IssetDynamicProperty {
            object, property, ..
        }
        | InstructionKind::EmptyDynamicProperty {
            object, property, ..
        } => {
            remap_operand_constants(object, remap);
            remap_operand_constants(property, remap);
        }
        InstructionKind::IssetDynamicPropertyDim {
            object,
            property,
            dims,
            ..
        }
        | InstructionKind::EmptyDynamicPropertyDim {
            object,
            property,
            dims,
            ..
        } => {
            remap_operand_constants(object, remap);
            remap_operand_constants(property, remap);
            remap_operands_constants(dims, remap);
        }
        InstructionKind::Yield { key, value, .. } => {
            remap_optional_operand_constants(key, remap);
            remap_optional_operand_constants(value, remap);
        }
        InstructionKind::MakeException { message, .. } => {
            remap_operand_constants(message, remap);
        }
        InstructionKind::MakeClosure { captures, .. } => {
            for capture in captures {
                remap_operand_constants(&mut capture.src, remap);
            }
        }
        InstructionKind::CallFunction { args, .. }
        | InstructionKind::CallStaticMethod { args, .. }
        | InstructionKind::NewObject { args, .. }
        | InstructionKind::BindReferenceFromCall { args, .. } => {
            remap_call_args_constants(args, remap);
        }
        InstructionKind::BindReferenceFromMethodCall { object, args, .. } => {
            remap_operand_constants(object, remap);
            remap_call_args_constants(args, remap);
        }
        InstructionKind::BindReferenceFromProperty { object, .. } => {
            remap_operand_constants(object, remap);
        }
        InstructionKind::BindReferenceFromPropertyDim { object, dims, .. } => {
            remap_operand_constants(object, remap);
            remap_operands_constants(dims, remap);
        }
        InstructionKind::CallMethod { object, args, .. } => {
            remap_operand_constants(object, remap);
            remap_call_args_constants(args, remap);
        }
        InstructionKind::CallClosure { callee, args, .. }
        | InstructionKind::CallCallable { callee, args, .. } => {
            remap_operand_constants(callee, remap);
            remap_call_args_constants(args, remap);
        }
        InstructionKind::Pipe {
            input, callable, ..
        } => {
            remap_operand_constants(input, remap);
            remap_operand_constants(callable, remap);
        }
        InstructionKind::CloneWith {
            object,
            replacements,
            ..
        }
        | InstructionKind::AssignProperty {
            object,
            value: replacements,
            ..
        } => {
            remap_operand_constants(object, remap);
            remap_operand_constants(replacements, remap);
        }
        InstructionKind::AssignPropertyDim {
            object,
            dims,
            value,
            ..
        } => {
            remap_operand_constants(object, remap);
            remap_operands_constants(dims, remap);
            remap_operand_constants(value, remap);
        }
        InstructionKind::AssignDynamicProperty {
            object,
            property,
            value,
            ..
        } => {
            remap_operand_constants(object, remap);
            remap_operand_constants(property, remap);
            remap_operand_constants(value, remap);
        }
        InstructionKind::IssetPropertyDim { object, dims, .. }
        | InstructionKind::EmptyPropertyDim { object, dims, .. }
        | InstructionKind::BindReferencePropertyDim { object, dims, .. } => {
            remap_operand_constants(object, remap);
            remap_operands_constants(dims, remap);
        }
        InstructionKind::BindReferenceDimFromProperty { object, dims, .. } => {
            remap_operand_constants(object, remap);
            remap_operands_constants(dims, remap);
        }
        InstructionKind::AssignDim { dims, value, .. }
        | InstructionKind::AppendDim { dims, value, .. } => {
            remap_operands_constants(dims, remap);
            remap_operand_constants(value, remap);
        }
        InstructionKind::ArrayInsert { key, value, .. } => {
            remap_optional_operand_constants(key, remap);
            remap_operand_constants(value, remap);
        }
        InstructionKind::ArraySpread { source, .. } => {
            remap_operand_constants(source, remap);
        }
        InstructionKind::FetchDim { array, key, .. } => {
            remap_operand_constants(array, remap);
            remap_operand_constants(key, remap);
        }
        InstructionKind::IssetDim { dims, .. }
        | InstructionKind::EmptyDim { dims, .. }
        | InstructionKind::UnsetDim { dims, .. }
        | InstructionKind::IssetStaticPropertyDim { dims, .. }
        | InstructionKind::EmptyStaticPropertyDim { dims, .. }
        | InstructionKind::UnsetStaticPropertyDim { dims, .. }
        | InstructionKind::BindReferenceDim { dims, .. }
        | InstructionKind::BindReferenceFromDim { dims, .. }
        | InstructionKind::BindReferenceFromStaticPropertyDim { dims, .. } => {
            remap_operands_constants(dims, remap);
        }
        InstructionKind::AssignStaticProperty { value, .. } => {
            remap_operand_constants(value, remap);
        }
        InstructionKind::FetchDynamicStaticProperty { class_name, .. } => {
            remap_operand_constants(class_name, remap);
        }
        InstructionKind::AssignDynamicStaticProperty {
            class_name, value, ..
        } => {
            remap_operand_constants(class_name, remap);
            remap_operand_constants(value, remap);
        }
        InstructionKind::FetchObjectClassName { object, .. } => {
            remap_operand_constants(object, remap);
        }
        InstructionKind::ArrayGet { array, index, .. } => {
            remap_operand_constants(array, remap);
            remap_operand_constants(index, remap);
        }
        InstructionKind::Nop
        | InstructionKind::FetchConst { .. }
        | InstructionKind::LoadLocal { .. }
        | InstructionKind::LoadLocalQuiet { .. }
        | InstructionKind::DeclareFunction { .. }
        | InstructionKind::DeclareClass { .. }
        | InstructionKind::BindReference { .. }
        | InstructionKind::BindGlobal { .. }
        | InstructionKind::BindReferenceStaticProperty { .. }
        | InstructionKind::EmitDiagnostic { .. }
        | InstructionKind::EnterTry { .. }
        | InstructionKind::LeaveTry
        | InstructionKind::EndFinally { .. }
        | InstructionKind::ResolveCallable { .. }
        | InstructionKind::FetchStaticProperty { .. }
        | InstructionKind::IssetStaticProperty { .. }
        | InstructionKind::EmptyStaticProperty { .. }
        | InstructionKind::FetchClassConstant { .. }
        | InstructionKind::NewArray { .. }
        | InstructionKind::IssetLocal { .. }
        | InstructionKind::EmptyLocal { .. }
        | InstructionKind::UnsetLocal { .. }
        | InstructionKind::ForeachNext { .. }
        | InstructionKind::ForeachCleanup { .. }
        | InstructionKind::ForeachInitRef { .. }
        | InstructionKind::ForeachNextRef { .. }
        | InstructionKind::RuntimeError { .. } => {}
    }
}

fn remap_terminator_constants(kind: &mut TerminatorKind, remap: &[ConstId]) {
    match kind {
        TerminatorKind::Jump { .. } => {}
        TerminatorKind::JumpIfFalse { condition, .. }
        | TerminatorKind::JumpIfTrue { condition, .. }
        | TerminatorKind::JumpIf { condition, .. } => {
            remap_operand_constants(condition, remap);
        }
        TerminatorKind::Return { value, .. } => {
            remap_optional_operand_constants(value, remap);
        }
        TerminatorKind::Exit { value } => {
            remap_optional_operand_constants(value, remap);
        }
    }
}
