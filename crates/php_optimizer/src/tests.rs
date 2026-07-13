use super::{
    ConstantFoldingPass, CopyPropagationPass, LiteralCompactionPass, NoopPass, OptimizationLevel,
    OptimizerPass, PassContext, PassError, PassPhase, PassPipeline, PassReport, PassTransaction,
    PeepholeSimplify,
};
use php_ir::instruction::TerminatorKind;
use php_ir::{
    BinaryOp, CompareOp, FunctionFlags, InstructionKind, IrBuilder, IrConstant, IrSpan, Operand,
    UnaryOp, UnitId, VerificationError, VerificationErrorCode,
};
use std::collections::BTreeMap;

fn simple_unit() -> php_ir::IrUnit {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("optimizer/noop.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 5),
    );
    let block = builder.append_block(function);
    let constant = builder.add_constant(IrConstant::String("noop".to_string()));
    let register = builder.alloc_register(function);
    builder.emit_load_const(
        function,
        block,
        register,
        constant,
        IrSpan::new(file, 6, 12),
    );
    builder.terminate_return(
        function,
        block,
        Some(Operand::Register(register)),
        IrSpan::new(file, 6, 12),
    );
    builder.set_entry(function);
    builder.finish()
}

fn folding_unit(kind: InstructionKind) -> php_ir::IrUnit {
    let mut builder = IrBuilder::new(UnitId::new(1));
    let file = builder.add_file("optimizer/folding.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 5),
    );
    let block = builder.append_block(function);
    let _register = builder.alloc_register(function);
    builder.emit(function, block, kind, IrSpan::new(file, 6, 12));
    builder.terminate_return(function, block, None, IrSpan::new(file, 13, 14));
    builder.set_entry(function);
    builder.finish()
}

fn constant(unit: &php_ir::IrUnit, index: usize) -> &IrConstant {
    &unit.constants[index]
}

#[test]
fn optimization_levels_parse_stable_cli_values() {
    assert_eq!("0".parse(), Ok(OptimizationLevel::O0));
    assert_eq!("1".parse(), Ok(OptimizationLevel::O1));
    assert_eq!("2".parse(), Ok(OptimizationLevel::O2));
    assert!("3".parse::<OptimizationLevel>().is_err());
    assert_eq!(OptimizationLevel::O1.as_str(), "1");
    assert!(OptimizationLevel::O0 < OptimizationLevel::O1);
}

#[test]
fn optimizer_verifier_failure_has_shared_envelope_context() {
    let error = PassError::Verification {
        phase: PassPhase::PostVerify,
        errors: vec![VerificationError {
            code: VerificationErrorCode::InvalidBlockId,
            message: "block id mismatch".to_string(),
        }],
    };

    let envelopes =
        error.to_diagnostic_envelopes(OptimizationLevel::O2, Some("unit.php"), Some("main"));
    let json: serde_json::Value = serde_json::from_str(
        &envelopes
            .first()
            .expect("one envelope")
            .compact_json()
            .expect("json"),
    )
    .expect("parse json");

    assert_eq!(json["code"], "E_PHP_IR_VERIFY_INVALID_BLOCK_ID");
    assert_eq!(json["layer"], "optimizer");
    assert_eq!(json["phase"], "verify_post_verify");
    assert_eq!(json["context"]["optimization_level"], "2");
    assert_eq!(json["context"]["optimizer_phase"], "post_verify");
    assert_eq!(json["context"]["unit"], "unit.php");
    assert_eq!(json["context"]["function"], "main");
}

#[test]
fn noop_pipeline_reports_without_changing_ir_or_spans() {
    let mut unit = simple_unit();
    let before = unit.clone();
    let report = PassPipeline::noop()
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("noop pipeline should pass");

    assert_eq!(unit, before);
    assert_eq!(report.level, OptimizationLevel::O1);
    assert_eq!(report.enabled_pass_count(), 2);
    assert_eq!(report.passes.len(), 2);
    assert!(report.passes.iter().all(|pass| !pass.changed));
    assert!(report.passes.iter().all(|pass| pass.source_spans_preserved));
    assert_eq!(report.passes[0].phase, PassPhase::PreVerify);
    assert_eq!(report.passes[1].phase, PassPhase::PostVerify);
    assert_eq!(report.passes[0].stats["functions"], 1);
    assert!(
        report
            .passes
            .iter()
            .all(|pass| pass.stats["scope_snapshots"] == 0)
    );
    assert!(
        report
            .passes
            .iter()
            .all(|pass| pass.stats["snapshot_bytes"] == 0)
    );
    assert!(
        report
            .passes
            .iter()
            .all(|pass| pass.stats["verifier_calls"] == 0)
    );
    assert!(
        report
            .passes
            .iter()
            .all(|pass| pass.scope == super::PassScopeReport::default())
    );
}

