use super::*;
use php_semantics::analyze_source;

#[test]
fn generator_methods_with_return_types_keep_the_generator_flag() {
    let frontend = analyze_source(
        "<?php class A { public function g(): Generator { yield 1; } } function h(): Generator { yield 2; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());
    let flags: Vec<(&str, bool)> = result
        .unit
        .functions
        .iter()
        .map(|function| (function.name.as_str(), function.flags.is_generator))
        .collect();
    assert!(
        flags.contains(&("h", true)),
        "function generator flag lost: {flags:?}"
    );
    assert!(
        flags.contains(&("A::g", true)),
        "method generator flag lost: {flags:?}"
    );
}

#[test]
fn lower_empty_file_to_top_level_return_null() {
    let frontend = analyze_source("");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.unit.constants, vec![IrConstant::Null]);
    assert!(result.unit.to_snapshot_text().contains("return const:0"));
}

#[test]
fn lower_open_tag_minimal_program() {
    let frontend = analyze_source("<?php");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty());
}

#[test]
fn unsupported_feature_diagnostic_has_shared_envelope() {
    let diagnostic = LoweringDiagnostic::unsupported(
        UnsupportedFeature::Eval,
        IrSpan::new(FileId::new(0), 10, 14),
        "eval is not supported by IR lowering",
    );
    let context = LoweringDiagnosticContext {
        source_id: Some("source:0".to_string()),
        origin: Some("hir:expr:2".to_string()),
        function: Some(FunctionId::new(1)),
        block: Some(BlockId::new(2)),
        instruction: None,
        class_name: Some("C".to_string()),
        method_name: Some("m".to_string()),
    };

    let envelope = diagnostic.to_diagnostic_envelope(Some("demo.php"), &context);
    let json: serde_json::Value =
        serde_json::from_str(&envelope.compact_json().expect("json")).expect("parse json");

    assert_eq!(json["code"], "E_PHP_IR_UNSUPPORTED_EVAL");
    assert_eq!(json["layer"], "ir");
    assert_eq!(json["phase"], "lower");
    assert_eq!(json["severity"], "unsupported_feature");
    assert_eq!(json["location"]["path"], "demo.php");
    assert_eq!(json["location"]["span"]["start"], 10);
    assert_eq!(json["context"]["feature"], "eval");
    assert_eq!(json["context"]["function_id"], "1");
    assert_eq!(json["context"]["block_id"], "2");
    assert_eq!(json["context"]["origin"], "hir:expr:2");
}

#[test]
fn global_array_const_initializers_lower_to_ir_constants() {
    let frontend = analyze_source(r#"<?php const EXPECTED = ["x" => "y", 2 => "z"];"#);
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.constant_table.len(), 1);
    let value = &result.unit.constants[result.unit.constant_table[0].value.index()];
    assert_eq!(
        value,
        &IrConstant::Array(vec![
            IrConstantArrayEntry {
                key: Some(IrConstant::String("x".to_string())),
                value: IrConstant::String("y".to_string()),
            },
            IrConstantArrayEntry {
                key: Some(IrConstant::Int(2)),
                value: IrConstant::String("z".to_string()),
            },
        ])
    );
}

#[test]
fn global_const_initializers_can_alias_class_constants() {
    let frontend = analyze_source(
        "<?php namespace Sodium; class Compat { const KEYBYTES = 32; } const CRYPTO_KEYBYTES = Compat::KEYBYTES;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.constant_table.len(), 1);
    let value = &result.unit.constants[result.unit.constant_table[0].value.index()];
    assert_eq!(value, &IrConstant::Int(32));
}

#[test]
fn global_const_initializers_can_register_external_class_constants_at_runtime() {
    let frontend =
        analyze_source("<?php namespace Sodium; const CRYPTO_KEYBYTES = Compat::KEYBYTES;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(result.unit.constant_table.is_empty());
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("fetch_class_constant r0 Sodium\\Compat::KEYBYTES"),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("register_constant \"Sodium\\\\CRYPTO_KEYBYTES\" r0"),
        "{snapshot}"
    );
}

#[test]
fn static_class_constant_targets_use_class_import_resolution() {
    let frontend = analyze_source(
        "<?php namespace Sodium; use ParagonIE_Sodium_Compat; const CRYPTO_KEYBYTES = ParagonIE_Sodium_Compat::KEYBYTES;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("fetch_class_constant r0 ParagonIE_Sodium_Compat::KEYBYTES"),
        "{snapshot}"
    );
    assert!(
        !snapshot.contains("Sodium\\ParagonIE_Sodium_Compat::KEYBYTES"),
        "{snapshot}"
    );
    assert!(
        !snapshot.contains("Sodium\\paragonie_sodium_compat::KEYBYTES"),
        "{snapshot}"
    );
}

#[test]
fn class_name_constant_preserves_source_spelling() {
    let frontend = analyze_source("<?php class ClassNameBase {} echo ClassNameBase::class;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("string \"ClassNameBase\""), "{snapshot}");
    assert!(!snapshot.contains("string \"classnamebase\""), "{snapshot}");
}

#[test]
fn namespaced_class_name_constant_uses_declared_fqn_display() {
    let frontend = analyze_source("<?php namespace P21\\Ns; class Child {} echo Child::class;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("string \"P21\\\\Ns\\\\Child\""),
        "{snapshot}"
    );
}

#[test]
fn namespaced_external_class_name_constant_uses_resolved_fqn_display() {
    let frontend = analyze_source(
        "<?php namespace WordPress\\AiClientDependencies\\Http\\Discovery; echo Strategy\\GeneratedDiscoveryStrategy::class;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
            snapshot.contains(
                "string \"WordPress\\\\AiClientDependencies\\\\Http\\\\Discovery\\\\Strategy\\\\GeneratedDiscoveryStrategy\""
            ),
            "{snapshot}"
        );
    assert!(!snapshot.contains("fetch_class_constant"), "{snapshot}");
}

#[test]
fn imported_qualified_class_name_constant_expands_alias_prefix() {
    let frontend = analyze_source(
        "<?php namespace Foo; use Vendor\\Package as PackageAlias; echo PackageAlias\\Generated::class;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("string \"Vendor\\\\Package\\\\Generated\""),
        "{snapshot}"
    );
    assert!(!snapshot.contains("fetch_class_constant"), "{snapshot}");
}

#[test]
fn static_property_class_name_constant_initializer_lowers_to_string() {
    let frontend = analyze_source(
        "<?php namespace WordPress\\AiClientDependencies\\Http\\Discovery; abstract class ClassDiscovery { private static $strategies = [Strategy\\GeneratedDiscoveryStrategy::class]; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
            snapshot.contains(
                "array [append=>string \"WordPress\\\\AiClientDependencies\\\\Http\\\\Discovery\\\\Strategy\\\\GeneratedDiscoveryStrategy\"]"
            ),
            "{snapshot}"
        );
    assert!(!snapshot.contains("class_const"), "{snapshot}");
}

#[test]
fn class_constant_forward_references_lower_to_ir_constants() {
    let frontend = analyze_source(
        "<?php class C { const CONST_2 = self::CONST_1; const CONST_1 = self::BASE_CONST; const BASE_CONST = 'hello'; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let class = result
        .unit
        .classes
        .iter()
        .find(|class| class.name == "c")
        .expect("class C");
    let values = class
        .constants
        .iter()
        .map(|constant| {
            let value = constant.value.expect("constant should have folded value");
            (
                constant.name.as_str(),
                result.unit.constants[value.index()].clone(),
            )
        })
        .collect::<HashMap<_, _>>();

    assert_eq!(
        values.get("CONST_1"),
        Some(&IrConstant::String("hello".into()))
    );
    assert_eq!(
        values.get("CONST_2"),
        Some(&IrConstant::String("hello".into()))
    );
    assert_eq!(
        values.get("BASE_CONST"),
        Some(&IrConstant::String("hello".into()))
    );
}

#[test]
fn method_parameter_defaults_can_use_class_constants() {
    let frontend = analyze_source(
        "<?php class C { const LIMIT = 32; public static function f($limit = self::LIMIT) { return $limit; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let method = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "C::f")
        .expect("method function");
    assert_eq!(method.params[0].default, Some(IrConstant::Int(32)));
}

#[test]
fn custom_typed_catches_and_by_ref_method_parameters_lower_to_ir() {
    let frontend = analyze_source(
        "<?php class MyEx extends Exception {} class C { public function fill(&$value) { try { throw new MyEx('x'); } catch (MyEx $e) { $value = $e->getMessage(); } } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let method = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "C::fill")
        .expect("method function");
    assert!(method.params[0].by_ref);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("catch_types=[myex]"), "{snapshot}");
}

#[test]
fn by_ref_method_returns_lower_to_reference_ir() {
    let frontend = analyze_source(
        "<?php class C { public function &counter() { static $x = 0; return $x; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let method = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "C::counter")
        .expect("method function");
    assert!(method.returns_by_ref);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("function \"C::counter\""), "{snapshot}");
    assert!(snapshot.contains("return_ref local:"), "{snapshot}");
    assert!(
        !snapshot.contains("E_PHP_IR_UNSUPPORTED_BY_REF_RETURN"),
        "{snapshot}"
    );
}

#[test]
fn class_constant_doc_comments_lower_to_ir_metadata() {
    let source = "<?php class C { /** label */ const LABEL = 'items'; const PLAIN = 1; }";
    let frontend = analyze_source(source);
    let result = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_text: Some(source.to_owned()),
            ..LoweringOptions::default()
        },
    );

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let class = result
        .unit
        .classes
        .iter()
        .find(|class| class.name == "c")
        .expect("class C");
    let doc_comments = class
        .constants
        .iter()
        .map(|constant| (constant.name.as_str(), constant.doc_comment.as_deref()))
        .collect::<HashMap<_, _>>();

    assert_eq!(doc_comments.get("LABEL"), Some(&Some("/** label */")));
    assert_eq!(doc_comments.get("PLAIN"), Some(&None));
}

#[test]
fn property_array_defaults_fold_nested_dir_magic_constants() {
    let source = "<?php
class ComposerStaticInit
{
    public static $files = array(
        'polyfill' => __DIR__ . '/../src/polyfills.php',
    );
}
var_dump(is_array(ComposerStaticInit::$files));";
    let frontend = analyze_source(source);
    let result = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_path: "/app/vendor/composer/autoload_static.php".to_owned(),
            source_text: Some(source.to_owned()),
            ..LoweringOptions::default()
        },
    );

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let property = result
        .unit
        .classes
        .iter()
        .find(|class| class.name == "composerstaticinit")
        .and_then(|class| {
            class
                .properties
                .iter()
                .find(|property| property.name == "files")
        })
        .expect("Composer static files property");
    let default = property.default.expect("folded property default");
    assert_eq!(
        result.unit.constants[default.index()],
        IrConstant::Array(vec![IrConstantArrayEntry {
            key: Some(IrConstant::String("polyfill".to_owned())),
            value: IrConstant::String("/app/vendor/composer/../src/polyfills.php".to_owned()),
        }])
    );
}

