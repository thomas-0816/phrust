//! IR invariant verifier.

use crate::block::BasicBlock;
use crate::function::IrFunction;
use crate::ids::{BlockId, ConstId, InstrId, LocalId, RegId};
use crate::instruction::{Instruction, InstructionKind, IrCallArg, Terminator, TerminatorKind};
use crate::module::{IR_VERSION, IrUnit};
use crate::operand::Operand;
use crate::source_map::IrSpan;
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticLocation, DiagnosticPhase, DiagnosticSeverity,
    DiagnosticSpan,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Stable verifier error code.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationErrorCode {
    /// Unit version is not supported.
    InvalidVersion,
    /// Entry function points outside the function table.
    InvalidEntryFunction,
    /// File table ID does not match its position.
    InvalidFileId,
    /// Class table ID does not match its position.
    InvalidClassId,
    /// A span references an unknown file or has an invalid range.
    InvalidSpan,
    /// Block ID does not match its position.
    InvalidBlockId,
    /// Instruction ID does not match its position.
    InvalidInstrId,
    /// Operand or destination register is outside the function register range.
    InvalidRegId,
    /// Operand or parameter local is outside the function local range.
    InvalidLocalId,
    /// Constant ID points outside the constant pool.
    InvalidConstId,
    /// Function lookup table entry points outside the function table.
    InvalidFunctionId,
    /// Function lookup table contains a duplicate normalized name.
    DuplicateFunctionName,
    /// Constant lookup table contains a duplicate name.
    DuplicateConstantName,
    /// Terminator target points outside the function block table.
    InvalidBlockTarget,
    /// Basic block is missing a terminator.
    MissingTerminator,
    /// Register operand can be read before every incoming path defines it.
    UndefinedRegisterUse,
    /// Call or return by-reference metadata is internally inconsistent.
    InvalidCallArgMetadata,
}

impl VerificationErrorCode {
    /// Stable machine-readable diagnostic ID for verifier consumers.
    #[must_use]
    pub const fn diagnostic_id(self) -> &'static str {
        match self {
            Self::InvalidVersion => "E_PHP_IR_VERIFY_INVALID_VERSION",
            Self::InvalidEntryFunction => "E_PHP_IR_VERIFY_INVALID_ENTRY_FUNCTION",
            Self::InvalidFileId => "E_PHP_IR_VERIFY_INVALID_FILE_ID",
            Self::InvalidClassId => "E_PHP_IR_VERIFY_INVALID_CLASS_ID",
            Self::InvalidSpan => "E_PHP_IR_VERIFY_INVALID_SPAN",
            Self::InvalidBlockId => "E_PHP_IR_VERIFY_INVALID_BLOCK_ID",
            Self::InvalidInstrId => "E_PHP_IR_VERIFY_INVALID_INSTR_ID",
            Self::InvalidRegId => "E_PHP_IR_VERIFY_INVALID_REG_ID",
            Self::InvalidLocalId => "E_PHP_IR_VERIFY_INVALID_LOCAL_ID",
            Self::InvalidConstId => "E_PHP_IR_VERIFY_INVALID_CONST_ID",
            Self::InvalidFunctionId => "E_PHP_IR_VERIFY_INVALID_FUNCTION_ID",
            Self::DuplicateFunctionName => "E_PHP_IR_VERIFY_DUPLICATE_FUNCTION_NAME",
            Self::DuplicateConstantName => "E_PHP_IR_VERIFY_DUPLICATE_CONSTANT_NAME",
            Self::InvalidBlockTarget => "E_PHP_IR_VERIFY_INVALID_BLOCK_TARGET",
            Self::MissingTerminator => "E_PHP_IR_VERIFY_MISSING_TERMINATOR",
            Self::UndefinedRegisterUse => "E_PHP_IR_VERIFY_UNDEFINED_REGISTER_USE",
            Self::InvalidCallArgMetadata => "E_PHP_IR_VERIFY_INVALID_CALL_ARG_METADATA",
        }
    }
}

/// One IR verifier error.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VerificationError {
    /// Stable error code.
    pub code: VerificationErrorCode,
    /// Human-readable context.
    pub message: String,
}

impl VerificationError {
    /// Stable machine-readable diagnostic ID for this verifier error.
    #[must_use]
    pub const fn diagnostic_id(&self) -> &'static str {
        self.code.diagnostic_id()
    }

    /// Converts this verifier error to the shared diagnostic envelope.
    #[must_use]
    pub fn to_diagnostic_envelope(
        &self,
        context: &VerificationDiagnosticContext,
    ) -> DiagnosticEnvelope {
        let mut metadata = BTreeMap::new();
        if let Some(function) = context.function {
            metadata.insert("function_id".to_string(), function.raw().to_string());
        }
        if let Some(block) = context.block {
            metadata.insert("block_id".to_string(), block.raw().to_string());
        }
        if let Some(instruction) = context.instruction {
            metadata.insert("instruction_id".to_string(), instruction.raw().to_string());
        }
        if let Some(span) = context.source_span {
            metadata.insert("file_id".to_string(), span.file.raw().to_string());
        }

        let envelope = DiagnosticEnvelope::new(
            self.diagnostic_id(),
            DiagnosticLayer::ir(),
            DiagnosticPhase::new("verify"),
            DiagnosticSeverity::Error,
            self.message.clone(),
        );
        let envelope = if let Some(span) = context.source_span {
            envelope.with_location(DiagnosticLocation::new(
                context.source_path.as_deref(),
                context.source_id.as_deref(),
                Some(DiagnosticSpan::new(span.start as usize, span.end as usize)),
            ))
        } else {
            envelope
        };
        envelope.with_context(metadata)
    }
}

/// Optional source-map context for an IR verifier error.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VerificationDiagnosticContext {
    /// Source path, when a verifier error can be mapped back to a file.
    pub source_path: Option<String>,
    /// Stable source ID, when available.
    pub source_id: Option<String>,
    /// Function that owns the failing IR.
    pub function: Option<crate::ids::FunctionId>,
    /// Basic block that owns the failing IR.
    pub block: Option<BlockId>,
    /// Instruction associated with the failure.
    pub instruction: Option<InstrId>,
    /// Source span associated with the failure.
    pub source_span: Option<IrSpan>,
}