#[test]
fn passes_can_be_individually_disabled_or_enabled() {
    let mut unit = simple_unit();
    let report = PassPipeline::noop()
        .run(
            &mut unit,
            &PassContext::new(OptimizationLevel::O1).with_disabled(["perf_post_verify_noop"]),
        )
        .expect("disabled pass should be skipped");

    assert_eq!(report.enabled_pass_count(), 1);
    assert!(report.passes[0].enabled);
    assert!(!report.passes[1].enabled);

    let mut unit = simple_unit();
    let report = PassPipeline::noop()
        .run(
            &mut unit,
            &PassContext::new(OptimizationLevel::O1).with_enabled_only(["perf_post_verify_noop"]),
        )
        .expect("enabled-only pass should run");

    assert_eq!(report.enabled_pass_count(), 1);
    assert!(!report.passes[0].enabled);
    assert!(report.passes[1].enabled);
}

#[test]
fn level_zero_context_skips_noop_passes() {
    let mut unit = simple_unit();
    let report = PassPipeline::noop()
        .run(&mut unit, &PassContext::new(OptimizationLevel::O0))
        .expect("level zero still verifies");

    assert_eq!(report.enabled_pass_count(), 0);
    assert_eq!(report.passes.len(), 2);
}

#[test]
fn direct_noop_pass_preserves_unit() {
    let mut unit = simple_unit();
    let before = unit.clone();
    let report = NoopPass::new("direct_noop", PassPhase::PreVerify)
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("noop pass should pass");

    assert_eq!(unit, before);
    assert!(report.enabled);
    assert!(!report.changed);
    assert!(report.source_spans_preserved);
}

#[test]
fn verifier_failing_pass_is_rolled_back_and_reported() {
    struct CorruptingPass;
    impl OptimizerPass for CorruptingPass {
        fn name(&self) -> &'static str {
            "test_corrupting_pass"
        }
        fn phase(&self) -> PassPhase {
            PassPhase::PreVerify
        }
        fn run(
            &self,
            transaction: &mut PassTransaction<'_>,
            _context: &PassContext,
        ) -> Result<PassReport, PassError> {
            // Reference an out-of-range register so the verifier rejects
            // the pass result.
            if let Some(function) = transaction.unit().functions.first()
                && !function.blocks.is_empty()
            {
                let function = transaction.function_mut(0);
                let block = &mut function.blocks[0];
                if let Some(instruction) = block.instructions.first_mut()
                    && let InstructionKind::LoadConst { dst, .. } = &mut instruction.kind
                {
                    *dst = php_ir::RegId::new(4096);
                }
                transaction.touch_block(0, 0);
            }
            let mut stats = BTreeMap::new();
            stats.insert("corruptions", 1);
            Ok(PassReport {
                name: self.name(),
                phase: self.phase(),
                enabled: true,
                changed: true,
                source_spans_preserved: true,
                rolled_back: false,
                scope: super::PassScopeReport::default(),
                stats,
            })
        }
    }

    let mut unit = simple_unit();
    let before = unit.clone();
    let report = PassPipeline::new(vec![Box::new(CorruptingPass)])
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("rollback keeps the pipeline fail-safe");

    assert_eq!(unit, before, "corrupted pass output must be rolled back");
    let pass = &report.passes[0];
    assert_eq!(pass.name, "test_corrupting_pass");
    assert!(pass.rolled_back);
    assert!(!pass.changed);
    assert!(pass.stats["verifier_errors"] >= 1);
    assert_eq!(pass.stats["functions_touched"], 1);
    assert_eq!(pass.stats["scope_snapshots"], 1);
    assert!(pass.stats["snapshot_bytes"] > 0);
    assert_eq!(pass.scope.functions, vec![0]);
    assert_eq!(pass.scope.blocks, vec![(0, 0)]);
}