#[test]
fn method_array_parameter_defaults_lower_to_ir_constants() {
    let frontend = analyze_source(
        "<?php class Test { static function f3(array $ar = array()) {} static function f4(array $ar = array(25)) {} }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let f3 = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "Test::f3")
        .expect("Test::f3 function");
    let f4 = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "Test::f4")
        .expect("Test::f4 function");

    assert_eq!(f3.params[0].default, Some(IrConstant::Array(Vec::new())));
    assert_eq!(
        f4.params[0].default,
        Some(IrConstant::Array(vec![IrConstantArrayEntry {
            key: None,
            value: IrConstant::Int(25),
        }]))
    );
}

#[test]
fn parameter_default_expression_matrix_lowers_to_ir_constants() {
    let frontend = analyze_source(
        "<?php const LABEL = 'B'; class Source { const FIRST = 'A'; } function f($items = ['left' => Source::FIRST, 'right' => LABEL], $selected = ['x', 'y'][1], $fallback = null ?? 'fallback', $conditional = true ? 'yes' : 'no', $casted = (int) '42') {}",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let function = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("function f");

    assert_eq!(
        function.params[0].default,
        Some(IrConstant::Array(vec![
            IrConstantArrayEntry {
                key: Some(IrConstant::String("left".to_owned())),
                value: IrConstant::String("A".to_owned()),
            },
            IrConstantArrayEntry {
                key: Some(IrConstant::String("right".to_owned())),
                value: IrConstant::String("B".to_owned()),
            },
        ]))
    );
    assert_eq!(
        function.params[1].default,
        Some(IrConstant::String("y".to_owned()))
    );
    assert_eq!(
        function.params[2].default,
        Some(IrConstant::String("fallback".to_owned()))
    );
    assert_eq!(
        function.params[3].default,
        Some(IrConstant::String("yes".to_owned()))
    );
    assert_eq!(function.params[4].default, Some(IrConstant::Int(42)));
}

#[test]
fn closure_and_arrow_parameter_defaults_lower_from_typed_hir() {
    let frontend = analyze_source(
        "<?php $closure = function ($value = 'B') { return $value; }; $arrow = fn($value = 'C') => $value;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let closure = result
        .unit
        .functions
        .iter()
        .find(|function| function.name.starts_with("closure@"))
        .expect("closure function");
    let arrow = result
        .unit
        .functions
        .iter()
        .find(|function| function.name.starts_with("arrow@"))
        .expect("arrow function");

    assert_eq!(
        closure.params[0].default,
        Some(IrConstant::String("B".to_owned()))
    );
    assert_eq!(
        arrow.params[0].default,
        Some(IrConstant::String("C".to_owned()))
    );
}

#[test]
fn parameter_default_array_preserves_external_class_constant() {
    let frontend = analyze_source(
        "<?php class Test { public function __construct($data = array('version' => External::LATEST_SCHEMA)) {} }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let function = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "Test::__construct")
        .expect("Test::__construct function");

    assert_eq!(
        function.params[0].default,
        Some(IrConstant::Array(vec![IrConstantArrayEntry {
            key: Some(IrConstant::String("version".to_owned())),
            value: IrConstant::ClassConstant {
                class_name: "external".to_owned(),
                display_class_name: "External".to_owned(),
                constant_name: "LATEST_SCHEMA".to_owned(),
            },
        }]))
    );
}

#[test]
fn conditional_method_array_parameter_defaults_lower_to_ir_constants() {
    let source = "<?php if (!class_exists('Test', false)) : class Test { static function f3($ar = array()) {} static function f4($ar = array(25)) {} } endif;";
    let frontend = analyze_source(source);
    let result = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_text: Some(source.to_owned()),
            ..LoweringOptions::default()
        },
    );

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let f3 = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "Test::f3")
        .expect("Test::f3 function");
    let f4 = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "Test::f4")
        .expect("Test::f4 function");

    assert_eq!(f3.params[0].default, Some(IrConstant::Array(Vec::new())));
    assert_eq!(
        f4.params[0].default,
        Some(IrConstant::Array(vec![IrConstantArrayEntry {
            key: None,
            value: IrConstant::Int(25),
        }]))
    );
}

#[test]
fn source_define_parameter_defaults_lower_to_ir_constants() {
    let source =
        "<?php define('OBJECT', 'OBJECT'); class Test { public function get($output = OBJECT) {} }";
    let frontend = analyze_source(source);
    let result = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_text: Some(source.to_owned()),
            ..LoweringOptions::default()
        },
    );

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let method = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "Test::get")
        .expect("Test::get function");

    assert_eq!(
        method.params[0].default,
        Some(IrConstant::String("OBJECT".to_owned()))
    );
}

#[test]
fn core_integer_constant_parameter_defaults_lower_to_ir_constants() {
    let frontend = analyze_source(
        "<?php function bounds(?int $max = PHP_INT_MAX, ?int $min = PHP_INT_MIN, int $size = PHP_INT_SIZE, int $level = E_USER_NOTICE) {}",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let function = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "bounds")
        .expect("bounds function");

    assert_eq!(
        function.params[0].default,
        Some(IrConstant::Int(isize::MAX as i64))
    );
    assert_eq!(
        function.params[1].default,
        Some(IrConstant::Int(isize::MIN as i64))
    );
    assert_eq!(
        function.params[2].default,
        Some(IrConstant::Int(std::mem::size_of::<isize>() as i64))
    );
    assert_eq!(function.params[3].default, Some(IrConstant::Int(1024)));
}

#[test]
fn static_property_isset_empty_lower_to_static_property_instructions() {
    let frontend = analyze_source("<?php class C {} var_dump(isset(C::$p), empty(C::$p));");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("isset_static_property r"), "{snapshot}");
    assert!(snapshot.contains("empty_static_property r"), "{snapshot}");
    assert!(snapshot.contains("C::$p"), "{snapshot}");
}

#[test]
fn dynamic_class_static_property_assignment_lowers() {
    let frontend = analyze_source(
        "<?php class Mailer { public static $validator; } $phpmailer = new Mailer(); $phpmailer::$validator = static function ($email) { return true; };",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("assign_dynamic_static_property r"),
        "{snapshot}"
    );
    assert!(snapshot.contains("::$validator"), "{snapshot}");
    assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
}

#[test]
fn static_property_dimension_isset_and_unset_lower_to_static_property_dim_instructions() {
    let frontend = analyze_source(
        "<?php class C { private static $map = ['id' => 'ID']; function f($key) { var_dump(isset(self::$map[$key])); unset(self::$map[$key]); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("isset_static_property_dim r"),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("unset_static_property_dim self::$map"),
        "{snapshot}"
    );
    assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
}

#[test]
fn class_constant_dimension_isset_empty_lower_through_hidden_local() {
    let frontend = analyze_source(
        "<?php class C { const MAP = ['id' => 'ID']; function f($key) { var_dump(isset(self::MAP[$key]), empty(self::MAP[$key])); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_class_constant r"), "{snapshot}");
    assert!(snapshot.contains("isset_dim r"), "{snapshot}");
    assert!(snapshot.contains("empty_dim r"), "{snapshot}");
    assert!(
        snapshot.contains("__phrust:isset-class-constant-dim"),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("__phrust:empty-class-constant-dim"),
        "{snapshot}"
    );
}

