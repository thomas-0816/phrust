use php_ir::{
    FunctionFlags, InstructionKind, IrBuilder, IrConstant, IrSpan, LoweringOptions, Operand,
    UnitId, lower_frontend_result, verify_unit,
};
use php_semantics::analyze_source;

#[test]
fn manual_basic_ir_snapshot_is_stable() {
    let unit = manual_basic_unit();
    verify_unit(&unit).expect("manual snapshot unit should verify");
    let actual = unit.to_snapshot_text();
    let expected = include_str!("../../../fixtures/bytecode/valid/manual-basic.ir.snap");
    assert_eq!(actual, expected);
}

#[test]
fn manual_basic_ir_json_is_available() {
    let unit = manual_basic_unit();
    let json = unit.to_json_pretty().expect("manual IR should serialize");
    assert!(json.contains("\"version\": 1"));
    assert!(json.contains("\"functions\""));
    assert!(json.contains("\"opcode\": \"binary\""));
}

#[test]
fn lowered_single_literal_snapshot_is_stable() {
    let actual = lowered_snapshot(
        "<?php echo 1;",
        "fixtures/bytecode/literals/valid/echo-int.php",
    );
    assert_lowered_snapshot(
        &actual,
        include_str!("../../../fixtures/bytecode/valid/literals-single.ir.snap"),
        "fixtures/bytecode/valid/literals-single.ir.snap",
    );
}

#[test]
fn lowered_multiple_literals_snapshot_is_stable() {
    let actual = lowered_snapshot(
        "<?php echo 1, \"x\";",
        "fixtures/bytecode/literals/valid/echo-multiple.php",
    );
    assert_lowered_snapshot(
        &actual,
        include_str!("../../../fixtures/bytecode/valid/literals-multiple.ir.snap"),
        "fixtures/bytecode/valid/literals-multiple.ir.snap",
    );
}

#[test]
fn lowered_source_map_snapshot_is_stable() {
    let actual = lowered_snapshot(
        "<?php echo null, true;",
        "fixtures/bytecode/literals/valid/echo-source-map.php",
    );
    assert_lowered_snapshot(
        &actual,
        include_str!("../../../fixtures/bytecode/valid/source-map.ir.snap"),
        "fixtures/bytecode/valid/source-map.ir.snap",
    );
}

#[test]
fn lowered_foreach_snapshot_is_stable() {
    let source = include_str!("../../../fixtures/bytecode/lower/valid/foreach.php");
    let actual = lowered_snapshot(source, "fixtures/bytecode/lower/valid/foreach.php");
    assert_lowered_snapshot(
        &actual,
        include_str!("../../../fixtures/bytecode/valid/foreach.ir.snap"),
        "fixtures/bytecode/valid/foreach.ir.snap",
    );
}

#[test]
fn lowered_include_snapshot_is_stable() {
    let source = include_str!("../../../fixtures/bytecode/lower/valid/include.php");
    let actual = lowered_snapshot(source, "fixtures/bytecode/lower/valid/include.php");
    assert_lowered_snapshot(
        &actual,
        include_str!("../../../fixtures/bytecode/valid/include.ir.snap"),
        "fixtures/bytecode/valid/include.ir.snap",
    );
}

const RUNTIME_SEMANTICS_INTERNAL_CLASS_SNAPSHOT: &str = concat!(
    "  class:0 \"traversable\" parent=None interfaces=[] methods=0 properties=0 constructor=none flags=abstract:true final:false readonly:false interface:true span=file:0@0..0\n",
    "  class:1 \"iterator\" parent=None interfaces=[\"traversable\"] methods=0 properties=0 constructor=none flags=abstract:true final:false readonly:false interface:true span=file:0@0..0\n",
    "  class:2 \"iteratoraggregate\" parent=None interfaces=[\"traversable\"] methods=0 properties=0 constructor=none flags=abstract:true final:false readonly:false interface:true span=file:0@0..0\n",
    "  class:3 \"arrayaccess\" parent=None interfaces=[] methods=0 properties=0 constructor=none flags=abstract:true final:false readonly:false interface:true span=file:0@0..0\n",
    "  class:4 \"throwable\" parent=None interfaces=[] methods=0 properties=0 constructor=none flags=abstract:true final:false readonly:false interface:true span=file:0@0..0\n",
    "  class:5 \"unitenum\" parent=None interfaces=[] methods=0 properties=0 constructor=none flags=abstract:true final:false readonly:false interface:true span=file:0@0..0\n",
    "  class:6 \"backedenum\" parent=None interfaces=[] methods=0 properties=0 constructor=none flags=abstract:true final:false readonly:false interface:true span=file:0@0..0\n",
    "  class:7 \"stringable\" parent=None interfaces=[] methods=0 properties=0 constructor=none flags=abstract:true final:false readonly:false interface:true span=file:0@0..0\n",
);

fn lowered_expected(snapshot: &str) -> String {
    snapshot.replace(
        "classes:\nfunction_table:",
        &format!("classes:\n{RUNTIME_SEMANTICS_INTERNAL_CLASS_SNAPSHOT}function_table:"),
    )
}

fn assert_lowered_snapshot(actual: &str, expected: &str, snapshot_path: &str) {
    if std::env::var_os("UPDATE_BYTECODE_SNAPSHOTS").is_some() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(snapshot_path);
        std::fs::write(&path, actual).expect("failed to update bytecode snapshot");
        return;
    }
    assert_eq!(actual, lowered_expected(expected));
}

fn manual_basic_unit() -> php_ir::IrUnit {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("fixtures/runtime/valid/scalars/echo.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 5),
    );
    let block = builder.append_block(function);
    let one = builder.add_constant(IrConstant::Int(1));
    let two = builder.add_constant(IrConstant::Int(2));
    let r0 = builder.alloc_register(function);
    let r1 = builder.alloc_register(function);
    let r2 = builder.alloc_register(function);
    builder.emit_load_const(function, block, r0, one, IrSpan::new(file, 6, 7));
    builder.emit_load_const(function, block, r1, two, IrSpan::new(file, 10, 11));
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: r2,
            op: php_ir::BinaryOp::Add,
            lhs: Operand::Register(r0),
            rhs: Operand::Register(r1),
        },
        IrSpan::new(file, 6, 11),
    );
    builder.terminate_return(
        function,
        block,
        Some(Operand::Register(r2)),
        IrSpan::new(file, 6, 11),
    );
    builder.set_entry(function);
    builder.finish()
}

fn lowered_snapshot(source: &str, source_path: &str) -> String {
    let frontend = analyze_source(source);
    let result = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_path: source_path.to_string(),
            ..LoweringOptions::default()
        },
    );
    result
        .verification
        .expect("lowered snapshot unit should verify");
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    result.unit.to_snapshot_text()
}
