//! runtime bytecode/IR boundary.
//!
//! The IR is register-based and organized into basic blocks. It is the stable
//! handoff shape between Semantic frontend HIR and the runtime VM. This crate does not
//! lower HIR yet.

/// Monotonic revision of the IR lowering output shape.
///
/// Bump this whenever lowering starts emitting different (still-compatible)
/// IR for the same source, so content-addressed native caches recompile
/// instead of serving the older lowering forever.
pub const IR_LOWERING_REVISION: u32 = 5;

pub mod block;
pub mod builder;
pub mod compilation;
pub mod constants;
pub mod display;
pub mod function;
pub mod ids;
pub mod instruction;
mod literal_text;
pub mod lower;
pub mod module;
pub mod operand;
pub mod rule_selection;
pub mod source_map;
pub mod verify;

pub use block::{BasicBlock, Terminator};
pub use builder::IrBuilder;
pub use compilation::{
    CompilationCycle, CompilationDependency, CompilationFileId, CompilationSession,
    CompilationSource, UnresolvedTraitRequest,
};
pub use constants::IrConstant;
pub use function::{FunctionFlags, IrCapture, IrFunction, IrParam, IrReturnType};
pub use ids::{BlockId, ClassId, ConstId, FileId, FunctionId, InstrId, LocalId, RegId, UnitId};
pub use instruction::{
    BinaryOp, CallableKind, CastKind, ClosureCaptureArg, CompareOp, IncludeKind, Instruction,
    InstructionKind, IrDiagnosticSeverity, UnaryOp,
};
pub use lower::{
    LoweringContext, LoweringDiagnostic, LoweringDiagnosticPayload, LoweringOptions,
    LoweringResult, MISSING_TRAIT_DIAGNOSTIC_CODE, MissingTraitDiagnostic, MissingTraitOwnerKind,
    UnsupportedFeature, lower_compilation_session, lower_frontend_result,
};
pub use module::{
    AttributeEntry, ClassEntry, ClassEnumBackingType, ClassEnumCaseEntry, ClassFlags,
    ClassMethodEntry, ClassMethodFlags, ClassPropertyEntry, ClassPropertyFlags, ClassPropertyHooks,
    FileEntry, FunctionEntry, GlobalConstantEntry, IR_VERSION, IrUnit, display_class_name,
    normalize_class_name,
};
pub use operand::Operand;
pub use rule_selection::{
    RuleId, RuleKind, RuleOperandConstraint, RuleSelection, RuleSelectionReport,
};
pub use source_map::{IrSourceMap, IrSourceMapEntry, IrSourceMapTarget, IrSpan};
pub use verify::{
    VerificationDiagnosticContext, VerificationError, VerificationErrorCode,
    instruction_register_defs, instruction_register_uses, terminator_register_uses,
    verify_function, verify_unit,
};

#[cfg(test)]
mod tests {
    use super::{
        BinaryOp, FunctionFlags, InstructionKind, IrBuilder, IrConstant, IrSpan, Operand, RegId,
        UnitId, verify_unit,
    };

    #[test]
    fn ids_are_stable_newtypes() {
        assert_eq!(UnitId::new(7).index(), 7);
        assert_eq!(RegId::new(3).index(), 3);
        assert_eq!(format!("{:?}", RegId::new(3)), "RegId(3)");
        assert_eq!(
            php_testkit::reference_checkout_path(),
            "third_party/php-src"
        );
    }

    #[test]
    fn builder_constructs_simple_ir() {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("fixtures/runtime/valid/scalars/echo.php");
        let entry = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let block = builder.append_block(entry);
        let c0 = builder.add_constant(IrConstant::Int(1));
        let c1 = builder.add_constant(IrConstant::Int(2));
        let r0 = builder.alloc_register(entry);
        let r1 = builder.alloc_register(entry);
        let r2 = builder.alloc_register(entry);
        builder.emit_load_const(entry, block, r0, c0, IrSpan::new(file, 6, 7));
        builder.emit_load_const(entry, block, r1, c1, IrSpan::new(file, 10, 11));
        builder.emit(
            entry,
            block,
            InstructionKind::Binary {
                dst: r2,
                op: BinaryOp::Add,
                lhs: Operand::Register(r0),
                rhs: Operand::Register(r1),
            },
            IrSpan::new(file, 6, 11),
        );
        builder.terminate_return(
            entry,
            block,
            Some(Operand::Register(r2)),
            IrSpan::new(file, 6, 11),
        );
        builder.set_entry(entry);

        let unit = builder.finish();
        verify_unit(&unit).expect("builder should create verifiable IR");
        assert_eq!(unit.entry, entry);
        assert_eq!(unit.constants.len(), 2);
        assert_eq!(unit.functions[entry.index()].register_count, 3);
        assert_eq!(
            unit.functions[entry.index()].blocks[block.index()]
                .instructions
                .len(),
            3
        );
        assert!(format!("{unit:#?}").contains("IrUnit"));
    }
}