#[test]
fn construct_empty_superglobal_dim_lowers_to_empty_dim_instruction() {
    let frontend = analyze_source(
        "<?php const RECOVERY_MODE_COOKIE = 'wordpress_rec'; if (empty($_COOKIE[RECOVERY_MODE_COOKIE])) { echo 'missing'; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("empty_dim r"), "{snapshot}");
    assert!(snapshot.contains("RECOVERY_MODE_COOKIE"), "{snapshot}");
}

#[test]
fn function_auto_global_binding_uses_cached_variable_spans() {
    let frontend = analyze_source(
        "<?php function uses_server() { return $_SERVER['REQUEST_METHOD']; } function plain($value) { return $value; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(snapshot.matches("bind_global").count(), 1, "{snapshot}");
    assert!(snapshot.contains("bind_global"), "{snapshot}");
    assert!(snapshot.contains("\"_SERVER\""), "{snapshot}");
}

#[test]
fn global_and_static_lists_lower_from_typed_hir() {
    let frontend = analyze_source(
        "<?php function f() { global /* first */ $first,\n $second; static $cache = 1 + 2, $empty; return $first; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(snapshot.matches("bind_global").count(), 2, "{snapshot}");
    assert!(
        snapshot.contains("bind_global local:0 \"first\""),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("bind_global local:1 \"second\""),
        "{snapshot}"
    );
    assert_eq!(
        snapshot.matches("init_static_local").count(),
        2,
        "{snapshot}"
    );
    assert!(snapshot.contains("\"cache\""), "{snapshot}");
    assert!(snapshot.contains("\"empty\""), "{snapshot}");
    assert!(snapshot.contains("binary r"), "{snapshot}");
}

#[test]
fn dynamic_global_reports_typed_runtime_gap_without_inventing_a_name() {
    let frontend = analyze_source("<?php function f($which) { global $$which; return $which; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert_eq!(result.diagnostics.len(), 1, "{:#?}", result.diagnostics);
    assert_eq!(
        result.diagnostics[0].message,
        "dynamic global variables are not lowered to IR in runtime-semantics"
    );
    let snapshot = result.unit.to_snapshot_text();
    assert!(!snapshot.contains("bind_global"), "{snapshot}");
    assert!(!snapshot.contains("\"$which\""), "{snapshot}");
}

#[test]
fn construct_empty_method_call_lowers_to_unary_not() {
    let frontend = analyze_source(
        "<?php class C { function get($name) { return $name; } } $c = new C(); var_dump(empty($c->get('RequiresWP')));",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_method r"), "{snapshot}");
    assert!(snapshot.contains("unary r"), "{snapshot}");
    assert!(snapshot.contains("not"), "{snapshot}");
}

#[test]
fn construct_empty_static_method_call_lowers_to_static_call_and_not() {
    let frontend = analyze_source("<?php var_dump(empty(Imagick::queryFormats('WEBP')));");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_static_method r"), "{snapshot}");
    assert!(
        snapshot.contains("\"Imagick\"::\"queryFormats\""),
        "{snapshot}"
    );
    assert!(snapshot.contains("unary r"), "{snapshot}");
    assert!(snapshot.contains("not"), "{snapshot}");
}

#[test]
fn static_method_call_uses_import_display_name_for_autoload() {
    let frontend = analyze_source(
        "<?php namespace WpOrg\\Requests; use WpOrg\\Requests\\Utility\\InputValidator; final class Requests { public static function set_certificate_path($path) { return InputValidator::is_string_or_stringable($path); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains(
            "\"WpOrg\\\\Requests\\\\Utility\\\\InputValidator\"::\"is_string_or_stringable\""
        ),
        "{snapshot}"
    );
    assert!(!snapshot.contains("\"InputValidator\"::"), "{snapshot}");
    assert!(
        !snapshot.contains("\"wporg\\\\requests\\\\utility\\\\inputvalidator\"::"),
        "{snapshot}"
    );
}

#[test]
fn dynamic_static_method_call_lowers_class_variable_callable_pair() {
    let frontend = analyze_source(
        "<?php class DynamicStaticProbe { public static function test($value) { return $value === 'ok'; } } $class = DynamicStaticProbe::class; $result = $class::test('ok'); var_dump($result);",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_callable r"), "{snapshot}");
    assert!(snapshot.contains("local:0"), "{snapshot}");
    assert!(snapshot.contains("\"test\""), "{snapshot}");
    assert!(snapshot.contains("store_local local:1"), "{snapshot}");
}

#[test]
fn dynamic_static_method_call_lowers_variable_method_operand() {
    let source = "<?php class DynamicStaticProbe { public static function test() { return 'ok'; } } $class = DynamicStaticProbe::class; $method = 'test'; echo $class::$method();";
    let frontend = analyze_source(source);
    let result = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_text: Some(source.to_owned()),
            ..LoweringOptions::default()
        },
    );

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_callable r"), "{snapshot}");
    assert!(snapshot.contains("load_local r"), "{snapshot}");
    assert!(!snapshot.contains("string \"method\""), "{snapshot}");
}

#[test]
fn function_import_alias_lowers_to_imported_name() {
    let source = "<?php namespace App; use function Vendor\\Package\\helper as imported_helper; echo imported_helper();";
    let frontend = analyze_source(source);
    let result = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_text: Some(source.to_owned()),
            ..LoweringOptions::default()
        },
    );

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("call_function") && snapshot.contains("vendor\\\\package\\\\helper"),
        "{snapshot}"
    );
    assert!(!snapshot.contains("app\\\\imported_helper"), "{snapshot}");
}

#[test]
fn class_constant_uses_import_display_name_for_autoload() {
    let frontend = analyze_source(
        "<?php namespace WpOrg\\Requests; use WpOrg\\Requests\\Capability; final class Requests { public static function request() { return [Capability::SSL => true]; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("WpOrg\\Requests\\Capability::SSL"),
        "{snapshot}"
    );
    assert!(!snapshot.contains(" Capability::SSL"), "{snapshot}");
    assert!(
        !snapshot.contains("wporg\\requests\\capability::SSL"),
        "{snapshot}"
    );
}

#[test]
fn isset_class_constant_dim_uses_import_display_name_for_autoload() {
    let frontend = analyze_source(
        "<?php namespace WpOrg\\Requests; use WpOrg\\Requests\\Capability; final class Requests { public static function test($values) { return isset($values[Capability::SSL]); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("WpOrg\\Requests\\Capability::SSL"),
        "{snapshot}"
    );
    assert!(!snapshot.contains(" Capability::SSL"), "{snapshot}");
    assert!(
        !snapshot.contains("wporg\\requests\\capability::SSL"),
        "{snapshot}"
    );
}

#[test]
fn construct_isset_braced_dynamic_property_lowers_to_dynamic_property_instruction() {
    let frontend =
        analyze_source("<?php function matches($obj, $m_key) { return isset($obj->{$m_key}); }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("isset_dynamic_property r"), "{snapshot}");
    assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
}

#[test]
fn construct_empty_unbraced_dynamic_property_lowers_to_dynamic_property_instruction() {
    let frontend = analyze_source(
        "<?php function active($kind) { return ! empty( get_queried_object()->$kind ); }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("empty_dynamic_property r")
            || snapshot.contains("fetch_dynamic_property r"),
        "{snapshot}"
    );
    assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
}

#[test]
fn dynamic_property_variable_member_isset_and_unset_lower_without_literal_diagnostics() {
    let frontend = analyze_source(
        "<?php class C { public $data; function has($key) { var_dump(isset($this->data->$key)); unset($this->data->$key); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("isset_dynamic_property r"), "{snapshot}");
    assert!(snapshot.contains("unset_dynamic_property"), "{snapshot}");
    assert!(
        !snapshot.contains("E_PHP_IR_UNSUPPORTED_LITERAL"),
        "{snapshot}"
    );
}

#[test]
fn method_call_with_dynamic_property_argument_is_not_dynamic_method_call() {
    let frontend = analyze_source(
        "<?php class U { function download_package($x, $y) {} function f($current, $to_download) { $download = $this->download_package($current->packages->$to_download, false); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_method r"), "{snapshot}");
    assert!(snapshot.contains("fetch_dynamic_property r"), "{snapshot}");
    assert!(!snapshot.contains("call_callable"), "{snapshot}");
    assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
}

#[test]
fn construct_isset_concat_dim_key_lowers_to_isset_dim_instruction() {
    let frontend = analyze_source(
        "<?php function cookie_exists($user_id) { return isset($_COOKIE['wp-settings-' . $user_id]); }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("binary r"), "{snapshot}");
    assert!(snapshot.contains("isset_dim r"), "{snapshot}");
    assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
}

#[test]
fn construct_isset_concat_constant_dim_key_lowers_to_isset_dim_instruction() {
    let frontend = analyze_source(
        "<?php function postpass_cookie_exists() { return isset($_COOKIE['wp-postpass_' . COOKIEHASH]); }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_const"), "{snapshot}");
    assert!(snapshot.contains("binary r"), "{snapshot}");
    assert!(snapshot.contains("isset_dim r"), "{snapshot}");
    assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
}

#[test]
fn construct_isset_interpolated_dim_lowers_to_isset_dim_instruction() {
    let frontend = analyze_source(
        r#"<?php function plugin($plugins, $extension) { if (isset($plugins["{$extension['slug']}/{$extension['slug']}.php"])) { return $plugins["{$extension['slug']}/{$extension['slug']}.php"]; } }"#,
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("isset_dim r"), "{snapshot}");
    assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
}

#[test]
fn construct_isset_nested_dim_key_lowers_to_isset_dim_instruction() {
    let frontend = analyze_source(
        "<?php function error_name($core_errors, $error) { if (isset($core_errors[$error['type']])) { echo $core_errors[$error['type']]; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("isset_dim r"), "{snapshot}");
    assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
}

#[test]
fn static_property_append_lowers_through_hidden_local_and_assign() {
    let frontend = analyze_source("<?php class C { static public $p = array(); } C::$p[] = 1;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
    assert!(snapshot.contains("append_dim r"), "{snapshot}");
    assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
    assert!(
        snapshot.contains("__phrust:static-property-dim"),
        "{snapshot}"
    );
    assert!(snapshot.contains("C::$p"), "{snapshot}");
}

#[test]
fn array_unshift_static_property_arg_lowers_through_hidden_local_and_assign() {
    let frontend = analyze_source(
        "<?php class C { private static $items = array(); function f($value) { array_unshift(self::$items, $value); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
    assert!(
        snapshot.contains("__phrust:array_unshift-static-property"),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("call_function r") && snapshot.contains("array_unshift"),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("by_ref=local") || snapshot.contains("by_ref=local:"),
        "{snapshot}"
    );
    assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
    assert!(snapshot.contains("self::$items"), "{snapshot}");
}

#[test]
fn namespaced_array_unshift_static_property_arg_lowers_through_hidden_local_and_assign() {
    let frontend = analyze_source(
        "<?php namespace WordPress\\AiClientDependencies\\Http\\Discovery; abstract class ClassDiscovery { private static $strategies = array(); function f($strategy) { array_unshift(self::$strategies, $strategy); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
    assert!(
            snapshot.contains("__phrust:wordpress\\aiclientdependencies\\http\\discovery\\array_unshift-static-property"),
            "{snapshot}"
        );
    assert!(
        snapshot.contains("call_function r")
            && snapshot.contains("wordpress\\aiclientdependencies\\http\\discovery\\array_unshift"),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("by_ref=local") || snapshot.contains("by_ref=local:"),
        "{snapshot}"
    );
    assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
    assert!(snapshot.contains("self::$strategies"), "{snapshot}");
}

#[test]
fn imported_nullable_parameter_type_lowers_to_resolved_class_name() {
    let frontend = analyze_source(
        "<?php namespace App; use Vendor\\Contracts\\CacheInterface; function set_cache(?CacheInterface $cache): void {}",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
            snapshot.contains(
                "param \"cache\" local:0 required=true variadic=false by_ref=false type=?class \"vendor\\\\contracts\\\\cacheinterface\""
            ),
            "{snapshot}"
        );
}

#[test]
fn nested_static_property_dimension_assignment_lowers_through_hidden_local() {
    let frontend = analyze_source(
        "<?php class C { static public $p = array(); function f($outer, $inner) { self::$p[$outer][$inner] = 1; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
    assert!(snapshot.contains("assign_dim r"), "{snapshot}");
    assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
    assert!(
        snapshot.contains("__phrust:static-property-dim"),
        "{snapshot}"
    );
    assert!(snapshot.contains("self::$p"), "{snapshot}");
}

#[test]
fn static_property_dimension_increment_lowers_through_hidden_local() {
    let frontend = analyze_source(
        "<?php class C { private static $seen = array(); function f($name) { ++static::$seen[$name]; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
    assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    assert!(snapshot.contains("assign_dim r"), "{snapshot}");
    assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
    assert!(snapshot.contains("static::$seen"), "{snapshot}");
}

#[test]
fn namespaced_self_static_property_keeps_relative_class_name() {
    let frontend = analyze_source(
        "<?php namespace WpOrg\\Requests; final class Requests { protected static $certificate_path = ''; public static function set_certificate_path($path) { self::$certificate_path = $path; return self::$certificate_path; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("assign_static_property r")
            && snapshot.contains("self::$certificate_path"),
        "{snapshot}"
    );
    assert!(
        !snapshot.contains("WpOrg\\\\Requests\\\\self::$certificate_path"),
        "{snapshot}"
    );
}

#[test]
fn composer_namespaced_self_static_property_fetch_keeps_relative_class_name() {
    let frontend = analyze_source(
        "<?php namespace Composer\\Autoload; class ClassLoader { private static $includeFile; public function loadClass() { $includeFile = self::$includeFile; } private static function initializeIncludeClosure() { if (self::$includeFile !== null) { return; } self::$includeFile = null; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("self::$includeFile"), "{snapshot}");
    assert!(
        !snapshot.contains("Composer\\\\Autoload\\\\self::$includeFile"),
        "{snapshot}"
    );
}

#[test]
fn anonymous_class_new_lowers_to_synthetic_class_instantiation() {
    let frontend = analyze_source(
        "<?php class Base { public function __construct($value) {} } function f($value) { return new class($value) extends Base { public function m() {} }; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("__phrust_anonymous_"), "{snapshot}");
    assert!(snapshot.contains("_anonymous_0"), "{snapshot}");
    assert!(snapshot.contains("new_object r"), "{snapshot}");
    assert!(snapshot.contains("anonymous#0"), "{snapshot}");
}

#[test]
fn static_property_compound_assign_and_increment_fetch_before_write() {
    let frontend = analyze_source("<?php class C {} C::$p += 1; C::$p++;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(
        snapshot.matches("fetch_static_property r").count(),
        2,
        "{snapshot}"
    );
    assert_eq!(
        snapshot.matches("assign_static_property r").count(),
        2,
        "{snapshot}"
    );
    assert!(snapshot.contains("binary r"), "{snapshot}");
    assert!(snapshot.contains("C::$p"), "{snapshot}");
}

#[test]
fn property_increment_lowers_through_fetch_and_assign_property() {
    let frontend = analyze_source("<?php class C {} $c = new C; $c->p++; ++$c->p;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(
        snapshot.matches("fetch_property r").count(),
        2,
        "{snapshot}"
    );
    assert_eq!(
        snapshot.matches("assign_property r").count(),
        2,
        "{snapshot}"
    );
    assert!(snapshot.contains("binary r"), "{snapshot}");
}

#[test]
fn dynamic_property_increment_lowers_through_fetch_and_assign_dynamic_property() {
    let frontend =
        analyze_source("<?php class C {} $c = new C; $field = 'p'; $c->$field++; ++$c->$field;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(
        snapshot.matches("fetch_dynamic_property r").count(),
        2,
        "{snapshot}"
    );
    assert_eq!(
        snapshot.matches("assign_dynamic_property r").count(),
        2,
        "{snapshot}"
    );
    assert!(snapshot.contains("binary r"), "{snapshot}");
    assert!(
        !snapshot.contains("E_PHP_IR_UNSUPPORTED_HIR_STATEMENT"),
        "{snapshot}"
    );
}

#[test]
fn property_dimension_increment_lowers_through_assign_property_dim() {
    let frontend = analyze_source("<?php class C {} $c = new C; ++$c->p['n']; $c->p['n']--;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(snapshot.matches("fetch_dim r").count(), 2, "{snapshot}");
    assert_eq!(
        snapshot.matches("assign_property_dim r").count(),
        2,
        "{snapshot}"
    );
    assert!(snapshot.contains("binary r"), "{snapshot}");
}

#[test]
fn property_compound_assign_lowers_through_fetch_binary_and_assign_property() {
    let frontend = analyze_source("<?php class C { public $s = ''; } $c = new C; $c->s .= 'x';");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_property r"), "{snapshot}");
    assert!(snapshot.contains("binary r"), "{snapshot}");
    assert!(snapshot.contains("assign_property r"), "{snapshot}");
    assert!(snapshot.contains("$s"), "{snapshot}");
}

#[test]
fn property_dimensions_assignment_append_and_unset_lower_to_dedicated_ir() {
    let frontend = analyze_source(
        "<?php class C { private $callbacks = array(); public function run($priority, $idx) { $this->callbacks[$priority][$idx] = array('function' => 'f'); $this->callbacks[] = 'tail'; unset($this->callbacks[$priority][$idx]); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("assign_property_dim r"), "{snapshot}");
    assert!(snapshot.contains("append_property_dim r"), "{snapshot}");
    assert!(snapshot.contains("unset_property_dim r"), "{snapshot}");
    assert!(snapshot.contains("$callbacks"), "{snapshot}");
}

#[test]
fn property_dimension_compound_assignment_lowers_through_fetch_binary_and_writeback() {
    let frontend = analyze_source(
        "<?php class C { private $cache = []; public function run($group, $key, $offset) { $this->cache[$group][$key] += $offset; $this->cache[$group][$key] -= $offset; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_property r"), "{snapshot}");
    assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    assert!(snapshot.contains("binary r"), "{snapshot}");
    assert_eq!(
        snapshot.matches("assign_property_dim r").count(),
        2,
        "{snapshot}"
    );
    assert!(snapshot.contains("$cache"), "{snapshot}");
}

#[test]
fn property_reference_assignments_lower_to_reference_ir() {
    let frontend = analyze_source(
        "<?php class C { public $extra; public function bind(&$value, $key, $source) { $this->extra = & $value; $GLOBALS[$key] = & $source->extra; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("bind_reference_property r"), "{snapshot}");
    assert!(
        snapshot.contains("bind_reference_dim_from_property"),
        "{snapshot}"
    );
    assert!(
        !snapshot.contains("E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE"),
        "{snapshot}"
    );
}

#[test]
fn by_ref_foreach_over_property_lowers_through_hidden_local_writeback() {
    let frontend = analyze_source(
        "<?php class C { private $iterations = array(1); public function run() { foreach ($this->iterations as &$iteration) { $iteration = $iteration + 1; } unset($iteration); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_property r"), "{snapshot}");
    assert!(
        snapshot.contains("__phrust:foreach-ref-property"),
        "{snapshot}"
    );
    assert!(snapshot.contains("foreach_init_ref iter"), "{snapshot}");
    assert!(snapshot.contains("assign_property r"), "{snapshot}");
    assert!(
        !snapshot.contains("E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH"),
        "{snapshot}"
    );
}

#[test]
fn by_ref_foreach_over_local_dim_lowers_through_hidden_local_writeback() {
    let frontend = analyze_source(
        "<?php function rename_blocks($settings) { foreach ($settings['blocks'] as &$block_settings) { $block_settings['x'] = 1; } unset($block_settings); return $settings; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    assert!(snapshot.contains("__phrust:foreach-ref-dim"), "{snapshot}");
    assert!(snapshot.contains("foreach_init_ref iter"), "{snapshot}");
    assert!(snapshot.contains("assign_dim r"), "{snapshot}");
    assert!(
        !snapshot.contains("E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH"),
        "{snapshot}"
    );
}

#[test]
fn constructor_promoted_properties_lower_to_property_and_assignment() {
    let frontend = analyze_source(
        "<?php class Name { function __construct(public string $name) {} function display() { echo $this->name; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let class = result
        .unit
        .classes
        .iter()
        .find(|class| class.name == "name")
        .expect("lowered Name class");
    let property = class
        .properties
        .iter()
        .find(|property| property.name == "name")
        .expect("promoted name property");
    assert!(property.flags.is_typed, "{property:#?}");
    assert!(!property.flags.is_private, "{property:#?}");
    assert!(!property.flags.is_protected, "{property:#?}");
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("assign_property r"), "{snapshot}");
    assert!(snapshot.contains("Name::__construct"), "{snapshot}");
}

#[test]
fn lower_echo_literal_statement_emits_load_const_and_echo() {
    let frontend = analyze_source("<?php echo 1;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("load_const r0 const:1"));
    assert!(snapshot.contains("echo r0"));
    assert!(snapshot.contains("source_map:"));
    assert!(snapshot.contains("instr function:0 block:1 instr:0 <= hir:expr:0"));
}

#[test]
fn lower_top_level_exit_statement_terminates_script() {
    let frontend = analyze_source("<?php echo 'before'; exit; echo 'after';");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("echo r0"), "{snapshot}");
    assert!(snapshot.contains("exit"), "{snapshot}");
    assert!(!snapshot.contains("after"), "{snapshot}");
}

#[test]
fn lower_top_level_exit_message_emits_before_terminating_script() {
    let frontend = analyze_source("<?php die('skip platform'); echo 'after';");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("string \"skip platform\""), "{snapshot}");
    assert!(snapshot.contains("exit r"), "{snapshot}");
    assert!(!snapshot.contains("after"), "{snapshot}");
}

#[test]
fn lower_short_circuit_or_die_statement_terminates_only_failure_path() {
    let frontend = analyze_source("<?php $ok or die('failed'); echo 'after';");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("jump_if"), "{snapshot}");
    assert!(snapshot.contains("string \"failed\""), "{snapshot}");
    assert!(snapshot.contains("exit r"), "{snapshot}");
    assert!(snapshot.contains("string \"after\""), "{snapshot}");
    assert!(!snapshot.contains("unsupported"), "{snapshot}");
    assert!(!snapshot.contains("missing"), "{snapshot}");
}

#[test]
fn lower_assignment_or_die_statement_terminates_only_failure_path() {
    let frontend = analyze_source("<?php $q = msg_get_queue($id) or die('failed'); echo 'after';");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_function r"), "{snapshot}");
    assert!(snapshot.contains("\"msg_get_queue\""), "{snapshot}");
    assert!(snapshot.contains("jump_if"), "{snapshot}");
    assert!(snapshot.contains("string \"failed\""), "{snapshot}");
    assert!(snapshot.contains("exit r"), "{snapshot}");
    assert!(snapshot.contains("string \"after\""), "{snapshot}");
    assert!(!snapshot.contains("unsupported"), "{snapshot}");
    assert!(!snapshot.contains("missing"), "{snapshot}");
}

#[test]
fn lower_zero_arg_die_statement_terminates_without_operand() {
    let frontend = analyze_source("<?php function stop_now() { die(); echo 'after'; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("exit"), "{snapshot}");
    assert!(!snapshot.contains("unsupported"), "{snapshot}");
    assert!(!snapshot.contains("missing"), "{snapshot}");
}

#[test]
fn lower_casted_die_operand_terminates_script() {
    let frontend = analyze_source(
        "<?php function stop_now($message) { die( (string) $message ); echo 'after'; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("cast r"), "{snapshot}");
    assert!(snapshot.contains(" string "), "{snapshot}");
    assert!(snapshot.contains("exit r"), "{snapshot}");
    assert!(!snapshot.contains("unsupported"), "{snapshot}");
    assert!(!snapshot.contains("missing"), "{snapshot}");
}

#[test]
fn lower_wordpress_style_die_concat_terminates_script() {
    let frontend = analyze_source(
        "<?php die( '<h1>' . __( 'Requirements Not Met' ) . '</h1><p>' . $compat . '</p></body></html>' ); echo 'after';",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_function r"), "{snapshot}");
    assert!(snapshot.contains("\"__\""), "{snapshot}");
    assert!(snapshot.contains("binary r"), "{snapshot}");
    assert!(snapshot.contains("exit r"), "{snapshot}");
    assert!(!snapshot.contains("after"), "{snapshot}");
    assert!(!snapshot.contains("unsupported"), "{snapshot}");
    assert!(!snapshot.contains("missing"), "{snapshot}");
}

#[test]
fn include_construct_operand_keeps_full_concat_expression() {
    let frontend = analyze_source("<?php include __DIR__ . '/_data/child.php';");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(
        snapshot.matches("include r3 include r2").count(),
        1,
        "{snapshot}"
    );
    assert!(snapshot.contains(" concat "), "{snapshot}");
    assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
}

#[test]
fn label_and_goto_lower_to_jumps_without_unsupported_hir() {
    let frontend = analyze_source(
        "<?php $i = 0; start: $i++; if ($i < 3) { goto start; } goto done; echo 'skip'; done: echo $i;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("hir:label:start"), "{snapshot}");
    assert!(snapshot.contains("hir:label:done"), "{snapshot}");
    assert!(snapshot.contains("hir:goto:start"), "{snapshot}");
    assert!(snapshot.contains("hir:goto:done"), "{snapshot}");
    assert!(snapshot.matches("jump block:").count() >= 3, "{snapshot}");
    assert!(
        !snapshot.contains("E_PHP_IR_UNSUPPORTED_HIR_STATEMENT"),
        "{snapshot}"
    );
}

#[test]
fn predefined_constants_fold_in_compile_time_contexts() {
    let source = "<?php
            #[Attr(PHP_INT_MAX)]
            class C {
                public const MASK = E_ALL & ~E_DEPRECATED;
                public const ROOT = DIRECTORY_SEPARATOR . 'wp';
                public string $eol = PHP_EOL;
            }
            function boot($limit = PHP_INT_MAX, $path = DEFAULT_INCLUDE_PATH) {}
            ";
    let frontend = analyze_source(source);
    let result = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_text: Some(source.to_owned()),
            ..LoweringOptions::default()
        },
    );

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::Int(php_std::constants::PHP_INT_MAX)),
        "{:#?}",
        result.unit.constants
    );
    assert!(
        result.unit.constants.contains(&IrConstant::Int(
            php_std::constants::E_ALL & !php_std::constants::E_DEPRECATED
        )),
        "{:#?}",
        result.unit.constants
    );
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::String(php_std::constants::PHP_EOL.to_string())),
        "{:#?}",
        result.unit.constants
    );
    assert!(
        result.unit.constants.contains(&IrConstant::String(format!(
            "{}wp",
            php_std::constants::DIRECTORY_SEPARATOR
        ))),
        "{:#?}",
        result.unit.constants
    );

    let class = result
        .unit
        .classes
        .iter()
        .find(|class| class.name == "c")
        .expect("class C");
    assert_eq!(class.attributes[0].arguments.len(), 1);
    let function = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "boot")
        .expect("boot function");
    assert_eq!(
        function.params[0].default,
        Some(IrConstant::Int(php_std::constants::PHP_INT_MAX))
    );
    assert_eq!(
        function.params[1].default,
        Some(IrConstant::String(
            php_std::constants::DEFAULT_INCLUDE_PATH.to_string()
        ))
    );
}

#[test]
fn error_suppressed_variable_load_lowers_quietly() {
    let frontend = analyze_source("<?php echo @$missing;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("load_local_quiet"), "{snapshot}");
    assert!(!snapshot.contains("unsupported"), "{snapshot}");
}

#[test]
fn literals_are_interned_in_first_use_order() {
    let frontend = analyze_source("<?php echo 1, 1, \"x\", null, true, 1.5;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(
        result.unit.constants,
        vec![
            IrConstant::Null,
            IrConstant::Int(1),
            IrConstant::String("x".to_string()),
            IrConstant::Bool(true),
            IrConstant::Float(1.5)
        ]
    );
    assert!(
        result
            .unit
            .source_map
            .entries()
            .iter()
            .any(|entry| matches!(
                entry.target,
                crate::source_map::IrSourceMapTarget::Instruction { .. }
            ) && entry.origin.starts_with("hir:expr:"))
    );
}

#[test]
fn numeric_literal_separators_and_prefixes_lower_to_constants() {
    let frontend = analyze_source(
        "<?php echo 299_792_458, '|', 0xCAFE_F00D, '|', 0b0101_1111, '|', 0137_041, '|', 0_124;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::Int(299_792_458))
    );
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::Int(0xCAFE_F00D))
    );
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::Int(0b0101_1111))
    );
    assert!(result.unit.constants.contains(&IrConstant::Int(0o137_041)));
    assert!(result.unit.constants.contains(&IrConstant::Int(0o124)));
}

#[test]
fn oversized_decimal_integer_literals_lower_to_float_constants() {
    let frontend = analyze_source("<?php echo 18446744073709551616;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(
            result
                .unit
                .constants
                .iter()
                .any(|constant| matches!(constant, IrConstant::Float(value) if *value == 18446744073709551616_f64))
        );
}

#[test]
fn literals_unescape_php_string_bytes_without_unicode_normalization() {
    let frontend = analyze_source("<?php echo \"a\\n\", 'b\\\\c';");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::String("a\n".to_string()))
    );
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::String("b\\c".to_string()))
    );
    assert_eq!(
        quoted_literal_body(r#""\0\x0n\141""#),
        Some(b"\0\0na".to_vec())
    );
    assert_eq!(
        quoted_literal_body(r#""\u{41}\xFF""#),
        Some(vec![b'A', 0xff])
    );
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::String("a\n".to_string()))
    );
}