/// Verifies basic IR invariants.
pub fn verify_unit(unit: &IrUnit) -> Result<(), Vec<VerificationError>> {
    let mut errors = Vec::new();

    if unit.version != IR_VERSION {
        errors.push(error(
            VerificationErrorCode::InvalidVersion,
            format!("unsupported IR version {}", unit.version),
        ));
    }
    if unit.entry.index() >= unit.functions.len() {
        errors.push(error(
            VerificationErrorCode::InvalidEntryFunction,
            format!("entry function {} is not defined", unit.entry.raw()),
        ));
    }
    for (index, file) in unit.files.iter().enumerate() {
        if file.id.index() != index {
            errors.push(error(
                VerificationErrorCode::InvalidFileId,
                format!("file table entry {index} has id {}", file.id.raw()),
            ));
        }
    }
    for (index, class) in unit.classes.iter().enumerate() {
        if class.id.index() != index {
            errors.push(error(
                VerificationErrorCode::InvalidClassId,
                format!("class table entry {index} has id {}", class.id.raw()),
            ));
        }
        verify_span(unit, class.span, &mut errors);
        for method in &class.methods {
            verify_function_id(method.function, unit.functions.len(), &mut errors);
        }
        for property in &class.properties {
            if let Some(default) = property.default {
                verify_constant(default, unit.constants.len(), &mut errors);
            }
        }
        if let Some(constructor) = class.constructor {
            verify_function_id(constructor, unit.functions.len(), &mut errors);
        }
    }
    for entry in &unit.function_table {
        if entry.function.index() >= unit.functions.len() {
            errors.push(error(
                VerificationErrorCode::InvalidFunctionId,
                format!(
                    "function table entry {:?} points at missing function {}",
                    entry.name,
                    entry.function.raw()
                ),
            ));
        }
        if unit
            .function_table
            .iter()
            .filter(|other| other.name == entry.name)
            .count()
            > 1
        {
            errors.push(error(
                VerificationErrorCode::DuplicateFunctionName,
                format!("function table contains duplicate name {:?}", entry.name),
            ));
        }
    }
    for entry in &unit.constant_table {
        verify_constant(entry.value, unit.constants.len(), &mut errors);
        verify_span(unit, entry.span, &mut errors);
        if unit
            .constant_table
            .iter()
            .filter(|other| other.name == entry.name)
            .count()
            > 1
        {
            errors.push(error(
                VerificationErrorCode::DuplicateConstantName,
                format!("constant table contains duplicate name {:?}", entry.name),
            ));
        }
    }
    for function in &unit.functions {
        verify_function(unit, function, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn verify_function(unit: &IrUnit, function: &IrFunction, errors: &mut Vec<VerificationError>) {
    verify_span(unit, function.span, errors);
    if function.locals.len() != function.local_count as usize {
        errors.push(error(
            VerificationErrorCode::InvalidLocalId,
            format!(
                "function has {} local names but local_count is {}",
                function.locals.len(),
                function.local_count
            ),
        ));
    }
    for param in &function.params {
        verify_local(param.local, function.local_count, errors);
    }
    for capture in &function.captures {
        verify_local(capture.local, function.local_count, errors);
    }
    for (index, block) in function.blocks.iter().enumerate() {
        verify_block_id(block.id, index, errors);
        verify_block(unit, function, block, errors);
    }
    verify_register_definitions(function, errors);
}

fn verify_block(
    unit: &IrUnit,
    function: &IrFunction,
    block: &BasicBlock,
    errors: &mut Vec<VerificationError>,
) {
    for (index, instruction) in block.instructions.iter().enumerate() {
        if instruction.id.index() != index {
            errors.push(error(
                VerificationErrorCode::InvalidInstrId,
                format!(
                    "block {} instruction {index} has id {}",
                    block.id.raw(),
                    instruction.id.raw()
                ),
            ));
        }
        verify_instruction(unit, function, instruction, errors);
    }
    match &block.terminator {
        Some(terminator) => verify_terminator(unit, function, terminator, errors),
        None => errors.push(error(
            VerificationErrorCode::MissingTerminator,
            format!("block {} has no terminator", block.id.raw()),
        )),
    }
}

fn verify_instruction(
    unit: &IrUnit,
    function: &IrFunction,
    instruction: &Instruction,
    errors: &mut Vec<VerificationError>,
) {
    verify_span(unit, instruction.span, errors);
    match &instruction.kind {
        InstructionKind::Nop
        | InstructionKind::EmitDiagnostic { .. }
        | InstructionKind::Unsupported { .. }
        | InstructionKind::RuntimeError { .. } => {}
        InstructionKind::LoadConst { dst, constant } => {
            verify_register(*dst, function.register_count, errors);
            verify_constant(*constant, unit.constants.len(), errors);
        }
        InstructionKind::FetchConst { dst, .. } => {
            verify_register(*dst, function.register_count, errors);
        }
        InstructionKind::RegisterConstant { value, .. } => {
            verify_operand(value, function, unit, errors);
        }
        InstructionKind::Move { dst, src } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(src, function, unit, errors);
        }
        InstructionKind::LoadLocal { dst, local }
        | InstructionKind::LoadLocalQuiet { dst, local } => {
            verify_register(*dst, function.register_count, errors);
            verify_local(*local, function.local_count, errors);
        }
        InstructionKind::StoreLocal { local, src } => {
            verify_local(*local, function.local_count, errors);
            verify_operand(src, function, unit, errors);
        }
        InstructionKind::BindReference { target, source } => {
            verify_local(*target, function.local_count, errors);
            verify_local(*source, function.local_count, errors);
        }
        InstructionKind::BindGlobal { local, .. } => {
            verify_local(*local, function.local_count, errors);
        }
        InstructionKind::BindReferenceDim {
            local,
            dims,
            source,
            ..
        } => {
            verify_local(*local, function.local_count, errors);
            verify_local(*source, function.local_count, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
        }
        InstructionKind::BindReferenceProperty { object, source, .. } => {
            verify_operand(object, function, unit, errors);
            verify_local(*source, function.local_count, errors);
        }
        InstructionKind::BindReferencePropertyDim {
            object,
            dims,
            source,
            ..
        } => {
            verify_operand(object, function, unit, errors);
            verify_local(*source, function.local_count, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
        }
        InstructionKind::BindReferenceDimFromProperty {
            local,
            dims,
            object,
            ..
        } => {
            verify_local(*local, function.local_count, errors);
            verify_operand(object, function, unit, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
        }
        InstructionKind::BindReferenceFromDim {
            target,
            local,
            dims,
        } => {
            verify_local(*target, function.local_count, errors);
            verify_local(*local, function.local_count, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
        }
        InstructionKind::BindReferenceProperty { object, source, .. } => {
            verify_operand(object, function, unit, errors);
            verify_local(*source, function.local_count, errors);
        }
        InstructionKind::BindReferenceStaticProperty { source, .. } => {
            verify_local(*source, function.local_count, errors);
        }
        InstructionKind::InitStaticLocal { local, default, .. } => {
            verify_local(*local, function.local_count, errors);
            verify_operand(default, function, unit, errors);
        }
        InstructionKind::Binary { dst, lhs, rhs, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(lhs, function, unit, errors);
            verify_operand(rhs, function, unit, errors);
        }
        InstructionKind::Compare { dst, lhs, rhs, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(lhs, function, unit, errors);
            verify_operand(rhs, function, unit, errors);
        }
        InstructionKind::InstanceOf { dst, object, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
        }
        InstructionKind::DynamicInstanceOf {
            dst,
            object,
            target,
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            verify_operand(target, function, unit, errors);
        }
        InstructionKind::Unary { dst, src, .. } | InstructionKind::Cast { dst, src, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(src, function, unit, errors);
        }
        InstructionKind::Discard { src } => verify_operand(src, function, unit, errors),
        InstructionKind::Echo { src } => verify_operand(src, function, unit, errors),
        InstructionKind::Yield { dst, key, value } => {
            verify_register(*dst, function.register_count, errors);
            if let Some(key) = key {
                verify_operand(key, function, unit, errors);
            }
            if let Some(value) = value {
                verify_operand(value, function, unit, errors);
            }
        }
        InstructionKind::YieldFrom { dst, source } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(source, function, unit, errors);
        }
        InstructionKind::BindReferenceFromCall { target, args, .. } => {
            verify_local(*target, function.local_count, errors);
            verify_call_args(args, function, unit, errors);
        }
        InstructionKind::CallFunction { dst, args, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_call_args(args, function, unit, errors);
        }
        InstructionKind::CallMethod {
            dst, object, args, ..
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            verify_call_args(args, function, unit, errors);
        }
        InstructionKind::CallStaticMethod { dst, args, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_call_args(args, function, unit, errors);
        }
        InstructionKind::CloneObject { dst, object } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
        }
        InstructionKind::CloneWith {
            dst,
            object,
            replacements,
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            verify_operand(replacements, function, unit, errors);
        }
        InstructionKind::EnterTry {
            catch,
            catch_types: _,
            finally,
            after,
            exception_local,
        } => {
            if let Some(catch) = catch {
                verify_block_target(*catch, function, errors);
            }
            if let Some(finally) = finally {
                verify_block_target(*finally, function, errors);
            }
            verify_block_target(*after, function, errors);
            if let Some(local) = exception_local {
                verify_local(*local, function.local_count, errors);
            }
        }
        InstructionKind::LeaveTry => {}
        InstructionKind::EndFinally { after } => verify_block_target(*after, function, errors),
        InstructionKind::Throw { value } => verify_operand(value, function, unit, errors),
        InstructionKind::MakeException {
            dst,
            class_name: _,
            message,
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(message, function, unit, errors);
        }
        InstructionKind::MakeClosure {
            dst,
            function: closure_function,
            captures,
        } => {
            verify_register(*dst, function.register_count, errors);
            if closure_function.index() >= unit.functions.len() {
                errors.push(error(
                    VerificationErrorCode::InvalidFunctionId,
                    format!(
                        "make_closure points at missing function {}",
                        closure_function.raw()
                    ),
                ));
            }
            for capture in captures {
                verify_operand(&capture.src, function, unit, errors);
            }
        }
        InstructionKind::CallClosure { dst, callee, args } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(callee, function, unit, errors);
            verify_call_args(args, function, unit, errors);
        }
        InstructionKind::ResolveCallable { dst, .. } => {
            verify_register(*dst, function.register_count, errors);
        }
        InstructionKind::AcquireCallable { dst, value } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(value, function, unit, errors);
        }
        InstructionKind::CallCallable { dst, callee, args } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(callee, function, unit, errors);
            verify_call_args(args, function, unit, errors);
        }
        InstructionKind::Pipe {
            dst,
            input,
            callable,
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(input, function, unit, errors);
            verify_operand(callable, function, unit, errors);
        }
        InstructionKind::Include { dst, path, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(path, function, unit, errors);
        }
        InstructionKind::Eval { dst, code } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(code, function, unit, errors);
        }
        InstructionKind::NewObject { dst, args, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_call_args(args, function, unit, errors);
        }
        InstructionKind::DynamicNewObject {
            dst,
            class_name,
            args,
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(class_name, function, unit, errors);
            verify_call_args(args, function, unit, errors);
        }
        InstructionKind::FetchProperty { dst, object, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
        }
        InstructionKind::FetchDynamicProperty {
            dst,
            object,
            property,
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            verify_operand(property, function, unit, errors);
        }
        InstructionKind::IssetProperty { dst, object, .. }
        | InstructionKind::EmptyProperty { dst, object, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
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
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            verify_operand(property, function, unit, errors);
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
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            verify_operand(property, function, unit, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
        }
        InstructionKind::IssetPropertyDim {
            dst, object, dims, ..
        }
        | InstructionKind::EmptyPropertyDim {
            dst, object, dims, ..
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
        }
        InstructionKind::UnsetProperty { object, .. } => {
            verify_operand(object, function, unit, errors);
        }
        InstructionKind::UnsetPropertyDim { object, dims, .. } => {
            verify_operand(object, function, unit, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
        }
        InstructionKind::UnsetDynamicProperty { object, property } => {
            verify_operand(object, function, unit, errors);
            verify_operand(property, function, unit, errors);
        }
        InstructionKind::FetchStaticProperty { dst, .. }
        | InstructionKind::IssetStaticProperty { dst, .. }
        | InstructionKind::EmptyStaticProperty { dst, .. }
        | InstructionKind::FetchClassConstant { dst, .. } => {
            verify_register(*dst, function.register_count, errors);
        }
        InstructionKind::FetchObjectClassName { dst, object } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
        }
        InstructionKind::AssignProperty {
            dst, object, value, ..
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            verify_operand(value, function, unit, errors);
        }
        InstructionKind::AssignPropertyDim {
            dst,
            object,
            dims,
            value,
            ..
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
            verify_operand(value, function, unit, errors);
        }
        InstructionKind::AssignDynamicProperty {
            dst,
            object,
            property,
            value,
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(object, function, unit, errors);
            verify_operand(property, function, unit, errors);
            verify_operand(value, function, unit, errors);
        }
        InstructionKind::AssignStaticProperty { dst, value, .. } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(value, function, unit, errors);
        }
        InstructionKind::NewArray { dst } => {
            verify_register(*dst, function.register_count, errors);
        }
        InstructionKind::ArrayInsert {
            array,
            key,
            value,
            by_ref_local,
        } => {
            verify_register(*array, function.register_count, errors);
            if let Some(key) = key {
                verify_operand(key, function, unit, errors);
            }
            verify_operand(value, function, unit, errors);
            if let Some(local) = by_ref_local {
                verify_local(*local, function.local_count, errors);
            }
        }
        InstructionKind::ArraySpread { array, source } => {
            verify_register(*array, function.register_count, errors);
            verify_operand(source, function, unit, errors);
        }
        InstructionKind::FetchDim {
            dst, array, key, ..
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(array, function, unit, errors);
            verify_operand(key, function, unit, errors);
        }
        InstructionKind::AssignDim {
            dst,
            local,
            dims,
            value,
        }
        | InstructionKind::AppendDim {
            dst,
            local,
            dims,
            value,
        } => {
            verify_register(*dst, function.register_count, errors);
            verify_local(*local, function.local_count, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
            verify_operand(value, function, unit, errors);
        }
        InstructionKind::IssetLocal { dst, local } | InstructionKind::EmptyLocal { dst, local } => {
            verify_register(*dst, function.register_count, errors);
            verify_local(*local, function.local_count, errors);
        }
        InstructionKind::UnsetLocal { local } => {
            verify_local(*local, function.local_count, errors);
        }
        InstructionKind::IssetDim { dst, local, dims }
        | InstructionKind::EmptyDim { dst, local, dims } => {
            verify_register(*dst, function.register_count, errors);
            verify_local(*local, function.local_count, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
        }
        InstructionKind::UnsetDim { local, dims } => {
            verify_local(*local, function.local_count, errors);
            for dim in dims {
                verify_operand(dim, function, unit, errors);
            }
        }
        InstructionKind::ForeachInit { iterator, source } => {
            verify_register(*iterator, function.register_count, errors);
            verify_operand(source, function, unit, errors);
        }
        InstructionKind::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            verify_register(*has_value, function.register_count, errors);
            verify_register(*iterator, function.register_count, errors);
            if let Some(key) = key {
                verify_register(*key, function.register_count, errors);
            }
            verify_register(*value, function.register_count, errors);
        }
        InstructionKind::ForeachInitRef { iterator, local } => {
            verify_register(*iterator, function.register_count, errors);
            verify_local(*local, function.local_count, errors);
        }
        InstructionKind::ForeachNextRef {
            has_value,
            iterator,
            key,
            value_local,
        } => {
            verify_register(*has_value, function.register_count, errors);
            verify_register(*iterator, function.register_count, errors);
            if let Some(key) = key {
                verify_register(*key, function.register_count, errors);
            }
            verify_local(*value_local, function.local_count, errors);
        }
        InstructionKind::ArrayGet { dst, array, index } => {
            verify_register(*dst, function.register_count, errors);
            verify_operand(array, function, unit, errors);
            verify_operand(index, function, unit, errors);
        }
    }
}

fn verify_terminator(
    unit: &IrUnit,
    function: &IrFunction,
    terminator: &Terminator,
    errors: &mut Vec<VerificationError>,
) {
    verify_span(unit, terminator.span, errors);
    match &terminator.kind {
        TerminatorKind::Jump { target } => verify_block_target(*target, function, errors),
        TerminatorKind::JumpIfFalse { condition, target }
        | TerminatorKind::JumpIfTrue { condition, target } => {
            verify_operand(condition, function, unit, errors);
            verify_block_target(*target, function, errors);
        }
        TerminatorKind::JumpIf {
            condition,
            if_true,
            if_false,
        } => {
            verify_operand(condition, function, unit, errors);
            verify_block_target(*if_true, function, errors);
            verify_block_target(*if_false, function, errors);
        }
        TerminatorKind::Return {
            value,
            by_ref_local,
        } => {
            if let Some(value) = value {
                verify_operand(value, function, unit, errors);
            }
            if let Some(local) = by_ref_local {
                verify_local(*local, function.local_count, errors);
                if value != &Some(Operand::Local(*local)) {
                    errors.push(error(
                        VerificationErrorCode::InvalidCallArgMetadata,
                        format!(
                            "by-reference return local {} does not match return value",
                            local.raw()
                        ),
                    ));
                }
            }
        }
        TerminatorKind::Exit { value } => {
            if let Some(value) = value {
                verify_operand(value, function, unit, errors);
            }
        }
    }
}

fn verify_call_args(
    args: &[IrCallArg],
    function: &IrFunction,
    unit: &IrUnit,
    errors: &mut Vec<VerificationError>,
) {
    for arg in args {
        verify_operand(&arg.value, function, unit, errors);
        if let Some(local) = arg.by_ref_local {
            verify_local(local, function.local_count, errors);
            if arg.unpack {
                errors.push(error(
                    VerificationErrorCode::InvalidCallArgMetadata,
                    format!(
                        "unpacked call argument cannot carry by-reference local {}",
                        local.raw()
                    ),
                ));
            }
        }
        if let Some(dim) = &arg.by_ref_dim {
            verify_local(dim.local, function.local_count, errors);
            for operand in &dim.dims {
                verify_operand(operand, function, unit, errors);
            }
            if arg.unpack {
                errors.push(error(
                    VerificationErrorCode::InvalidCallArgMetadata,
                    "unpacked call argument cannot carry by-reference dimension metadata"
                        .to_owned(),
                ));
            }
        }
        if let Some(property) = &arg.by_ref_property {
            verify_operand(&property.object, function, unit, errors);
            if arg.unpack {
                errors.push(error(
                    VerificationErrorCode::InvalidCallArgMetadata,
                    "unpacked call argument cannot carry by-reference property metadata".to_owned(),
                ));
            }
        }
    }
}

fn verify_register_definitions(function: &IrFunction, errors: &mut Vec<VerificationError>) {
    let block_count = function.blocks.len();
    let register_count = function.register_count as usize;
    if block_count == 0 || register_count == 0 {
        return;
    }

    let predecessors = block_predecessors(function);
    let reachable = reachable_blocks(function);
    let mut in_defs = vec![vec![true; register_count]; block_count];
    let mut out_defs = vec![vec![true; register_count]; block_count];
    in_defs[0] = vec![false; register_count];
    out_defs[0] = apply_register_defs(&function.blocks[0], in_defs[0].clone());

    let mut changed = true;
    while changed {
        changed = false;
        for block_index in 0..block_count {
            if !reachable[block_index] {
                continue;
            }
            let next_in = if block_index == 0 || predecessors[block_index].is_empty() {
                vec![false; register_count]
            } else {
                intersect_predecessor_defs(
                    function,
                    block_index,
                    &predecessors[block_index],
                    &reachable,
                    &out_defs,
                    register_count,
                )
            };
            let next_out = apply_register_defs(&function.blocks[block_index], next_in.clone());
            if next_in != in_defs[block_index] || next_out != out_defs[block_index] {
                in_defs[block_index] = next_in;
                out_defs[block_index] = next_out;
                changed = true;
            }
        }
    }

    for (block_index, block) in function.blocks.iter().enumerate() {
        if !reachable[block_index] {
            continue;
        }
        let mut defined = in_defs[block_index].clone();
        for instruction in &block.instructions {
            let mut uses = Vec::new();
            instruction_register_uses(&instruction.kind, &mut uses);
            report_undefined_registers(block.id, instruction.id, &uses, &defined, errors);
            mark_instruction_defs(&instruction.kind, &mut defined);
        }
        if let Some(terminator) = &block.terminator {
            let mut uses = Vec::new();
            terminator_register_uses(&terminator.kind, &mut uses);
            report_undefined_registers(block.id, InstrId::new(u32::MAX), &uses, &defined, errors);
        }
    }
}

fn block_predecessors(function: &IrFunction) -> Vec<Vec<usize>> {
    let mut predecessors = vec![Vec::new(); function.blocks.len()];
    for (source_index, block) in function.blocks.iter().enumerate() {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        let mut targets = Vec::new();
        terminator_targets(&terminator.kind, &mut targets);
        for target in targets {
            if target.index() < predecessors.len() {
                predecessors[target.index()].push(source_index);
            }
        }
    }
    predecessors
}

fn intersect_predecessor_defs(
    function: &IrFunction,
    target_index: usize,
    predecessors: &[usize],
    reachable: &[bool],
    out_defs: &[Vec<bool>],
    register_count: usize,
) -> Vec<bool> {
    let mut result = vec![true; register_count];
    let mut saw_reachable_predecessor = false;
    for predecessor in predecessors {
        if !reachable[*predecessor] {
            continue;
        }
        saw_reachable_predecessor = true;
        let edge_defs = edge_register_defs(
            &function.blocks[*predecessor],
            BlockId::new(target_index as u32),
            &out_defs[*predecessor],
        );
        for (index, defined) in edge_defs.iter().enumerate() {
            result[index] &= *defined;
        }
    }
    if !saw_reachable_predecessor {
        return vec![false; register_count];
    }
    result
}

fn reachable_blocks(function: &IrFunction) -> Vec<bool> {
    let mut reachable = vec![false; function.blocks.len()];
    if function.blocks.is_empty() {
        return reachable;
    }
    let mut stack = vec![0usize];
    while let Some(index) = stack.pop() {
        if index >= function.blocks.len() || reachable[index] {
            continue;
        }
        reachable[index] = true;
        let Some(terminator) = &function.blocks[index].terminator else {
            continue;
        };
        let mut targets = Vec::new();
        terminator_targets(&terminator.kind, &mut targets);
        for target in targets {
            if target.index() < function.blocks.len() {
                stack.push(target.index());
            }
        }
    }
    reachable
}

fn edge_register_defs(block: &BasicBlock, target: BlockId, out_defs: &[bool]) -> Vec<bool> {
    let mut defs = out_defs.to_vec();
    let Some(terminator) = &block.terminator else {
        return defs;
    };
    if !is_false_successor(&terminator.kind, target) {
        return defs;
    }
    let Some(last) = block.instructions.last() else {
        return defs;
    };
    match &last.kind {
        InstructionKind::ForeachNext { key, value, .. } => {
            if let Some(key) = key
                && let Some(slot) = defs.get_mut(key.index())
            {
                *slot = false;
            }
            if let Some(slot) = defs.get_mut(value.index()) {
                *slot = false;
            }
        }
        InstructionKind::ForeachNextRef { key, .. } => {
            if let Some(key) = key
                && let Some(slot) = defs.get_mut(key.index())
            {
                *slot = false;
            }
        }
        _ => {}
    }
    defs
}

fn is_false_successor(kind: &TerminatorKind, target: BlockId) -> bool {
    match kind {
        TerminatorKind::JumpIfFalse {
            condition: _,
            target: false_target,
        } => *false_target == target,
        TerminatorKind::JumpIfTrue { .. } => false,
        TerminatorKind::JumpIf { if_false, .. } => *if_false == target,
        TerminatorKind::Jump { .. }
        | TerminatorKind::Return { .. }
        | TerminatorKind::Exit { .. } => false,
    }
}

fn apply_register_defs(block: &BasicBlock, mut defined: Vec<bool>) -> Vec<bool> {
    for instruction in &block.instructions {
        mark_instruction_defs(&instruction.kind, &mut defined);
    }
    defined
}

fn report_undefined_registers(
    block: BlockId,
    instruction: InstrId,
    uses: &[RegId],
    defined: &[bool],
    errors: &mut Vec<VerificationError>,
) {
    for register in uses {
        if register.index() < defined.len() && !defined[register.index()] {
            let where_ = if instruction.raw() == u32::MAX {
                format!("block {} terminator", block.raw())
            } else {
                format!("block {} instruction {}", block.raw(), instruction.raw())
            };
            errors.push(error(
                VerificationErrorCode::UndefinedRegisterUse,
                format!(
                    "{where_} reads register {} before definition",
                    register.raw()
                ),
            ));
        }
    }
}

fn verify_span(unit: &IrUnit, span: IrSpan, errors: &mut Vec<VerificationError>) {
    if span.start > span.end || span.file.index() >= unit.files.len() {
        errors.push(error(
            VerificationErrorCode::InvalidSpan,
            format!(
                "span file {} range {}..{} is invalid",
                span.file.raw(),
                span.start,
                span.end
            ),
        ));
    }
}

fn verify_operand(
    operand: &Operand,
    function: &IrFunction,
    unit: &IrUnit,
    errors: &mut Vec<VerificationError>,
) {
    match operand {
        Operand::Register(id) => verify_register(*id, function.register_count, errors),
        Operand::Local(id) => verify_local(*id, function.local_count, errors),
        Operand::Constant(id) => verify_constant(*id, unit.constants.len(), errors),
    }
}

fn instruction_register_uses(kind: &InstructionKind, uses: &mut Vec<RegId>) {
    match kind {
        InstructionKind::Nop
        | InstructionKind::LoadConst { .. }
        | InstructionKind::FetchConst { .. }
        | InstructionKind::LoadLocal { .. }
        | InstructionKind::LoadLocalQuiet { .. }
        | InstructionKind::BindReference { .. }
        | InstructionKind::BindGlobal { .. }
        | InstructionKind::LeaveTry
        | InstructionKind::FetchStaticProperty { .. }
        | InstructionKind::IssetStaticProperty { .. }
        | InstructionKind::EmptyStaticProperty { .. }
        | InstructionKind::FetchClassConstant { .. }
        | InstructionKind::NewArray { .. }
        | InstructionKind::IssetLocal { .. }
        | InstructionKind::EmptyLocal { .. }
        | InstructionKind::UnsetLocal { .. }
        | InstructionKind::ForeachInitRef { .. }
        | InstructionKind::EmitDiagnostic { .. }
        | InstructionKind::Unsupported { .. }
        | InstructionKind::RuntimeError { .. } => {}
        InstructionKind::RegisterConstant { value, .. } => operand_register_uses(value, uses),
        InstructionKind::Move { src, .. }
        | InstructionKind::StoreLocal { src, .. }
        | InstructionKind::InitStaticLocal { default: src, .. }
        | InstructionKind::InstanceOf { object: src, .. }
        | InstructionKind::Unary { src, .. }
        | InstructionKind::Cast { src, .. }
        | InstructionKind::Discard { src }
        | InstructionKind::Echo { src }
        | InstructionKind::YieldFrom { source: src, .. }
        | InstructionKind::Throw { value: src }
        | InstructionKind::CloneObject { object: src, .. }
        | InstructionKind::Include { path: src, .. }
        | InstructionKind::Eval { code: src, .. }
        | InstructionKind::FetchObjectClassName { object: src, .. }
        | InstructionKind::FetchProperty { object: src, .. }
        | InstructionKind::IssetProperty { object: src, .. }
        | InstructionKind::EmptyProperty { object: src, .. }
        | InstructionKind::UnsetProperty { object: src, .. }
        | InstructionKind::ForeachInit { source: src, .. } => operand_register_uses(src, uses),
        InstructionKind::UnsetPropertyDim { object, dims, .. } => {
            operand_register_uses(object, uses);
            for dim in dims {
                operand_register_uses(dim, uses);
            }
        }
        InstructionKind::DynamicInstanceOf { object, target, .. } => {
            operand_register_uses(object, uses);
            operand_register_uses(target, uses);
        }
        InstructionKind::FetchDynamicProperty {
            object, property, ..
        }
        | InstructionKind::IssetDynamicProperty {
            object, property, ..
        }
        | InstructionKind::EmptyDynamicProperty {
            object, property, ..
        }
        | InstructionKind::UnsetDynamicProperty { object, property } => {
            operand_register_uses(object, uses);
            operand_register_uses(property, uses);
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
            operand_register_uses(object, uses);
            operand_register_uses(property, uses);
            for dim in dims {
                operand_register_uses(dim, uses);
            }
        }
        InstructionKind::IssetPropertyDim { object, dims, .. }
        | InstructionKind::EmptyPropertyDim { object, dims, .. } => {
            operand_register_uses(object, uses);
            for dim in dims {
                operand_register_uses(dim, uses);
            }
        }
        InstructionKind::BindReferenceDim { dims, .. }
        | InstructionKind::BindReferenceFromDim { dims, .. }
        | InstructionKind::IssetDim { dims, .. }
        | InstructionKind::EmptyDim { dims, .. }
        | InstructionKind::UnsetDim { dims, .. } => {
            for dim in dims {
                operand_register_uses(dim, uses);
            }
        }
        InstructionKind::BindReferencePropertyDim { object, dims, .. } => {
            operand_register_uses(object, uses);
            for dim in dims {
                operand_register_uses(dim, uses);
            }
        }
        InstructionKind::BindReferenceProperty { object, .. } => {
            operand_register_uses(object, uses);
        }
        InstructionKind::BindReferenceDimFromProperty { dims, object, .. } => {
            operand_register_uses(object, uses);
            for dim in dims {
                operand_register_uses(dim, uses);
            }
        }
        InstructionKind::Binary { lhs, rhs, .. }
        | InstructionKind::Compare { lhs, rhs, .. }
        | InstructionKind::ArrayGet {
            array: lhs,
            index: rhs,
            ..
        } => {
            operand_register_uses(lhs, uses);
            operand_register_uses(rhs, uses);
        }
        InstructionKind::Yield { key, value, .. } => {
            if let Some(key) = key {
                operand_register_uses(key, uses);
            }
            if let Some(value) = value {
                operand_register_uses(value, uses);
            }
        }
        InstructionKind::BindReferenceFromCall { args, .. }
        | InstructionKind::CallFunction { args, .. }
        | InstructionKind::CallStaticMethod { args, .. }
        | InstructionKind::NewObject { args, .. } => call_args_register_uses(args, uses),
        InstructionKind::DynamicNewObject {
            class_name, args, ..
        } => {
            operand_register_uses(class_name, uses);
            call_args_register_uses(args, uses);
        }
        InstructionKind::CallMethod { object, args, .. } => {
            operand_register_uses(object, uses);
            call_args_register_uses(args, uses);
        }
        InstructionKind::CloneWith {
            object,
            replacements,
            ..
        } => {
            operand_register_uses(object, uses);
            operand_register_uses(replacements, uses);
        }
        InstructionKind::EnterTry { .. } | InstructionKind::EndFinally { .. } => {}
        InstructionKind::MakeException { message, .. } => operand_register_uses(message, uses),
        InstructionKind::MakeClosure { captures, .. } => {
            for capture in captures {
                operand_register_uses(&capture.src, uses);
            }
        }
        InstructionKind::CallClosure { callee, args, .. }
        | InstructionKind::CallCallable { callee, args, .. } => {
            operand_register_uses(callee, uses);
            call_args_register_uses(args, uses);
        }
        InstructionKind::AcquireCallable { value, .. } => operand_register_uses(value, uses),
        InstructionKind::ResolveCallable { .. } => {}
        InstructionKind::Pipe {
            input, callable, ..
        } => {
            operand_register_uses(input, uses);
            operand_register_uses(callable, uses);
        }
        InstructionKind::AssignProperty { object, value, .. } => {
            operand_register_uses(object, uses);
            operand_register_uses(value, uses);
        }
        InstructionKind::AssignPropertyDim {
            object,
            dims,
            value,
            ..
        } => {
            operand_register_uses(object, uses);
            for dim in dims {
                operand_register_uses(dim, uses);
            }
            operand_register_uses(value, uses);
        }
        InstructionKind::AssignDynamicProperty {
            object,
            property,
            value,
            ..
        } => {
            operand_register_uses(object, uses);
            operand_register_uses(property, uses);
            operand_register_uses(value, uses);
        }
        InstructionKind::AssignStaticProperty { value, .. } => operand_register_uses(value, uses),
        InstructionKind::BindReferenceProperty { object, .. } => {
            operand_register_uses(object, uses);
        }
        InstructionKind::BindReferenceStaticProperty { .. } => {}
        InstructionKind::ArrayInsert {
            array, key, value, ..
        } => {
            uses.push(*array);
            if let Some(key) = key {
                operand_register_uses(key, uses);
            }
            operand_register_uses(value, uses);
        }
        InstructionKind::ArraySpread { array, source } => {
            uses.push(*array);
            operand_register_uses(source, uses);
        }
        InstructionKind::FetchDim { array, key, .. } => {
            operand_register_uses(array, uses);
            operand_register_uses(key, uses);
        }
        InstructionKind::AssignDim { dims, value, .. }
        | InstructionKind::AppendDim { dims, value, .. } => {
            for dim in dims {
                operand_register_uses(dim, uses);
            }
            operand_register_uses(value, uses);
        }
        InstructionKind::ForeachNext { iterator, .. }
        | InstructionKind::ForeachNextRef { iterator, .. } => uses.push(*iterator),
    }
}

fn mark_instruction_defs(kind: &InstructionKind, defined: &mut [bool]) {
    let mut defs = Vec::new();
    instruction_register_defs(kind, &mut defs);
    for register in defs {
        if let Some(slot) = defined.get_mut(register.index()) {
            *slot = true;
        }
    }
}

fn instruction_register_defs(kind: &InstructionKind, defs: &mut Vec<RegId>) {
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
        | InstructionKind::DynamicNewObject { dst, .. }
        | InstructionKind::NewObject { dst, .. }
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
        | InstructionKind::IssetStaticProperty { dst, .. }
        | InstructionKind::EmptyStaticProperty { dst, .. }
        | InstructionKind::FetchClassConstant { dst, .. }
        | InstructionKind::FetchObjectClassName { dst, .. }
        | InstructionKind::AssignProperty { dst, .. }
        | InstructionKind::AssignPropertyDim { dst, .. }
        | InstructionKind::AssignDynamicProperty { dst, .. }
        | InstructionKind::AssignStaticProperty { dst, .. }
        | InstructionKind::NewArray { dst }
        | InstructionKind::FetchDim { dst, .. }
        | InstructionKind::AssignDim { dst, .. }
        | InstructionKind::AppendDim { dst, .. }
        | InstructionKind::IssetLocal { dst, .. }
        | InstructionKind::EmptyLocal { dst, .. }
        | InstructionKind::IssetDim { dst, .. }
        | InstructionKind::EmptyDim { dst, .. }
        | InstructionKind::ForeachInit { iterator: dst, .. }
        | InstructionKind::ForeachInitRef { iterator: dst, .. }
        | InstructionKind::ArrayGet { dst, .. } => defs.push(*dst),
        InstructionKind::ForeachNext {
            has_value,
            key,
            value,
            ..
        } => {
            defs.push(*has_value);
            if let Some(key) = key {
                defs.push(*key);
            }
            defs.push(*value);
        }
        InstructionKind::ForeachNextRef { has_value, key, .. } => {
            defs.push(*has_value);
            if let Some(key) = key {
                defs.push(*key);
            }
        }
        InstructionKind::Nop
        | InstructionKind::RegisterConstant { .. }
        | InstructionKind::StoreLocal { .. }
        | InstructionKind::BindReference { .. }
        | InstructionKind::BindGlobal { .. }
        | InstructionKind::BindReferenceDim { .. }
        | InstructionKind::BindReferenceProperty { .. }
        | InstructionKind::BindReferencePropertyDim { .. }
        | InstructionKind::BindReferenceDimFromProperty { .. }
        | InstructionKind::BindReferenceFromDim { .. }
        | InstructionKind::BindReferenceProperty { .. }
        | InstructionKind::BindReferenceStaticProperty { .. }
        | InstructionKind::BindReferenceFromCall { .. }
        | InstructionKind::InitStaticLocal { .. }
        | InstructionKind::Discard { .. }
        | InstructionKind::Echo { .. }
        | InstructionKind::EnterTry { .. }
        | InstructionKind::LeaveTry
        | InstructionKind::EndFinally { .. }
        | InstructionKind::Throw { .. }
        | InstructionKind::UnsetProperty { .. }
        | InstructionKind::UnsetPropertyDim { .. }
        | InstructionKind::UnsetDynamicProperty { .. }
        | InstructionKind::ArrayInsert { .. }
        | InstructionKind::ArraySpread { .. }
        | InstructionKind::UnsetLocal { .. }
        | InstructionKind::UnsetDim { .. }
        | InstructionKind::EmitDiagnostic { .. }
        | InstructionKind::Unsupported { .. }
        | InstructionKind::RuntimeError { .. } => {}
    }
}