#[test]
fn pass_error_before_commit_rolls_back_touched_scope() {
    struct FailingPass;
    impl OptimizerPass for FailingPass {
        fn name(&self) -> &'static str {
            "test_failing_pass"
        }

        fn phase(&self) -> PassPhase {
            PassPhase::PreVerify
        }

        fn run(
            &self,
            transaction: &mut PassTransaction<'_>,
            _context: &PassContext,
        ) -> Result<PassReport, PassError> {
            transaction.function_mut(0).name = "corrupted".to_owned();
            Err(PassError::PassFailed {
                pass: self.name(),
                message: "deliberate test failure".to_owned(),
            })
        }
    }

    let mut unit = simple_unit();
    let before = unit.clone();
    let error = PassPipeline::new(vec![Box::new(FailingPass)])
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect_err("pass error must abort the pipeline");

    assert!(matches!(error, PassError::PassFailed { .. }));
    assert_eq!(unit, before);
}

#[test]
fn constant_folding_snapshots_only_the_touched_function() {
    let mut unit = simple_unit();
    let untouched = unit.functions[0].clone();
    let mut candidate = folding_unit(InstructionKind::Binary {
        dst: php_ir::RegId::new(0),
        op: BinaryOp::Add,
        lhs: Operand::Constant(php_ir::ConstId::new(0)),
        rhs: Operand::Constant(php_ir::ConstId::new(1)),
    });
    unit.constants = vec![IrConstant::Int(20), IrConstant::Int(22)];
    unit.functions.push(candidate.functions.remove(0));

    let report = PassPipeline::new(vec![Box::new(ConstantFoldingPass)])
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("constant folding should verify");
    let pass = &report.passes[0];

    assert_eq!(unit.functions[0], untouched);
    assert!(pass.changed);
    assert_eq!(pass.stats["functions_touched"], 1);
    assert_eq!(pass.stats["blocks_touched"], 1);
    assert_eq!(pass.stats["constant_pool_touched"], 1);
    assert_eq!(pass.stats["scope_snapshots"], 2);
    assert!(pass.stats["snapshot_bytes"] > 0);
    assert_eq!(pass.stats["verifier_calls"], 1);
    assert_eq!(pass.scope.functions, vec![1]);
    assert_eq!(pass.scope.blocks, vec![(1, 0)]);
    assert!(pass.scope.constants);
    assert!(pass.scope.metadata.is_empty());
    assert!(!pass.scope.source_mappings_may_change);
}

#[test]
fn optimizer_reports_are_deterministic_for_identical_units() {
    let context = PassContext::new(OptimizationLevel::O1);
    let mut first = simple_unit();
    let mut second = first.clone();

    let first_report = PassPipeline::performance()
        .run(&mut first, &context)
        .expect("first optimizer run");
    let second_report = PassPipeline::performance()
        .run(&mut second, &context)
        .expect("second optimizer run");

    assert_eq!(first, second);
    assert_eq!(first_report, second_report);
}

#[test]
fn perf_pipeline_runs_constant_folding_between_verifiers() {
    let mut unit = simple_unit();
    let report = PassPipeline::performance()
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("performance pipeline should pass");

    assert_eq!(report.enabled_pass_count(), 7);
    assert_eq!(report.passes[1].name, "constant_folding_safe_subset");
    assert_eq!(report.passes[1].phase, PassPhase::PreVerify);
    assert_eq!(report.passes[1].stats["total_folded"], 0);
    assert_eq!(report.passes[2].name, "literal_compaction");
    assert_eq!(report.passes[2].stats["duplicates_removed"], 0);
    assert_eq!(report.passes[3].name, "copy_propagation_register_subset");
    assert_eq!(report.passes[3].stats["operands_rewritten"], 0);
    assert_eq!(report.passes[4].name, "peephole_simplify");
    assert_eq!(report.passes[4].stats["total_transformations"], 0);
    assert_eq!(report.passes[5].name, "branch_simplify");
    assert_eq!(report.passes[5].stats["total_transformations"], 0);
}