#[test]
fn literals_keep_binary_php_string_bytes() {
    let frontend = analyze_source("<?php echo \"\\xFF\\0\";");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::StringBytes(vec![0xff, 0]))
    );
}

#[test]
fn literals_lower_heredoc_and_nowdoc_bodies() {
    let frontend = analyze_source(
        "<?php $a = <<<TXT\nhello\\n\nTXT; $b = <<<'TXT'\nhello\\n\nTXT; echo $a, $b;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::String("hello\n".to_string()))
    );
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::String("hello\\n".to_string()))
    );

    let frontend = analyze_source("<?php $a = <<<'TXT'\n  <?php echo 1;\n  TXT; echo $a;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::String("<?php echo 1;".to_string()))
    );

    let frontend = analyze_source("<?php $a = <<<TXT\n\\\"quotes\nTXT; $b = \"\\\"quotes\";");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::String("\\\"quotes".to_string()))
    );
    assert!(
        result
            .unit
            .constants
            .contains(&IrConstant::String("\"quotes".to_string()))
    );
}

#[test]
fn literals_lower_simple_interpolation_to_concat() {
    let frontend = analyze_source("<?php $counter = 3; echo \"-- Iteration $counter --\\n\";");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains(" concat "), "{snapshot}");
    assert!(snapshot.contains("cast r"), "{snapshot}");
    assert!(snapshot.contains(" string "), "{snapshot}");
    assert!(snapshot.contains("local:0 $counter"), "{snapshot}");
    assert!(
        interpolated_literal_parts("\"a {$counter} b\"").is_some(),
        "braced simple interpolation should be recognized"
    );
}