fn terminator_register_uses(kind: &TerminatorKind, uses: &mut Vec<RegId>) {
    match kind {
        TerminatorKind::Jump { .. } => {}
        TerminatorKind::JumpIfFalse { condition, .. }
        | TerminatorKind::JumpIfTrue { condition, .. }
        | TerminatorKind::JumpIf { condition, .. } => operand_register_uses(condition, uses),
        TerminatorKind::Return { value, .. } => {
            if let Some(value) = value {
                operand_register_uses(value, uses);
            }
        }
        TerminatorKind::Exit { value } => {
            if let Some(value) = value {
                operand_register_uses(value, uses);
            }
        }
    }
}

fn terminator_targets(kind: &TerminatorKind, targets: &mut Vec<BlockId>) {
    match kind {
        TerminatorKind::Jump { target }
        | TerminatorKind::JumpIfFalse { target, .. }
        | TerminatorKind::JumpIfTrue { target, .. } => targets.push(*target),
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => {
            targets.push(*if_true);
            targets.push(*if_false);
        }
        TerminatorKind::Return { .. } | TerminatorKind::Exit { .. } => {}
    }
}

fn call_args_register_uses(args: &[IrCallArg], uses: &mut Vec<RegId>) {
    for arg in args {
        operand_register_uses(&arg.value, uses);
    }
}