#[test]
fn folds_safe_integer_binary_without_overflow() {
    let mut unit = folding_unit(InstructionKind::Binary {
        dst: php_ir::RegId::new(0),
        op: BinaryOp::Mul,
        lhs: Operand::Constant(php_ir::ConstId::new(0)),
        rhs: Operand::Constant(php_ir::ConstId::new(1)),
    });
    unit.constants = vec![IrConstant::Int(6), IrConstant::Int(7)];

    let report = ConstantFoldingPass
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("folding should pass");

    assert!(report.changed);
    assert_eq!(report.stats["integer_binary_folded"], 1);
    assert_eq!(constant(&unit, 2), &IrConstant::Int(42));
    assert!(matches!(
        unit.functions[0].blocks[0].instructions[0].kind,
        InstructionKind::LoadConst {
            constant,
            ..
        } if constant == php_ir::ConstId::new(2)
    ));
}

#[test]
fn folds_bool_not_and_string_concat() {
    let mut unit = folding_unit(InstructionKind::Unary {
        dst: php_ir::RegId::new(0),
        op: UnaryOp::Not,
        src: Operand::Constant(php_ir::ConstId::new(0)),
    });
    unit.constants = vec![IrConstant::Bool(false)];

    let report = ConstantFoldingPass
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("bool not should fold");
    assert_eq!(report.stats["bool_not_folded"], 1);
    assert_eq!(constant(&unit, 1), &IrConstant::Bool(true));

    let mut unit = folding_unit(InstructionKind::Binary {
        dst: php_ir::RegId::new(0),
        op: BinaryOp::Concat,
        lhs: Operand::Constant(php_ir::ConstId::new(0)),
        rhs: Operand::Constant(php_ir::ConstId::new(1)),
    });
    unit.constants = vec![
        IrConstant::String("php".to_string()),
        IrConstant::String("-vm".to_string()),
    ];

    let report = ConstantFoldingPass
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("string concat should fold");
    assert_eq!(report.stats["string_concat_folded"], 1);
    assert_eq!(
        constant(&unit, 2),
        &IrConstant::String("php-vm".to_string())
    );
}

#[test]
fn folds_literal_compare_safe_subset() {
    let mut unit = folding_unit(InstructionKind::Compare {
        dst: php_ir::RegId::new(0),
        op: CompareOp::Less,
        lhs: Operand::Constant(php_ir::ConstId::new(0)),
        rhs: Operand::Constant(php_ir::ConstId::new(1)),
    });
    unit.constants = vec![IrConstant::Int(3), IrConstant::Int(5)];

    let report = ConstantFoldingPass
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("literal int comparison should fold");

    assert!(report.changed);
    assert_eq!(report.stats["literal_compare_folded"], 1);
    assert_eq!(constant(&unit, 2), &IrConstant::Bool(true));
    assert!(matches!(
        unit.functions[0].blocks[0].instructions[0].kind,
        InstructionKind::LoadConst {
            constant,
            ..
        } if constant == php_ir::ConstId::new(2)
    ));

    let mut unit = folding_unit(InstructionKind::Compare {
        dst: php_ir::RegId::new(0),
        op: CompareOp::Spaceship,
        lhs: Operand::Constant(php_ir::ConstId::new(0)),
        rhs: Operand::Constant(php_ir::ConstId::new(1)),
    });
    unit.constants = vec![IrConstant::Int(3), IrConstant::Int(5)];

    let report = ConstantFoldingPass
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("literal int spaceship should fold");

    assert_eq!(report.stats["literal_compare_folded"], 1);
    assert_eq!(constant(&unit, 2), &IrConstant::Int(-1));
}

#[test]
fn skips_compare_folds_that_can_hide_php_semantics() {
    for (op, lhs, rhs) in [
        (
            CompareOp::Equal,
            IrConstant::String("01".to_string()),
            IrConstant::String("1".to_string()),
        ),
        (
            CompareOp::Less,
            IrConstant::String("2".to_string()),
            IrConstant::Int(10),
        ),
        (
            CompareOp::Spaceship,
            IrConstant::Float(1.0),
            IrConstant::Float(1.0),
        ),
    ] {
        let mut unit = folding_unit(InstructionKind::Compare {
            dst: php_ir::RegId::new(0),
            op,
            lhs: Operand::Constant(php_ir::ConstId::new(0)),
            rhs: Operand::Constant(php_ir::ConstId::new(1)),
        });
        unit.constants = vec![lhs, rhs];
        let before = unit.clone();

        let report = ConstantFoldingPass
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("unsafe compare fold should be skipped");

        assert_eq!(unit, before);
        assert!(!report.changed);
        assert_eq!(report.stats["literal_compare_folded"], 0);
        assert_eq!(report.stats["skipped_unsafe"], 1);
    }
}