#[test]
fn integer_braced_variable_names_lower_to_stable_local_slot() {
    let frontend = analyze_source("<?php ${10} = 42; echo ${10};");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("local:0 $10"), "{snapshot}");
    assert_eq!(snapshot.matches("local:0 $10").count(), 1, "{snapshot}");
}

#[test]
fn deprecated_dollar_brace_interpolation_lowers_diagnostic() {
    let frontend = analyze_source("<?php $counter = 3; echo \"-- Iteration ${counter} --\\n\";");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("emit_diagnostic Deprecation"),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("E_PHP_RUNTIME_DEPRECATED_DOLLAR_BRACE_INTERPOLATION"),
        "{snapshot}"
    );
    assert!(snapshot.contains(" concat "), "{snapshot}");
    assert!(snapshot.contains("local:0 $counter"), "{snapshot}");

    let parts =
        interpolated_literal_parts("\"a {$counter} ${counter} b\"").expect("interpolated parts");
    assert!(matches!(
        &parts[1],
        InterpolatedPart::Variable {
            deprecated_dollar_brace: false,
            ..
        }
    ));
    assert!(matches!(
        &parts[3],
        InterpolatedPart::Variable {
            deprecated_dollar_brace: true,
            ..
        }
    ));
}

#[test]
fn simple_array_dim_interpolation_lowers_fetch_dim() {
    let frontend = analyze_source(
        "<?php $needles = ['Hello world']; $i = 0; echo \"Position of '$needles[$i]'\\n\";",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    assert!(snapshot.contains("local:0 $needles"), "{snapshot}");
    assert!(snapshot.contains("local:1 $i"), "{snapshot}");

    let parts = interpolated_literal_parts("\"Position of '$needles[$i]'\"").expect("parts");
    assert!(matches!(
        &parts[1],
        InterpolatedPart::Variable {
            name,
            dim: Some(InterpolatedDim::Variable(dim)),
            ..
        } if name == "needles" && dim == "i"
    ));
}

#[test]
fn braced_array_dim_chain_interpolation_lowers_fetch_dim_chain() {
    let frontend = analyze_source(
        "<?php $submenu_items = [[0, 1, 'index.php']]; echo \"{$submenu_items[0][2]}\";",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.matches("fetch_dim r").count() >= 2, "{snapshot}");
    assert!(snapshot.contains("local:0 $submenu_items"), "{snapshot}");
    assert!(snapshot.contains("int 0"), "{snapshot}");
    assert!(snapshot.contains("int 2"), "{snapshot}");

    let parts = interpolated_literal_parts("\"{$submenu_items[0][2]}\"").expect("parts");
    assert!(matches!(
        &parts[1],
        InterpolatedPart::Variable {
            name,
            dim: Some(InterpolatedDim::Int(0)),
            dim_tail,
            ..
        } if name == "submenu_items"
            && matches!(dim_tail.as_slice(), [InterpolatedDim::Int(2)])
    ));
}

#[test]
fn braced_method_call_interpolation_lowers_call_method() {
    let frontend = analyze_source(
        "<?php try { throw new Error('bad'); } catch (Error $ex) { echo \"{$ex->getCode()}: {$ex->getMessage()}\"; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_method r"), "{snapshot}");
    assert!(snapshot.contains("\"getCode\""), "{snapshot}");
    assert!(snapshot.contains("\"getMessage\""), "{snapshot}");

    let parts =
        interpolated_literal_parts("\"{$ex->getCode()}: {$ex->getMessage()}\"").expect("parts");
    assert!(matches!(
        &parts[1],
        InterpolatedPart::MethodCall { receiver, method }
            if receiver == "ex" && method == "getCode"
    ));
    assert!(matches!(
        &parts[3],
        InterpolatedPart::MethodCall { receiver, method }
            if receiver == "ex" && method == "getMessage"
    ));
}

#[test]
fn interpolated_dynamic_method_name_lowers_to_callable_pair() {
    let frontend = analyze_source(
        "<?php class IriProbe { function __get($name) { return $this->{\"get_$name\"}(); } function get_iri() { return 'iri'; } } echo (new IriProbe())->iri;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_callable r"), "{snapshot}");
    assert!(snapshot.contains("binary r"), "{snapshot}");
    assert!(snapshot.contains("concat"), "{snapshot}");
    assert!(snapshot.contains("string \"get_\""), "{snapshot}");
    assert!(!snapshot.contains("\"get_$name\""), "{snapshot}");
}

#[test]
fn simple_property_interpolation_lowers_fetch_property() {
    let frontend = analyze_source(
        "<?php class D { private $counter = 2; function f() { echo \"($this->counter)\"; echo \"({$this->counter})\"; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_property r"), "{snapshot}");
    assert!(snapshot.contains("local:0 $this"), "{snapshot}");
    assert!(snapshot.contains("$counter"), "{snapshot}");

    let parts = interpolated_literal_parts("\"($this->counter)\"").expect("parts");
    assert!(matches!(
        &parts[1],
        InterpolatedPart::Property {
            receiver, property, ..
        }
            if receiver == "this" && property == "counter"
    ));
    let parts = interpolated_literal_parts("\"({$this->counter})\"").expect("parts");
    assert!(matches!(
        &parts[1],
        InterpolatedPart::Property {
            receiver, property, ..
        }
            if receiver == "this" && property == "counter"
    ));
}

#[test]
fn property_dim_interpolation_lowers_fetch_dim() {
    let frontend = analyze_source(
        "<?php class D { private $rewrite = ['slug' => 'category']; function f() { echo \"{$this->rewrite['slug']}\"; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_property r"), "{snapshot}");
    assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    assert!(snapshot.contains("$rewrite"), "{snapshot}");
    assert!(snapshot.contains("string \"slug\""), "{snapshot}");

    let parts = interpolated_literal_parts("\"{$this->rewrite['slug']}\"").expect("parts");
    assert!(matches!(
        &parts[1],
        InterpolatedPart::Property {
            receiver,
            property,
            property_tail,
            dim: Some(InterpolatedDim::String(dim)),
        } if receiver == "this" && property == "rewrite" && property_tail.is_empty() && dim == "slug"
    ));
}