fn operand_register_uses(operand: &Operand, uses: &mut Vec<RegId>) {
    if let Operand::Register(register) = operand {
        uses.push(*register);
    }
}

fn verify_block_id(id: BlockId, expected: usize, errors: &mut Vec<VerificationError>) {
    if id.index() != expected {
        errors.push(error(
            VerificationErrorCode::InvalidBlockId,
            format!("block table entry {expected} has id {}", id.raw()),
        ));
    }
}

fn verify_block_target(id: BlockId, function: &IrFunction, errors: &mut Vec<VerificationError>) {
    if id.index() >= function.blocks.len() {
        errors.push(error(
            VerificationErrorCode::InvalidBlockTarget,
            format!("target block {} is not defined", id.raw()),
        ));
    }
}

fn verify_register(id: RegId, register_count: u32, errors: &mut Vec<VerificationError>) {
    if id.raw() >= register_count {
        errors.push(error(
            VerificationErrorCode::InvalidRegId,
            format!("register {} exceeds count {register_count}", id.raw()),
        ));
    }
}

fn verify_function_id(
    id: crate::ids::FunctionId,
    function_count: usize,
    errors: &mut Vec<VerificationError>,
) {
    if id.index() >= function_count {
        errors.push(error(
            VerificationErrorCode::InvalidFunctionId,
            format!("function {} is not defined", id.raw()),
        ));
    }
}