#[test]
fn refuses_unsafe_or_observable_folds() {
    for (op, lhs, rhs) in [
        (BinaryOp::Add, IrConstant::Int(i64::MAX), IrConstant::Int(1)),
        (BinaryOp::Div, IrConstant::Int(6), IrConstant::Int(3)),
        (BinaryOp::Mod, IrConstant::Int(6), IrConstant::Int(3)),
        (
            BinaryOp::Add,
            IrConstant::String("1".to_string()),
            IrConstant::Int(2),
        ),
    ] {
        let mut unit = folding_unit(InstructionKind::Binary {
            dst: php_ir::RegId::new(0),
            op,
            lhs: Operand::Constant(php_ir::ConstId::new(0)),
            rhs: Operand::Constant(php_ir::ConstId::new(1)),
        });
        unit.constants = vec![lhs, rhs];
        let before = unit.clone();

        let report = ConstantFoldingPass
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("unsafe fold should be skipped");

        assert_eq!(unit, before);
        assert!(!report.changed);
        assert_eq!(report.stats["total_folded"], 0);
        assert_eq!(report.stats["skipped_unsafe"], 1);
    }
}

#[test]
fn preserves_source_maps_and_does_not_fold_non_bool_not() {
    let mut unit = folding_unit(InstructionKind::Unary {
        dst: php_ir::RegId::new(0),
        op: UnaryOp::Not,
        src: Operand::Constant(php_ir::ConstId::new(0)),
    });
    unit.constants = vec![IrConstant::Int(0)];
    let before_files = unit.files.clone();
    let before_source_map = unit.source_map.clone();

    let report = ConstantFoldingPass
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("non-bool not should be skipped");

    assert!(!report.changed);
    assert!(report.source_spans_preserved);
    assert_eq!(unit.files, before_files);
    assert_eq!(unit.source_map, before_source_map);
    assert_eq!(report.stats["skipped_unsafe"], 1);
}

#[test]
fn literal_compaction_remaps_duplicate_constants() {
    let mut builder = IrBuilder::new(UnitId::new(20));
    let file = builder.add_file("optimizer/literals.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 5),
    );
    let block = builder.append_block(function);
    let first = builder.add_constant(IrConstant::String("same".to_string()));
    let second = builder.add_constant(IrConstant::String("same".to_string()));
    let register = builder.alloc_register(function);
    builder.emit_load_const(function, block, register, second, IrSpan::new(file, 6, 10));
    builder.terminate_return(
        function,
        block,
        Some(Operand::Constant(second)),
        IrSpan::new(file, 11, 12),
    );
    builder.set_entry(function);
    let mut unit = builder.finish();

    let report = LiteralCompactionPass
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("literal compaction should verify");

    assert!(report.changed);
    assert_eq!(report.stats["duplicates_removed"], 1);
    assert_eq!(unit.constants.len(), 1);
    assert!(matches!(
        unit.functions[0].blocks[0].instructions[0].kind,
        InstructionKind::LoadConst {
            constant,
            ..
        } if constant == first
    ));
    assert!(matches!(
        unit.functions[0].blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::Return {
            value: Some(Operand::Constant(constant)),
            ..
        } if constant == first
    ));
}

#[test]
fn copy_propagation_rewrites_register_sources_within_block() {
    let mut builder = IrBuilder::new(UnitId::new(21));
    let file = builder.add_file("optimizer/copy-prop.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 5),
    );
    let block = builder.append_block(function);
    let constant = builder.add_constant(IrConstant::String("copy".to_string()));
    let source = builder.alloc_register(function);
    let copy = builder.alloc_register(function);
    builder.emit_load_const(function, block, source, constant, IrSpan::new(file, 6, 10));
    builder.emit(
        function,
        block,
        InstructionKind::Move {
            dst: copy,
            src: Operand::Register(source),
        },
        IrSpan::new(file, 11, 12),
    );
    builder.emit(
        function,
        block,
        InstructionKind::Echo {
            src: Operand::Register(copy),
        },
        IrSpan::new(file, 13, 14),
    );
    builder.terminate_return(
        function,
        block,
        Some(Operand::Register(copy)),
        IrSpan::new(file, 15, 16),
    );
    builder.set_entry(function);
    let mut unit = builder.finish();

    let report = CopyPropagationPass
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("copy propagation should verify");

    assert!(report.changed);
    assert_eq!(report.stats["moves_considered"], 1);
    assert_eq!(report.stats["aliases_recorded"], 1);
    assert_eq!(report.stats["operands_rewritten"], 2);
    assert!(matches!(
        unit.functions[0].blocks[0].instructions[2].kind,
        InstructionKind::Echo {
            src: Operand::Register(register)
        } if register == source
    ));
    assert!(matches!(
        unit.functions[0].blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::Return {
            value: Some(Operand::Register(register)),
            ..
        } if register == source
    ));
}