#[test]
fn braced_property_chain_interpolation_lowers_fetch_property_chain() {
    let frontend = analyze_source(
        "<?php class Screen { public $id = 'dashboard'; } class D { public $screen; function __construct() { $this->screen = new Screen(); } function f() { echo \"manage_{$this->screen->id}_columns\"; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.matches("fetch_property r").count() >= 2,
        "{snapshot}"
    );
    assert!(snapshot.contains("$screen"), "{snapshot}");
    assert!(snapshot.contains("$id"), "{snapshot}");

    let parts =
        interpolated_literal_parts("\"manage_{$this->screen->id}_columns\"").expect("parts");
    assert!(matches!(
        &parts[1],
        InterpolatedPart::Property {
            receiver,
            property,
            property_tail,
            dim: None,
        } if receiver == "this" && property == "screen" && property_tail.len() == 1 && property_tail[0] == "id"
    ));
}

#[test]
fn static_new_object_preserves_display_class_name_for_autoload() {
    let frontend = analyze_source("<?php new TestX;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("new_object r"), "{snapshot}");
    assert!(
        snapshot.contains("\"testx\" display=\"TestX\""),
        "{snapshot}"
    );
}

#[test]
fn namespaced_new_object_preserves_fully_qualified_display_name_for_autoload() {
    let frontend = analyze_source("<?php namespace SimplePie; new Exception;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains(r#""simplepie\\exception" display="SimplePie\\Exception""#),
        "{snapshot}"
    );
}

#[test]
fn aliased_parent_declaration_preserves_imported_display_name_for_autoload() {
    let frontend = analyze_source(
        "<?php namespace SimplePie\\HTTP; use SimplePie\\Exception as SimplePieException; final class ClientException extends SimplePieException {}",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    let class = result
        .unit
        .classes
        .iter()
        .find(|class| class.name == "simplepie\\http\\clientexception")
        .expect("client exception class should be lowered");
    assert_eq!(class.parent.as_deref(), Some("simplepie\\exception"));
    assert_eq!(
        class.parent_display_name.as_deref(),
        Some("SimplePie\\Exception")
    );
}

#[test]
fn qualified_parent_declaration_preserves_source_display_name_for_autoload() {
    let frontend =
        analyze_source("<?php class WP_HTTP_Requests_Hooks extends WpOrg\\Requests\\Hooks {}");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    let class = result
        .unit
        .classes
        .iter()
        .find(|class| class.name == "wp_http_requests_hooks")
        .expect("requests hook bridge class should be lowered");
    assert_eq!(class.parent.as_deref(), Some("wporg\\requests\\hooks"));
    assert_eq!(
        class.parent_display_name.as_deref(),
        Some("WpOrg\\Requests\\Hooks")
    );
}

#[test]
fn namespaced_parent_declaration_preserves_namespace_display_name_for_autoload() {
    let frontend = analyze_source(
        "<?php namespace WordPress\\AiClientDependencies\\Http\\Discovery; final class Psr18ClientDiscovery extends ClassDiscovery {}",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    let class = result
        .unit
        .classes
        .iter()
        .find(|class| {
            class.name == "wordpress\\aiclientdependencies\\http\\discovery\\psr18clientdiscovery"
        })
        .expect("PSR-18 discovery class should be lowered");
    assert_eq!(
        class.parent.as_deref(),
        Some("wordpress\\aiclientdependencies\\http\\discovery\\classdiscovery")
    );
    assert_eq!(
        class.parent_display_name.as_deref(),
        Some("WordPress\\AiClientDependencies\\Http\\Discovery\\ClassDiscovery")
    );
}

#[test]
fn new_self_lowers_to_declaring_class_name() {
    let frontend = analyze_source(
        "<?php class C { private static $instance = null; public static function get_instance() { if ( null === self::$instance ) { self::$instance = new self(); } return self::$instance; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("new_object r"), "{snapshot}");
    assert!(snapshot.contains("\"c\" display=\"C\""), "{snapshot}");
    assert!(!snapshot.contains("\"self\""), "{snapshot}");
}

#[test]
fn self_class_constant_lowers_to_declaring_class_name() {
    let frontend = analyze_source(
        "<?php namespace WpOrg\\Requests; final class Autoload { public static function register() { spl_autoload_register([self::class, 'load'], true); } public static function load($class) {} }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("string \"WpOrg\\\\Requests\\\\Autoload\""),
        "{snapshot}"
    );
    assert!(!snapshot.contains("string \"self\""), "{snapshot}");
}

#[test]
fn locals_lower_variable_assignment_fetch_and_compound_ops() {
    let frontend = analyze_source("<?php $a = 1; $a += 2; echo $a;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let function = &result.unit.functions[0];
    assert_eq!(function.locals, vec!["a"]);
    assert_eq!(function.local_count, 1);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("local:0 $a"));
    assert!(snapshot.contains("store_local local:0"));
    assert!(snapshot.contains("load_local r"));
    assert!(snapshot.contains("binary r"));
}

#[test]
fn null_coalescing_assignment_lowers_for_locals_and_dimensions() {
    let frontend = analyze_source(
        "<?php $value ??= 'fallback'; $url = []; $url['path'] ??= ''; echo $url['path'];",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("isset_local r"), "{snapshot}");
    assert!(snapshot.contains("isset_dim r"), "{snapshot}");
    assert!(snapshot.contains("assign_dim r"), "{snapshot}");
    assert!(snapshot.contains("store_local local:0"), "{snapshot}");
}

#[test]
fn null_coalescing_expression_assignment_lowers_for_static_local_cache() {
    let frontend = analyze_source(
        "<?php function f() { static $skipStrategy; $skipStrategy ?? $skipStrategy = class_exists('A') ? false : 'A'; return $skipStrategy; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("load_local_quiet r"), "{snapshot}");
    assert!(snapshot.contains("compare r"), "{snapshot}");
    assert!(snapshot.contains("store_local local:"), "{snapshot}");
}

#[test]
fn null_coalescing_expression_lowers_static_property_dim_fetch_quietly() {
    let frontend = analyze_source(
        "<?php class S { private static array $items = []; public string $id = 'dashboard'; function f() { return self::$items[$this->id] ?? ''; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_static_property"), "{snapshot}");
    assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    assert!(snapshot.contains("quiet=true"), "{snapshot}");
}

#[test]
fn dim_fetch_lowers_binary_index_expression() {
    let frontend = analyze_source(
        "<?php $args_array = array(array(0), array(-1, 1)); $counter = 1; var_dump($args_array[$counter - 1]);",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("local:0 $args_array"), "{snapshot}");
    assert!(snapshot.contains("local:1 $counter"), "{snapshot}");
    assert!(snapshot.contains("binary r"), "{snapshot}");
    assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    assert!(snapshot.contains("mode=read"), "{snapshot}");
}

#[test]
fn array_literal_preserves_nested_keyed_array_as_append_value() {
    let frontend = analyze_source("<?php $xs = array(array(12 => \"12twelve\"));");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("array_insert"), "{snapshot}");
    assert!(
        !snapshot.contains("array element is missing its value"),
        "{snapshot}"
    );
}

#[test]
fn large_constant_array_literal_lowers_to_one_constant_load() {
    let values = (0..128)
        .map(|index| format!("\"value-{index}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let frontend = analyze_source(&format!("<?php function values() {{ return [{values}]; }}"));
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let function = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "values")
        .expect("values function");
    assert_eq!(function.register_count, 1);
    assert!(
        function.blocks.iter().all(
            |block| block.instructions.iter().all(|instruction| !matches!(
                instruction.kind,
                InstructionKind::NewArray { .. } | InstructionKind::ArrayInsert { .. }
            ))
        )
    );
    assert!(function.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.kind, InstructionKind::LoadConst { .. }))
    }));
}