fn verify_local(id: LocalId, local_count: u32, errors: &mut Vec<VerificationError>) {
    if id.raw() >= local_count {
        errors.push(error(
            VerificationErrorCode::InvalidLocalId,
            format!("local {} exceeds count {local_count}", id.raw()),
        ));
    }
}

fn verify_constant(id: ConstId, constant_count: usize, errors: &mut Vec<VerificationError>) {
    if id.index() >= constant_count {
        errors.push(error(
            VerificationErrorCode::InvalidConstId,
            format!("constant {} is not defined", id.raw()),
        ));
    }
}

fn error(code: VerificationErrorCode, message: String) -> VerificationError {
    VerificationError { code, message }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BasicBlock;
    use crate::builder::IrBuilder;
    use crate::constants::IrConstant;
    use crate::function::FunctionFlags;
    use crate::ids::{FileId, FunctionId, InstrId, UnitId};
    use crate::instruction::{BinaryOp, InstructionKind, IrCallArg};

    #[test]
    fn verifier_accepts_basic_unit() {
        let unit = valid_unit();
        verify_unit(&unit).expect("valid unit should verify");
    }

    #[test]
    fn verifier_accepts_identity_optimizer_boundary() {
        let unit = valid_unit();
        verify_unit(&unit).expect("pre-optimizer unit should verify");
        let optimized = unit.clone();
        verify_unit(&optimized).expect("post-optimizer unit should verify");
    }

    #[test]
    fn verifier_rejects_missing_terminator() {
        let mut unit = valid_unit();
        unit.functions[0].blocks[0].terminator = None;
        assert_has_error(&unit, VerificationErrorCode::MissingTerminator);
    }

    #[test]
    fn verifier_rejects_invalid_const_register_block_and_span() {
        let mut unit = valid_unit();
        unit.functions[0].blocks[0].instructions[0].kind = InstructionKind::LoadConst {
            dst: RegId::new(99),
            constant: ConstId::new(99),
        };
        unit.functions[0].blocks[0].instructions[0].span = IrSpan::new(FileId::new(99), 7, 1);
        unit.functions[0].blocks[0].terminator = Some(Terminator {
            span: IrSpan::new(FileId::new(0), 0, 1),
            kind: TerminatorKind::Jump {
                target: BlockId::new(99),
            },
        });
        let errors = verify_unit(&unit).expect_err("unit should fail verification");
        assert!(
            errors
                .iter()
                .any(|error| error.code == VerificationErrorCode::InvalidRegId)
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == VerificationErrorCode::InvalidConstId)
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == VerificationErrorCode::InvalidSpan)
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == VerificationErrorCode::InvalidBlockTarget)
        );
    }

    #[test]
    fn verifier_rejects_invalid_entry_and_local() {
        let mut unit = valid_unit();
        unit.entry = FunctionId::new(99);
        unit.functions[0].blocks[0].instructions.push(Instruction {
            id: InstrId::new(1),
            span: IrSpan::new(FileId::new(0), 0, 1),
            kind: InstructionKind::StoreLocal {
                local: LocalId::new(99),
                src: Operand::Register(RegId::new(0)),
            },
        });
        let errors = verify_unit(&unit).expect_err("unit should fail verification");
        assert!(
            errors
                .iter()
                .any(|error| { error.code == VerificationErrorCode::InvalidEntryFunction })
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == VerificationErrorCode::InvalidLocalId)
        );
    }

    #[test]
    fn verifier_rejects_register_use_before_definition() {
        let mut unit = valid_unit();
        unit.functions[0].blocks[0].instructions[0].kind = InstructionKind::Binary {
            dst: RegId::new(0),
            op: BinaryOp::Add,
            lhs: Operand::Register(RegId::new(0)),
            rhs: Operand::Constant(ConstId::new(0)),
        };
        unit.functions[0].blocks[0].terminator = Some(Terminator {
            span: IrSpan::new(FileId::new(0), 6, 7),
            kind: TerminatorKind::Return {
                value: None,
                by_ref_local: None,
            },
        });
        let errors = verify_unit(&unit).expect_err("unit should fail verification");
        let undefined = errors
            .iter()
            .find(|error| error.code == VerificationErrorCode::UndefinedRegisterUse)
            .expect("undefined register use should be reported");
        assert_eq!(
            undefined.diagnostic_id(),
            "E_PHP_IR_VERIFY_UNDEFINED_REGISTER_USE"
        );
    }

    #[test]
    fn verifier_accepts_register_defined_on_all_predecessors() {
        let mut unit = valid_unit();
        unit.functions[0].blocks[0].terminator = Some(Terminator {
            span: IrSpan::new(FileId::new(0), 6, 7),
            kind: TerminatorKind::Jump {
                target: BlockId::new(1),
            },
        });
        let mut block = BasicBlock::new(BlockId::new(1));
        block.terminator = Some(Terminator {
            span: IrSpan::new(FileId::new(0), 6, 7),
            kind: TerminatorKind::Return {
                value: Some(Operand::Register(RegId::new(0))),
                by_ref_local: None,
            },
        });
        unit.functions[0].blocks.push(block);
        verify_unit(&unit).expect("register should be defined on the only incoming edge");
    }

    #[test]
    fn verifier_rejects_foreach_value_use_on_false_edge() {
        let mut unit = valid_unit();
        unit.functions[0].register_count = 4;
        unit.functions[0].blocks[0].instructions.push(Instruction {
            id: InstrId::new(1),
            span: IrSpan::new(FileId::new(0), 7, 8),
            kind: InstructionKind::ForeachNext {
                has_value: RegId::new(1),
                iterator: RegId::new(0),
                key: None,
                value: RegId::new(2),
            },
        });
        unit.functions[0].blocks[0].terminator = Some(Terminator {
            span: IrSpan::new(FileId::new(0), 8, 9),
            kind: TerminatorKind::JumpIf {
                condition: Operand::Register(RegId::new(1)),
                if_true: BlockId::new(1),
                if_false: BlockId::new(2),
            },
        });
        let mut true_block = BasicBlock::new(BlockId::new(1));
        true_block.terminator = Some(Terminator {
            span: IrSpan::new(FileId::new(0), 8, 9),
            kind: TerminatorKind::Return {
                value: Some(Operand::Register(RegId::new(2))),
                by_ref_local: None,
            },
        });
        let mut false_block = BasicBlock::new(BlockId::new(2));
        false_block.terminator = Some(Terminator {
            span: IrSpan::new(FileId::new(0), 8, 9),
            kind: TerminatorKind::Return {
                value: Some(Operand::Register(RegId::new(2))),
                by_ref_local: None,
            },
        });
        unit.functions[0].blocks.push(true_block);
        unit.functions[0].blocks.push(false_block);
        assert_has_error(&unit, VerificationErrorCode::UndefinedRegisterUse);
    }

    #[test]
    fn verifier_rejects_inconsistent_call_argument_metadata() {
        let mut unit = valid_unit();
        unit.functions[0].locals.push("x".to_string());
        unit.functions[0].local_count = 1;
        unit.functions[0].blocks[0].instructions.push(Instruction {
            id: InstrId::new(1),
            span: IrSpan::new(FileId::new(0), 7, 8),
            kind: InstructionKind::CallFunction {
                dst: RegId::new(0),
                name: "f".to_string(),
                args: vec![IrCallArg {
                    name: None,
                    value: Operand::Register(RegId::new(0)),
                    unpack: true,
                    value_kind: crate::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: Some(LocalId::new(0)),
                    by_ref_dim: None,
                    by_ref_property: None,
                }],
            },
        });
        assert_has_error(&unit, VerificationErrorCode::InvalidCallArgMetadata);
    }

    #[test]
    fn verifier_failure_has_shared_envelope_context() {
        let error = VerificationError {
            code: VerificationErrorCode::MissingTerminator,
            message: "block 2 is missing a terminator".to_string(),
        };
        let context = VerificationDiagnosticContext {
            source_path: Some("verify.php".to_string()),
            source_id: Some("file:0".to_string()),
            function: Some(crate::ids::FunctionId::new(1)),
            block: Some(BlockId::new(2)),
            instruction: Some(InstrId::new(3)),
            source_span: Some(IrSpan::new(FileId::new(0), 20, 24)),
        };

        let envelope = error.to_diagnostic_envelope(&context);
        let json: serde_json::Value =
            serde_json::from_str(&envelope.compact_json().expect("json")).expect("parse json");

        assert_eq!(json["code"], "E_PHP_IR_VERIFY_MISSING_TERMINATOR");
        assert_eq!(json["layer"], "ir");
        assert_eq!(json["phase"], "verify");
        assert_eq!(json["location"]["path"], "verify.php");
        assert_eq!(json["context"]["function_id"], "1");
        assert_eq!(json["context"]["block_id"], "2");
        assert_eq!(json["context"]["instruction_id"], "3");
    }

    fn assert_has_error(unit: &IrUnit, code: VerificationErrorCode) {
        let errors = verify_unit(unit).expect_err("unit should fail verification");
        assert!(errors.iter().any(|error| error.code == code), "{errors:#?}");
    }

    fn valid_unit() -> IrUnit {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("valid.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let block = builder.append_block(function);
        let constant = builder.add_constant(IrConstant::Int(1));
        let register = builder.alloc_register(function);
        builder.emit_load_const(function, block, register, constant, IrSpan::new(file, 6, 7));
        builder.terminate_return(
            function,
            block,
            Some(Operand::Register(register)),
            IrSpan::new(file, 6, 7),
        );
        builder.set_entry(function);
        builder.finish()
    }
}