#[test]
fn peephole_removes_nop_and_self_move_with_snapshot() {
    let mut builder = IrBuilder::new(UnitId::new(2));
    let file = builder.add_file("optimizer/peephole.php");
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
    builder.emit(
        function,
        block,
        InstructionKind::Nop,
        IrSpan::new(file, 6, 7),
    );
    builder.emit_load_const(function, block, register, constant, IrSpan::new(file, 8, 9));
    builder.emit(
        function,
        block,
        InstructionKind::Move {
            dst: register,
            src: Operand::Register(register),
        },
        IrSpan::new(file, 10, 11),
    );
    builder.terminate_return(
        function,
        block,
        Some(Operand::Register(register)),
        IrSpan::new(file, 12, 13),
    );
    builder.set_entry(function);
    let mut unit = builder.finish();
    let before = format!("{unit}");

    let report = PeepholeSimplify
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("peephole pass should verify after each transform");
    let after = format!("{unit}");

    assert!(before.contains("nop"));
    assert!(before.contains("move"));
    assert!(!after.contains("nop"));
    assert!(!after.contains("move"));
    assert_eq!(report.stats["noops_removed"], 1);
    assert_eq!(report.stats["self_moves_removed"], 1);
    assert_eq!(report.stats["total_transformations"], 2);
    assert!(report.source_spans_preserved);
}

#[test]
fn peephole_keeps_effectful_and_register_defining_moves() {
    let mut builder = IrBuilder::new(UnitId::new(3));
    let file = builder.add_file("optimizer/no-peephole.php");
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
    let source = builder.alloc_register(function);
    let target = builder.alloc_register(function);
    builder.emit_load_const(function, block, source, constant, IrSpan::new(file, 6, 7));
    builder.emit(
        function,
        block,
        InstructionKind::Move {
            dst: target,
            src: Operand::Register(source),
        },
        IrSpan::new(file, 8, 9),
    );
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: source,
            name: "side_effect".to_string(),
            args: Vec::new(),
        },
        IrSpan::new(file, 10, 11),
    );
    builder.terminate_return(
        function,
        block,
        Some(Operand::Register(target)),
        IrSpan::new(file, 12, 13),
    );
    builder.set_entry(function);
    let mut unit = builder.finish();
    let before = unit.clone();

    let report = PeepholeSimplify
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("negative peepholes should pass");

    assert_eq!(unit, before);
    assert!(!report.changed);
    assert_eq!(report.stats["total_transformations"], 0);
}

#[test]
fn branch_simplify_rewrites_constant_jump_if_snapshot() {
    let mut builder = IrBuilder::new(UnitId::new(4));
    let file = builder.add_file("optimizer/branch.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 5),
    );
    let entry = builder.append_block(function);
    let true_block = builder.append_block(function);
    let false_block = builder.append_block(function);
    let condition = builder.add_constant(IrConstant::Bool(true));
    builder.terminate_jump_if(
        function,
        entry,
        Operand::Constant(condition),
        true_block,
        false_block,
        IrSpan::new(file, 6, 10),
    );
    builder.terminate_return(function, true_block, None, IrSpan::new(file, 11, 12));
    builder.terminate_return(function, false_block, None, IrSpan::new(file, 13, 14));
    builder.set_entry(function);
    let mut unit = builder.finish();
    let before = format!("{unit}");

    let report = super::BranchSimplify
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("constant branch should simplify");
    let after = format!("{unit}");

    assert!(before.contains("jump_if"));
    assert!(after.contains("jump block:1"));
    assert_eq!(report.stats["constant_branches"], 1);
    assert_eq!(report.stats["unreachable_empty_tail_blocks_removed"], 1);
    assert_eq!(report.stats["total_transformations"], 2);
    assert!(report.source_spans_preserved);
}