#[test]
fn deeply_populated_constant_array_uses_total_shape_for_compaction() {
    let nested = |prefix: &str| {
        (0..64)
            .map(|index| format!("\"{prefix}-{index}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let frontend = analyze_source(&format!(
        "<?php function values() {{ return [[{}], [{}]]; }}",
        nested("left"),
        nested("right"),
    ));
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let function = result
        .unit
        .functions
        .iter()
        .find(|function| function.name == "values")
        .expect("values function");
    assert_eq!(function.register_count, 1);
    assert!(function.blocks.iter().all(|block| {
        block.instructions.iter().all(|instruction| {
            !matches!(
                instruction.kind,
                InstructionKind::NewArray { .. } | InstructionKind::ArrayInsert { .. }
            )
        })
    }));
}

#[test]
fn locals_lower_pre_and_post_increment_with_distinct_return_registers() {
    let frontend = analyze_source("<?php $a = 1; echo $a++; echo ++$a;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(result.unit.functions[0].locals, vec!["a"]);
    assert!(snapshot.contains("local:0 $a"));
    assert!(snapshot.matches("store_local local:0").count() >= 3);
}

#[test]
fn control_flow_lowers_if_else_to_readable_blocks() {
    let frontend = analyze_source("<?php if (true) { echo \"t\"; } else { echo \"f\"; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("jump_if r"));
    assert!(snapshot.contains("block:1"));
    assert!(snapshot.contains("block:2"));
    assert!(snapshot.contains("string \"t\""));
    assert!(snapshot.contains("string \"f\""));
}

#[test]
fn ternary_after_if_uses_explicit_false_target() {
    let frontend = analyze_source(
        "<?php function cmp($a, $b) { if ($a == $b) { return 0; } return ($a < $b) ? -1 : 1; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("jump_if r"));
    assert!(snapshot.contains(" block:"));
}

#[test]
fn control_flow_lowers_loops_and_break_continue_targets() {
    let frontend = analyze_source(
        "<?php $i = 0; while ($i < 4) { $i++; if ($i == 2) { continue; } if ($i == 3) { break; } echo $i; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("jump_if r"));
    assert!(snapshot.matches("jump block:").count() >= 3);
    assert!(snapshot.contains("compare r"));
}

#[test]
fn control_flow_lowers_goto_to_label_blocks() {
    let frontend = analyze_source(
        "<?php function scan($i) { if ($i > 0) { goto found; } echo \"skip\"; found: return $i; } echo scan(1);",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("jump block:"), "{snapshot}");
    assert!(snapshot.contains("string \"skip\""), "{snapshot}");
    assert!(snapshot.contains("function \"scan\""), "{snapshot}");
}

#[test]
fn for_loop_lowers_two_initializer_expressions() {
    let frontend = analyze_source("<?php for ($x = 0, $count = 0; $x < 3; $x++) { $count++; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("local:0 $x"), "{snapshot}");
    assert!(snapshot.contains("local:1 $count"), "{snapshot}");
    assert!(snapshot.matches("store_local").count() >= 2, "{snapshot}");
    assert!(
        !snapshot.contains("E_PHP_IR_UNSUPPORTED_FOR_HEADER_MULTI_EXPR"),
        "{snapshot}"
    );
}

#[test]
fn for_loop_lowers_multi_expression_header_sections() {
    let frontend = analyze_source("<?php for ($i = 0, $j = 3; $i < 3; $i++, $j--) { echo $i; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("local:0 $i"), "{snapshot}");
    assert!(snapshot.contains("local:1 $j"), "{snapshot}");
    assert!(snapshot.matches("store_local").count() >= 2, "{snapshot}");
    assert!(
        !snapshot.contains("E_PHP_IR_UNSUPPORTED_FOR_HEADER_MULTI_EXPR"),
        "{snapshot}"
    );
}

#[test]
fn foreach_lowers_keyless_list_destructuring_value_target() {
    let frontend = analyze_source("<?php foreach ([[1, 2]] as [$val, $precision]) { echo $val; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("$val"), "{snapshot}");
    assert!(snapshot.contains("$precision"), "{snapshot}");
    assert!(snapshot.contains("fetch_dim"), "{snapshot}");
    assert!(snapshot.matches("store_local").count() >= 2, "{snapshot}");
    assert!(
        !snapshot.contains("foreach value target must be a simple local variable"),
        "{snapshot}"
    );
}

#[test]
fn foreach_lowers_list_destructuring_hole_offsets() {
    let source = "<?php foreach ([[1, 2, 3]] as [$first, , $third]) { echo $third; }";
    let frontend = analyze_source(source);
    let result = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_text: Some(source.to_owned()),
            ..LoweringOptions::default()
        },
    );

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("$third"), "{snapshot}");
    assert!(snapshot.contains("int 2"), "{snapshot}");
    assert!(
        !snapshot.contains("E_PHP_IR_DESTRUCTURING_HOLE_INDEX_GAP"),
        "{snapshot}"
    );
}

#[test]
fn list_assignment_lowers_property_targets() {
    let frontend = analyze_source(
        "<?php class D { public function __construct(...$args) { list($this->handle, $this->src) = $args; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_dim"), "{snapshot}");
    assert!(snapshot.contains("assign_property"), "{snapshot}");
    assert!(
        !snapshot.contains("only simple variable assignment"),
        "{snapshot}"
    );
}

#[test]
fn list_assignment_lowers_array_dimension_targets() {
    let frontend = analyze_source(
        "<?php $data = []; list($data['width'], $data['height']) = image_constrain_size_for_editor($data['width'], $data['height'], $size);",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("fetch_dim"), "{snapshot}");
    assert!(snapshot.contains("assign_dim"), "{snapshot}");
    assert!(
        !snapshot.contains("only simple variable assignment"),
        "{snapshot}"
    );
}

#[test]
fn array_destructuring_assignment_lowers_string_keys() {
    let frontend =
        analyze_source("<?php ['namespace' => $ns, 'value' => $path] = $entry; echo $ns, $path;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("$ns"), "{snapshot}");
    assert!(snapshot.contains("$path"), "{snapshot}");
    assert!(snapshot.contains("string \"namespace\""), "{snapshot}");
    assert!(snapshot.contains("string \"value\""), "{snapshot}");
    assert!(snapshot.contains("fetch_dim"), "{snapshot}");
    assert!(snapshot.matches("store_local").count() >= 2, "{snapshot}");
    assert!(
        !snapshot.contains("only simple variable assignment"),
        "{snapshot}"
    );
}

#[test]
fn list_assignment_lowers_string_keyed_array_destructuring() {
    let frontend = analyze_source(
        "<?php [ 'prefix' => $attr_prefix, 'suffix' => $suffix, 'unique_id' => $unique_id] = $parts;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("$attr_prefix"), "{snapshot}");
    assert!(snapshot.contains("$suffix"), "{snapshot}");
    assert!(snapshot.contains("$unique_id"), "{snapshot}");
    assert!(snapshot.contains("string \"prefix\""), "{snapshot}");
    assert!(snapshot.contains("string \"suffix\""), "{snapshot}");
    assert!(snapshot.contains("string \"unique_id\""), "{snapshot}");
    assert!(snapshot.matches("fetch_dim").count() >= 3, "{snapshot}");
    assert!(
        !snapshot.contains("only simple variable assignment"),
        "{snapshot}"
    );
}

#[test]
fn switch_match_lowers_switch_fallthrough_and_match_error() {
    let frontend = analyze_source(
        "<?php $x = 1; switch ($x) { case 0: echo \"zero\"; case 1: echo \"one\"; break; default: echo \"default\"; } echo match ($x) { 0 => \"zero\" };",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("jump_if r"));
    assert!(snapshot.contains("equal"));
    assert!(snapshot.contains("identical"));
    assert!(snapshot.contains("runtime_error \"E_PHP_VM_UNHANDLED_MATCH\""));
    assert!(snapshot.matches("jump block:").count() >= 2);
    assert!(snapshot.contains("string \"zero\""));
    assert!(snapshot.contains("string \"one\""));
}

#[test]
fn functions_lower_named_declaration_table_params_and_call() {
    let frontend = analyze_source("<?php function add($a, $b) { return $a + $b; } echo add(2, 3);");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.functions.len(), 2);
    assert_eq!(result.unit.function_table.len(), 1);
    assert_eq!(result.unit.function_table[0].name, "add");
    assert_eq!(result.unit.functions[1].params.len(), 2);
    assert_eq!(result.unit.functions[1].locals, vec!["a", "b"]);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("function_name \"add\" => function:1"));
    assert!(snapshot.contains("call_function r"));
    assert!(snapshot.contains("\"add\""));
}

#[test]
fn functions_lower_namespaced_declaration_table_and_call() {
    let frontend =
        analyze_source("<?php namespace PerformanceIC; function hot() { return 2; } echo hot();");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.function_table.len(), 1);
    assert_eq!(result.unit.function_table[0].name, "performanceic\\hot");
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("function_name \"performanceic\\\\hot\" => function:1"));
    assert!(snapshot.contains("\"performanceic\\\\hot\""));
}

#[test]
fn namespaced_magic_constant_lowers_for_top_level_and_functions() {
    let frontend = analyze_source(
        "<?php namespace Demo\\Calls; function ns() { return __NAMESPACE__; } echo __NAMESPACE__, \"|\", ns();",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("const:1 string \"Demo\\\\Calls\""),
        "{snapshot}"
    );
    assert!(
        snapshot.matches("load_const r0 const:1").count() >= 2,
        "{snapshot}"
    );
}

#[test]
fn conditional_duplicate_functions_keep_bodies_without_duplicate_lookup_entries() {
    let frontend = analyze_source(
        "<?php if (false) : function branch_dup() { return 'no'; } else : function branch_dup() { return 'yes'; } endif;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.functions.len(), 3);
    assert_eq!(result.unit.function_table.len(), 0);
    assert_eq!(
        result
            .unit
            .functions
            .iter()
            .filter(|function| function.name == "branch_dup")
            .count(),
        2
    );
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(snapshot.matches("function_name \"branch_dup\"").count(), 0);
}

#[test]
fn conditional_function_declaration_emits_runtime_declare() {
    let frontend = analyze_source(
        "<?php if (true) { function branch_runtime() { return 'yes'; } } echo branch_runtime();",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.function_table.len(), 0);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("declare_function \"branch_runtime\""));
}

#[test]
fn namespaced_conditional_function_declaration_emits_runtime_declare() {
    let frontend = analyze_source(
        "<?php namespace Sodium; if (!is_callable('\\\\Sodium\\\\bin2hex')) { function bin2hex($string) { return ParagonIE_Sodium_Compat::bin2hex($string); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.function_table.len(), 0);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("declare_function \"sodium\\\\bin2hex\""),
        "{snapshot}"
    );
}

#[test]
fn call_arg_property_dimension_emits_by_ref_metadata() {
    let frontend = analyze_source(
        "<?php class C { public $iterations = [[1, 2]]; function run($i) { next($this->iterations[$i]); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("by_ref_property_dim="), "{snapshot}");
    assert!(snapshot.contains("mode=lvalue"), "{snapshot}");
}

#[test]
fn unresolved_call_preserves_plain_property_location_for_runtime_signature() {
    let frontend = analyze_source(
        "<?php class C { public $value = []; function run() { return later($this->value); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("\"later\""), "{snapshot}");
    assert!(snapshot.contains("by_ref_property="), "{snapshot}");
}

#[test]
fn static_method_call_preserves_dimension_location_for_runtime_signature() {
    let frontend = analyze_source(
        "<?php class C { static function mutate(&$value) {} static function run($data) { static::mutate($data['settings']); } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_static_method"), "{snapshot}");
    assert!(snapshot.contains("by_ref_dim="), "{snapshot}");
    assert!(snapshot.contains("mode=lvalue"), "{snapshot}");
}

#[test]
fn direct_builtin_call_uses_generated_by_ref_metadata() {
    let frontend = analyze_source("<?php $status = -1; pcntl_waitpid(123, $status); echo $status;");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_function r"), "{snapshot}");
    assert!(snapshot.contains("\"pcntl_waitpid\""), "{snapshot}");
    assert!(snapshot.contains("by_ref=local:"), "{snapshot}");
}

#[test]
fn array_literal_by_ref_local_dimension_lowers_through_hidden_local() {
    let frontend = analyze_source(
        "<?php $credentials = ['user_login' => 'u']; $args = array(&$credentials['user_login']);",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("__phrust:array-ref-dim"), "{snapshot}");
    assert!(snapshot.contains("bind_reference_from_dim"), "{snapshot}");
    assert!(snapshot.contains("array_insert"), "{snapshot}");
    assert!(snapshot.contains("by_ref=local:"), "{snapshot}");
}

#[test]
fn comparison_assignment_idiom_lowers_assignment_then_compare() {
    let frontend =
        analyze_source("<?php while ( false !== $file = readdir( $dh ) ) { echo $file; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_function r"), "{snapshot}");
    assert!(snapshot.contains("store_local local:"), "{snapshot}");
    assert!(snapshot.contains("compare r"), "{snapshot}");
    assert!(snapshot.contains("not_identical"), "{snapshot}");
}

#[test]
fn logical_or_strict_comparison_assignment_lowers_with_short_circuit() {
    let frontend = analyze_source(
        "<?php if (empty($url) || !is_readable($url) || false === $filebody = file_get_contents($url)) { echo 'bad'; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_function r"), "{snapshot}");
    assert!(snapshot.contains("store_local local:"), "{snapshot}");
    assert!(snapshot.contains("compare r"), "{snapshot}");
    assert!(snapshot.contains("identical"), "{snapshot}");
    assert!(snapshot.contains("jump_if"), "{snapshot}");
}

#[test]
fn unary_not_assignment_idiom_lowers_assignment_then_not() {
    let frontend = analyze_source(
        "<?php function maybe_post($id) { if ( !$post = get_post($id) ) { return false; } return $post; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_function r"), "{snapshot}");
    assert!(snapshot.contains("store_local local:"), "{snapshot}");
    assert!(snapshot.contains("unary r"), "{snapshot}");
    assert!(snapshot.contains("not"), "{snapshot}");
}

#[test]
fn logical_or_not_assignment_idiom_lowers_with_short_circuit() {
    let frontend = analyze_source(
        "<?php if ( ('attachment' != $_post->post_type) || !$url = wp_get_attachment_url($_post->ID) ) { return false; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("not_equal"), "{snapshot}");
    assert!(snapshot.contains("jump_if r"), "{snapshot}");
    assert!(snapshot.contains("store_local local:"), "{snapshot}");
    assert!(snapshot.contains("unary r"), "{snapshot}");
}

#[test]
fn logical_and_assignment_idiom_lowers_with_short_circuit() {
    let frontend = analyze_source(
        "<?php if ( !$fullsize && $src = wp_get_attachment_thumb_url($post->ID) ) { return $src; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("jump_if r"), "{snapshot}");
    assert!(snapshot.contains("store_local local:"), "{snapshot}");
    assert!(snapshot.contains("call_function r"), "{snapshot}");
}

#[test]
fn logical_xor_lowers_to_bool_casts_and_not_identical_compare() {
    let frontend = analyze_source(
        "<?php function f($noopen, $noclose) { if ($noopen xor $noclose) { return 'one'; } return 'both-or-none'; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("cast r"), "{snapshot}");
    assert!(snapshot.contains("bool"), "{snapshot}");
    assert!(snapshot.contains("compare r"), "{snapshot}");
    assert!(snapshot.contains("not_identical"), "{snapshot}");
}

#[test]
fn append_then_keyed_dimension_assignment_lowers_through_temp_array() {
    let frontend = analyze_source(
        "<?php $patternses = array(); $type = 'x'; $regex = 'r'; $patternses[][ $type ] = $regex;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("__phrust:append-nested-dim"),
        "{snapshot}"
    );
    assert!(snapshot.contains("assign_dim"), "{snapshot}");
    assert!(snapshot.contains("append_dim"), "{snapshot}");
}

#[test]
fn keyed_dimension_then_append_assignment_lowers_directly() {
    let frontend =
        analyze_source("<?php $cache = array(); $id = 1; $key = 'x'; $cache[$id][$key][] = 'v';");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        !snapshot.contains("__phrust:append-nested-dim"),
        "{snapshot}"
    );
    assert!(snapshot.contains("append_dim"), "{snapshot}");
}

#[test]
fn dim_to_dim_reference_assignment_lowers_through_hidden_source() {
    let frontend =
        analyze_source("<?php $types[$name] =& $icon_files[$file]; $icon_files[$file] = 'x';");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("__phrust:dim-ref-source"), "{snapshot}");
    assert!(snapshot.contains("bind_reference_from_dim"), "{snapshot}");
    assert!(snapshot.contains("bind_reference_dim"), "{snapshot}");
}

#[test]
fn dim_reference_assignment_allows_property_fetch_dimension_keys() {
    let frontend = analyze_source(
        "<?php foreach ((array) $terms as $key => $term) { $terms_by_id[$term->term_id] =& $terms[$key]; }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("__phrust:dim-ref-source"), "{snapshot}");
    assert!(snapshot.contains("fetch_property"), "{snapshot}");
    assert!(snapshot.contains("bind_reference_from_dim"), "{snapshot}");
    assert!(snapshot.contains("bind_reference_dim"), "{snapshot}");
}

#[test]
fn property_dim_to_property_dim_reference_assignment_lowers_through_hidden_source() {
    let frontend = analyze_source(
        "<?php class C { public $data = array('links' => array()); function run($key) { $this->data['links'][$key] =& $this->data['links']['base' . $key]; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("__phrust:property-dim-ref-source"),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("bind_reference_from_property_dim"),
        "{snapshot}"
    );
    assert!(
        snapshot.contains("bind_reference_property_dim"),
        "{snapshot}"
    );
}

#[test]
fn property_dimension_reference_assignment_allows_method_call_keys() {
    let frontend = analyze_source(
        "<?php class MO { public array $entries = []; function add($entry) { $this->entries[$entry->key()] = &$entry; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("call_method"), "{snapshot}");
    assert!(
        snapshot.contains("bind_reference_property_dim"),
        "{snapshot}"
    );
    assert!(
        !snapshot.contains("object-property references are a known gap"),
        "{snapshot}"
    );
}

#[test]
fn local_reference_assignment_lowers_method_return_reference() {
    let frontend = analyze_source(
        "<?php class MO { function add($original, $translation) { $entry = &$this->make_entry($original, $translation); return $entry; } public function &make_entry($original, $translation) { $entry = new stdClass(); return $entry; } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(
        snapshot.contains("bind_reference_method_call"),
        "{snapshot}"
    );
    assert!(
        !snapshot.contains("object-property references are a known gap"),
        "{snapshot}"
    );
}

#[test]
fn nested_conditional_function_declarations_emit_once_per_branch() {
    let frontend = analyze_source(
        "<?php if (!function_exists('utf8_encode')) : if (extension_loaded('mbstring')) : function utf8_encode($value) { return 'mb'; } else : function utf8_encode($value) { return 'fallback'; } endif; endif;",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.function_table.len(), 0);
    let snapshot = result.unit.to_snapshot_text();
    assert_eq!(
        snapshot.matches("declare_function \"utf8_encode\"").count(),
        2,
        "{snapshot}"
    );
}

#[test]
fn nested_conditional_function_inside_function_emits_runtime_declare() {
    let frontend = analyze_source(
        "<?php function outer($flag) { if ($flag) { if (!function_exists('lowercase_octets')) { function lowercase_octets($matches) { return strtolower($matches[0]); } } } }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.function_table.len(), 1);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("function_name \"outer\""), "{snapshot}");
    assert!(
        snapshot.contains("declare_function \"lowercase_octets\""),
        "{snapshot}"
    );
    assert_eq!(
        snapshot
            .matches("function_name \"lowercase_octets\"")
            .count(),
        0,
        "{snapshot}"
    );
}

#[test]
fn direct_nested_function_inside_function_emits_runtime_declare() {
    let frontend = analyze_source(
        "<?php function outer() { function nested_helper() { return 'ok'; } return nested_helper(); }",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.unit.function_table.len(), 1);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("function_name \"outer\""), "{snapshot}");
    assert!(
        snapshot.contains("declare_function \"nested_helper\""),
        "{snapshot}"
    );
    assert_eq!(
        snapshot.matches("function_name \"nested_helper\"").count(),
        0,
        "{snapshot}"
    );
}

#[test]
fn closures_lower_with_stable_function_id_and_capture_dump() {
    let frontend =
        analyze_source("<?php $x = 2; $f = function($y) use ($x) { return $x + $y; }; echo $f(3);");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("make_closure r"));
    assert!(snapshot.contains("function:1"));
    assert!(snapshot.contains("\"x\"=local:0 by_ref=false"));
    assert!(snapshot.contains("capture \"x\" local:0 by_ref=false"));
    assert!(snapshot.contains("call_callable r"));
}

#[test]
fn pipe_lowers_first_class_callable_to_stable_callable_ir() {
    let frontend = analyze_source(
        "<?php function plus1($x) { return $x + 1; } echo 2 |> plus1(...); echo \" a \" |> trim(...);",
    );
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("resolve_callable"));
    assert!(snapshot.contains("function_name \"plus1\""));
    assert!(snapshot.contains("function_name \"trim\""));
    assert!(snapshot.contains("pipe r"));
}

#[test]
fn lower_generator_known_gap_is_machine_readable() {
    let frontend = analyze_source("<?php function gen() { yield 1; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(result.unit.to_snapshot_text().contains("yield r"));
}

#[test]
fn lower_generator_method_to_ir_instruction() {
    let frontend = analyze_source("<?php class C { public function gen() { yield $this->x; } }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains("function \"C::gen\""), "{snapshot}");
    assert!(snapshot.contains("yield r"), "{snapshot}");
}

#[test]
fn lower_yield_from_to_ir_instruction() {
    let frontend = analyze_source("<?php function gen($items) { yield from $items; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(result.unit.to_snapshot_text().contains("yield_from r"));
}

#[test]
fn lower_eval_to_ir_instruction() {
    let frontend = analyze_source("<?php $code = 'echo '; eval($code . '1;');");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let snapshot = result.unit.to_snapshot_text();
    assert!(snapshot.contains(" concat "), "{snapshot}");
    assert!(snapshot.contains("eval r"), "{snapshot}");
}

#[test]
fn unsupported_feature_ids_are_machine_readable() {
    let expected = [
        (
            UnsupportedFeature::Generator,
            "E_PHP_IR_UNSUPPORTED_GENERATOR",
        ),
        (
            UnsupportedFeature::YieldFrom,
            "E_PHP_IR_UNSUPPORTED_YIELD_FROM",
        ),
        (UnsupportedFeature::Fiber, "E_PHP_IR_UNSUPPORTED_FIBER"),
        (UnsupportedFeature::Eval, "E_PHP_IR_UNSUPPORTED_EVAL"),
        (
            UnsupportedFeature::Autoload,
            "E_PHP_IR_UNSUPPORTED_AUTOLOAD",
        ),
        (
            UnsupportedFeature::Reflection,
            "E_PHP_IR_UNSUPPORTED_REFLECTION",
        ),
        (
            UnsupportedFeature::TraitRuntime,
            "E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME",
        ),
        (
            UnsupportedFeature::EnumRuntime,
            "E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME",
        ),
        (
            UnsupportedFeature::PropertyHooks,
            "E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS",
        ),
        (
            UnsupportedFeature::FullReferences,
            "E_PHP_IR_UNSUPPORTED_REFERENCE_SEMANTICS",
        ),
    ];

    for (feature, diagnostic_id) in expected {
        assert_eq!(feature.diagnostic_id(), diagnostic_id);
    }
}

#[test]
fn formerly_unsupported_constructs_lower_without_unsupported_diagnostics() {
    let cases = [
        "<?php function gen() { yield from []; }",
        "<?php spl_autoload_register(function ($class) {});",
        "<?php trait T { public function f() {} } class C { use T; }",
        "<?php class C { public string $name { get { return 'x'; } } }",
    ];

    for source in cases {
        let frontend = analyze_source(source);
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(
            result
                .diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.id.starts_with("E_PHP_IR_UNSUPPORTED_")),
            "{source}: {:#?}",
            result.diagnostics
        );
    }
}

#[test]
fn enums_lower_runtime_metadata_and_case_table() {
    let frontend = analyze_source("<?php enum Priority: string { case High = 'H'; }");
    let result = lower_frontend_result(&frontend, LoweringOptions::default());

    assert!(result.verification.is_ok(), "{:#?}", result.verification);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let class = result
        .unit
        .classes
        .iter()
        .find(|class| class.name == "priority")
        .expect("enum class entry");
    assert_eq!(class.display_name, "Priority");
    assert!(class.flags.is_enum);
    assert!(class.flags.is_final);
    assert_eq!(class.enum_backing_type, Some(ClassEnumBackingType::String));
    assert_eq!(class.enum_cases.len(), 1);
    assert_eq!(class.enum_cases[0].name, "High");
    assert!(class.enum_cases[0].value.is_some());
    assert!(class.interfaces.iter().any(|name| name == "unitenum"));
    assert!(class.interfaces.iter().any(|name| name == "backedenum"));
}