#[test]
fn branch_simplify_uses_cfg_fallthrough_for_loaded_bool_conditions() {
    let mut builder = IrBuilder::new(UnitId::new(5));
    let file = builder.add_file("optimizer/fallthrough.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 5),
    );
    let entry = builder.append_block(function);
    let fallthrough = builder.append_block(function);
    let false_target = builder.append_block(function);
    let condition = builder.add_constant(IrConstant::Bool(true));
    let register = builder.alloc_register(function);
    builder.emit_load_const(
        function,
        entry,
        register,
        condition,
        IrSpan::new(file, 6, 7),
    );
    builder.terminate_jump_if_false(
        function,
        entry,
        Operand::Register(register),
        false_target,
        IrSpan::new(file, 8, 9),
    );
    builder.terminate_return(function, fallthrough, None, IrSpan::new(file, 10, 11));
    builder.terminate_return(function, false_target, None, IrSpan::new(file, 12, 13));
    builder.set_entry(function);
    let mut unit = builder.finish();

    let report = super::BranchSimplify
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("loaded bool branch should simplify to fallthrough jump");

    assert!(matches!(
        unit.functions[0].blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::Jump { target } if target == fallthrough
    ));
    assert_eq!(report.stats["constant_branches"], 1);
}

#[test]
fn branch_simplify_forwards_empty_blocks_and_truncates_empty_unreachable_tail() {
    let mut builder = IrBuilder::new(UnitId::new(6));
    let file = builder.add_file("optimizer/empty-block.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 5),
    );
    let entry = builder.append_block(function);
    let forwarding = builder.append_block(function);
    let target = builder.append_block(function);
    let tail = builder.append_block(function);
    builder.terminate_jump(function, entry, forwarding, IrSpan::new(file, 6, 7));
    builder.terminate_jump(function, forwarding, target, IrSpan::new(file, 8, 9));
    builder.terminate_return(function, target, None, IrSpan::new(file, 10, 11));
    builder.terminate_return(function, tail, None, IrSpan::new(file, 12, 13));
    builder.set_entry(function);
    let mut unit = builder.finish();

    let report = super::BranchSimplify
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("empty block CFG simplifications should verify");

    assert!(matches!(
        unit.functions[0].blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::Jump { target: rewritten } if rewritten == target
    ));
    assert_eq!(unit.functions[0].blocks.len(), 3);
    assert_eq!(report.stats["empty_block_forwards"], 1);
    assert_eq!(report.stats["unreachable_empty_tail_blocks_removed"], 1);
}

#[test]
fn branch_simplify_keeps_non_bool_and_exception_boundary_blocks() {
    let mut builder = IrBuilder::new(UnitId::new(7));
    let file = builder.add_file("optimizer/no-branch.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 5),
    );
    let entry = builder.append_block(function);
    let target = builder.append_block(function);
    let fallback = builder.append_block(function);
    let after = builder.append_block(function);
    let condition = builder.add_constant(IrConstant::Int(1));
    let register = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::EnterTry {
            catch: None,
            catch_types: Vec::new(),
            finally: None,
            after,
            exception_local: None,
        },
        IrSpan::new(file, 6, 7),
    );
    builder.emit_load_const(
        function,
        entry,
        register,
        condition,
        IrSpan::new(file, 8, 9),
    );
    builder.terminate_jump_if(
        function,
        entry,
        Operand::Register(register),
        target,
        fallback,
        IrSpan::new(file, 10, 11),
    );
    builder.terminate_return(function, target, None, IrSpan::new(file, 12, 13));
    builder.terminate_return(function, fallback, None, IrSpan::new(file, 14, 15));
    builder.terminate_return(function, after, None, IrSpan::new(file, 16, 17));
    builder.set_entry(function);
    let mut unit = builder.finish();
    let before = unit.clone();

    let report = super::BranchSimplify
        .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
        .expect("unsafe branch simplifications should be skipped");

    assert_eq!(unit, before);
    assert!(!report.changed);
    assert_eq!(report.stats["total_transformations"], 0);
}
